//! The Tauri layer: every command here is a name the webview calls and one call into `jott-core::ipc`.
//!
//! There is deliberately no logic in this file. The whole surface — what an escaping path does, what a save
//! answers with, which connection a search goes to — is decided in the core, where it is tested on any
//! machine (`crates/jott-core/tests/ipc_surface.rs`). This crate needs webkit2gtk or WebView2 to build at
//! all, so the most useful thing it can be is thin: what is untested here is argument passing and event
//! emission, nothing else.

use jott_core::app_paths::app_data_dir;
use jott_core::diagnostics::Journal;
use jott_core::ingest::consent::np_config_path;
use jott_core::ipc::{
    self, IpcError, NpMissingPayload, NoteContent, RegistryInfo, RenameOutcome, SearchResults, VaultListing, WriteResult,
};
use jott_core::np::resolve::{current_os, resolve_np, NpSource};
use jott_core::np::NpInfo;
use jott_core::vault::store::{to_abs, Consent, RegisterWarning, VaultStore};
use jott_core::vault::VaultSession;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

pub struct App {
    /// The registry: recents and the consent decision, in the app data dir (§12).
    store: VaultStore,
    /// Where per-vault state lives, handed to every session so nothing reads the environment later.
    data: PathBuf,
    /// np, once it has answered the handshake. `None` means nobody has asked yet or nobody could reach it.
    np: Mutex<Option<(NpSource, NpInfo)>>,
    /// Why np could not be reached, kept for the window to ask about (`np_status`) as well as be told
    /// (`np://missing`). An event emitted before the webview has listeners is a message into an empty room.
    problem: Mutex<Option<NpMissingPayload>>,
    session: Mutex<Option<Arc<VaultSession>>>,
    /// `<app_data>/logs/jott.jsonl` — the file §19's hand-off asks for. One per run, shared by every vault
    /// opened in it, because "what happened this afternoon" is the question it answers.
    journal: Arc<Journal>,
}

impl App {
    /// Open what the app knows about at startup. A registry that cannot be read is not a reason not to
    /// show a window — Task 6 quarantines the broken file and starts empty.
    pub fn startup() -> App {
        let data = app_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("jott"));
        let store = VaultStore::new(data.clone());
        let journal = Arc::new(Journal::open(&data));
        App {
            data,
            journal,
            np: Mutex::new(None),
            problem: Mutex::new(None),
            session: Mutex::new(None),
        }
    }

    fn current(&self) -> Result<Arc<VaultSession>, IpcError> {
        self.session.lock().unwrap().clone().ok_or_else(IpcError::no_vault)
    }

    /// Ask np once per run. A failure is remembered and turned into the fallback session's inputs, so the
    /// window keeps working on the parts of itself that do not need a model (§1).
    async fn np_pair(&self) -> (NpSource, NpInfo) {
        if let Some(pair) = self.np.lock().unwrap().clone() {
            return pair;
        }
        let os = current_os();
        let outcome = match resolve_np(&self.data) {
            Ok(source) => ipc::read_np_info(&source).await.map(|info| (source.clone(), info)).map_err(|gone| gone.payload(os)),
            Err(missing) => Err(ipc::np_missing_payload(&missing, os)),
        };
        match outcome {
            Ok(pair) => {
                *self.problem.lock().unwrap() = None;
                *self.np.lock().unwrap() = Some(pair.clone());
                pair
            }
            Err(payload) => {
                *self.problem.lock().unwrap() = Some(payload);
                ipc::np_absent()
            }
        }
    }

    /// Register the folder, remember what was decided about embedding, and start its session. The previous
    /// vault's session is closed on a task of its own: it drains its ingest queue up to its cap (§11), and
    /// switching folders should not wait on that.
    async fn open(&self, handle: &AppHandle, path: &Path) -> Result<VaultListing, IpcError> {
        // The same folder again (a picker re-picked, a recent clicked twice) is the same session. Two
        // queues on one workspace would race over np's lock and both write the same stamp file, so this is
        // not a nicety.
        if let Some(current) = self.session.lock().unwrap().clone() {
            if to_abs(current.vault()) == to_abs(path) {
                return current.listing();
            }
        }
        let warning = self.store.register(path).ok().flatten();
        let consent = self.store.consent(path);
        let (source, info) = self.np_pair().await;
        let session = VaultSession::open_in(source, info, path, consent, np_config_path(), self.data.clone())
            .with_journal(self.journal.clone());
        bridge(handle, &session);

        let previous = self.session.lock().unwrap().replace(session.clone());
        if let Some(previous) = previous {
            tauri::async_runtime::spawn(async move { previous.close().await });
        }

        let mut listing = session.listing()?;
        if let Some(RegisterWarning::NestedVault { parent }) = warning {
            // §8.2: a vault inside a vault is legal, and worth one sentence about the second index it means.
            listing.warnings.push(format!(
                "{} is inside another np vault ({}) — indexing here builds a second index",
                path.display(),
                parent.display()
            ));
        }
        Ok(listing)
    }
}

/// Forward the two streams the window listens to. Each ends by itself when its session is dropped, because
/// a broadcast channel with no sender is a closed one — no bookkeeping to forget.
fn bridge(handle: &AppHandle, session: &Arc<VaultSession>) {
    let mut events = session.subscribe_events();
    let sink = handle.clone();
    tauri::async_runtime::spawn(async move {
        while let Ok(event) = events.recv().await {
            let _ = sink.emit("ingest://event", &event);
        }
    });

    let mut changes = session.subscribe_changes();
    let sink = handle.clone();
    tauri::async_runtime::spawn(async move {
        while let Ok(rel) = changes.recv().await {
            let _ = sink.emit("vault://changed", json!({ "rel": rel }));
        }
    });
}

/// Say what is wrong with np, if anything is. Called at startup and asked about by the window as well.
pub async fn announce_np(handle: &AppHandle) {
    let app = handle.state::<App>();
    let _ = app.np_pair().await;
    if let Some(payload) = app.problem.lock().unwrap().clone() {
        let _ = handle.emit("np://missing", payload);
    }
}

#[tauri::command]
fn registry_info(app: State<'_, App>) -> RegistryInfo {
    let recent = app.store.load().into_iter().map(|entry| entry.path.display().to_string()).collect();
    let consent = app.store.last_opened().map(|path| app.store.consent(&path)).unwrap_or(Consent::Undecided);
    RegistryInfo { recent, consent }
}

#[tauri::command]
fn np_status(app: State<'_, App>) -> Option<NpMissingPayload> {
    app.problem.lock().unwrap().clone()
}

#[tauri::command]
fn list_vault(app: State<'_, App>) -> Result<VaultListing, IpcError> {
    app.current()?.listing()
}

#[tauri::command]
async fn open_vault(path: String, handle: AppHandle, app: State<'_, App>) -> Result<VaultListing, IpcError> {
    app.open(&handle, Path::new(&path)).await
}

#[tauri::command]
fn read_note(rel: String, app: State<'_, App>) -> Result<NoteContent, IpcError> {
    app.current()?.read_note(&rel)
}

#[tauri::command]
fn save_note(rel: String, text: String, app: State<'_, App>) -> Result<WriteResult, IpcError> {
    app.current()?.save_note(&rel, &text)
}

#[tauri::command]
fn create_note(rel: String, app: State<'_, App>) -> Result<WriteResult, IpcError> {
    app.current()?.create_note(&rel)
}

#[tauri::command]
fn create_folder(rel: String, app: State<'_, App>) -> Result<(), IpcError> {
    app.current()?.create_folder(&rel)
}

#[tauri::command]
fn rename_entry(from: String, to: String, app: State<'_, App>) -> Result<RenameOutcome, IpcError> {
    app.current()?.rename_entry(&from, &to)
}

#[tauri::command]
fn move_entry(from: String, to: String, app: State<'_, App>) -> Result<(), IpcError> {
    app.current()?.move_entry(&from, &to)
}

#[tauri::command]
fn delete_entry(rel: String, app: State<'_, App>) -> Result<String, IpcError> {
    app.current()?.delete_entry(&rel)
}

#[tauri::command]
fn mirror_buffer(rel: String, text: String, app: State<'_, App>) -> Result<(), IpcError> {
    app.current()?.mirror_buffer(&rel, &text)
}

#[tauri::command]
fn discard_mirror(rel: String, app: State<'_, App>) -> Result<(), IpcError> {
    app.current()?.discard_mirror(&rel)
}

#[tauri::command]
async fn search_vault(query: String, limit: u8, app: State<'_, App>) -> Result<SearchResults, IpcError> {
    app.current()?.search(&query, limit).await
}

#[tauri::command]
fn ingest_all(app: State<'_, App>) -> Result<(), IpcError> {
    app.current()?.ingest_all();
    Ok(())
}

/// The consent prompt's two buttons. The queue writes np's config; the registry remembers the answer for
/// the next launch (§8, §12).
#[tauri::command]
fn set_consent(granted: bool, app: State<'_, App>) -> Result<(), IpcError> {
    let session = app.current()?;
    session.set_consent(granted);
    app.store
        .set_consent(session.vault(), if granted { Consent::Granted } else { Consent::Declined })
        .map_err(|e| IpcError::io("", e.to_string()))
}

/// Asked when the window closes, so a save that was in the queue gets its pass (§11). Bounded by the
/// queue's own cap: closing never waits for a model.
#[tauri::command]
async fn flush_on_close(app: State<'_, App>) -> Result<(), IpcError> {
    // Taken out of the mutex first: a guard held across an `await` would sit on the whole shell while np
    // finishes what it was asked to do.
    let session = { app.session.lock().unwrap().clone() };
    if let Some(session) = session {
        session.close().await;
    }
    Ok(())
}

// The commands above are the whole surface, and `main.rs` registers them by name. `src/lib/ipc.ts` is the
// other half of that list: a name added on one side only shows up as `Command not found` in the window, so
// keep the two in step — the payload *shapes* they share are pinned by `crates/jott-core/tests/ipc_surface.rs`.
