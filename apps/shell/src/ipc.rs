//! The Tauri layer: every command here is a name the webview calls and one call into `jott-core::ipc`.
//!
//! There is deliberately no logic in this file. The whole surface — what an escaping path does, what a save
//! answers with, which connection a search goes to — is decided in the core, where it is tested on any
//! machine (`crates/jott-core/tests/ipc_surface.rs`). This crate needs webkit2gtk or WebView2 to build at
//! all, so the most useful thing it can be is thin: what is untested here is argument passing and event
//! emission, nothing else.

use jott_core::about::AboutInfo;
use jott_core::app_paths::app_data_dir;
use jott_core::diagnostics::Journal;
use jott_core::ingest::consent::np_config_path;
use jott_core::ingest::queue::ConsentOffer;
use jott_core::ipc::{
    self, IpcError, NpMissingPayload, NoteContent, RegistryInfo, RenameOutcome, SearchResults, VaultListing, VaultSession,
    WriteResult,
};
use jott_core::np::client::NpClient;
use jott_core::np::resolve::{current_os, resolve_np, NpSource};
use jott_core::np::NpInfo;
use jott_core::vault::store::{to_abs, Consent, RegisterWarning, VaultStore};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

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
    /// The folder holding the np we ship inside the installer (`$RESOURCES/np`). Empty when there is none —
    /// a source build without `scripts/fetch-np` — and resolution then falls through to PATH as it always did.
    np_bundle: PathBuf,
    /// The child the startup handshake started, kept for the first vault to use instead of being killed and
    /// started again. Taken when a session opens; shut down at quit if no vault ever claimed it, because an np
    /// left running after Jott exits keeps its index open on Windows.
    warm: Mutex<Option<Arc<NpClient>>>,
}

impl App {
    /// Open what the app knows about at startup. A registry that cannot be read is not a reason not to
    /// show a window — Task 6 quarantines the broken file and starts empty.
    pub fn startup(np_bundle: PathBuf) -> App {
        let data = app_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("jott"));
        let store = VaultStore::new(data.clone());
        let journal = Arc::new(Journal::open(&data));
        App {
            store,
            data,
            np_bundle,
            journal,
            np: Mutex::new(None),
            problem: Mutex::new(None),
            session: Mutex::new(None),
            warm: Mutex::new(None),
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
        // The shipped copy first: `resolve_np` looks in this folder before it looks at PATH, so a Jott
        // install speaks to the np whose protocol pair it was built and tested against. `NP_PATH` still
        // outranks both — see §14 as amended.
        let lookup = if self.np_bundle.as_os_str().is_empty() { &self.data } else { &self.np_bundle };
        let outcome = match resolve_np(lookup) {
            // The handshake child is kept, not killed: the first vault opened uses it as its writer. An idle
            // np costs about 8 MB and a spawn-and-handshake is about 2 ms, so this is not a speed-up — it is
            // one process that stops existing purely to answer one question.
            Ok(source) => ipc::read_np_info_warm(&source)
                .await
                .map(|(client, info)| {
                    *self.warm.lock().unwrap() = Some(client);
                    (source.clone(), info)
                })
                .map_err(|gone| gone.payload(os)),
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
        // Register first, always: it is what notices a folder sitting inside another np vault, and that
        // sentence belongs in the listing a re-pick returns as much as in the first one (§8.2).
        let warning = self.store.register(path).ok().flatten();
        let nested = match warning {
            Some(RegisterWarning::NestedVault { parent }) => Some(format!(
                "{} is inside another np vault ({}) — indexing here builds a second index",
                path.display(),
                parent.display()
            )),
            None => None,
        };

        // The same folder again (a picker re-picked, a recent clicked twice) is the same session: two
        // queues on one workspace would race over np's lock and both write the same stamp file, so this is
        // not a nicety. It does not re-detect np — the session speaks to the child it was opened with, and
        // installing np mid-session is what `np_problem` and a reopen are for.
        if let Some(current) = self.session.lock().unwrap().clone() {
            if to_abs(current.vault()) == to_abs(path) {
                return Ok(with_nested_warning(current.listing()?, nested));
            }
        }
        let consent = self.store.consent(path);
        let (source, info) = self.np_pair().await;
        // The handshake child, if nobody has used it yet. `None` by the second vault, which spawns its own.
        let warm = self.warm.lock().unwrap().take();
        let session =
            VaultSession::open_in_with(source, info, warm, path, consent, np_config_path(), self.data.clone())
                .with_journal(self.journal.clone());
        bridge(handle, &session);

        let previous = self.session.lock().unwrap().replace(session.clone());
        if let Some(previous) = previous {
            tauri::async_runtime::spawn(async move { previous.close().await });
        }

        // §8.2: a vault inside a vault is legal, and worth one sentence about the second index it means.
        Ok(with_nested_warning(session.listing()?, nested))
    }
}

/// One sentence, added once. The listing already carries whatever the walk itself had to complain about.
fn with_nested_warning(mut listing: VaultListing, nested: Option<String>) -> VaultListing {
    if let Some(sentence) = nested {
        if !listing.warnings.contains(&sentence) {
            listing.warnings.push(sentence);
        }
    }
    listing
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
    // Read it and put the guard down in one statement. Written as `if let Some(p) = self.lock().clone()`,
    // the guard is a temporary that outlives the block, and its borrow of the managed state outlives `app` —
    // which only shows up when something actually compiles this file.
    let problem = { app.problem.lock().unwrap().clone() };
    if let Some(payload) = problem {
        let _ = handle.emit("np://missing", payload);
    }
}

#[tauri::command]
pub fn registry_info(app: State<'_, App>) -> RegistryInfo {
    let recent = app.store.load().into_iter().map(|entry| entry.path.display().to_string()).collect();
    let consent = app.store.last_opened().map(|path| app.store.consent(&path)).unwrap_or(Consent::Undecided);
    RegistryInfo { recent, consent }
}

#[tauri::command]
pub fn np_status(app: State<'_, App>) -> Option<NpMissingPayload> {
    app.problem.lock().unwrap().clone()
}

/// What the About panel shows: both versions, which np file this run is actually talking to, and where Jott
/// keeps its files. The payload — including the block of text behind "Copy details" — is built in the core
/// (`jott_core::about`), where its wording has tests; there is nothing to decide here but which facts to hand
/// over. `np_pair` asks once per run and remembers, so opening the panel does not start another child.
///
/// Tauri insists that an async command taking a reference answers with a `Result`, so an error type comes
/// along for the ride even though nothing here can fail: every fact is already in memory once `np_pair` has
/// asked, and a panel that could not open would be a worse lie than one showing "not asked yet".
#[tauri::command]
pub async fn app_info(app: State<'_, App>) -> Result<AboutInfo, IpcError> {
    let (source, info) = app.np_pair().await;
    let problem = app.problem.lock().unwrap().clone();
    Ok(jott_core::about::about(
        env!("CARGO_PKG_VERSION"),
        Some(&source),
        Some(&info),
        problem.as_ref(),
        &app.data,
        &app.journal.path(),
    ))
}

#[tauri::command]
pub fn list_vault(app: State<'_, App>) -> Result<VaultListing, IpcError> {
    app.current()?.listing()
}

#[tauri::command]
pub async fn open_vault(path: String, handle: AppHandle, app: State<'_, App>) -> Result<VaultListing, IpcError> {
    app.open(&handle, Path::new(&path)).await
}

#[tauri::command]
pub fn read_note(rel: String, app: State<'_, App>) -> Result<NoteContent, IpcError> {
    app.current()?.read_note(&rel)
}

#[tauri::command]
pub fn save_note(rel: String, text: String, app: State<'_, App>) -> Result<WriteResult, IpcError> {
    app.current()?.save_note(&rel, &text)
}

#[tauri::command]
pub fn create_note(rel: String, app: State<'_, App>) -> Result<WriteResult, IpcError> {
    app.current()?.create_note(&rel)
}

#[tauri::command]
pub fn create_folder(rel: String, app: State<'_, App>) -> Result<(), IpcError> {
    app.current()?.create_folder(&rel)
}

#[tauri::command]
pub fn rename_entry(from: String, to: String, app: State<'_, App>) -> Result<RenameOutcome, IpcError> {
    app.current()?.rename_entry(&from, &to)
}

#[tauri::command]
pub fn move_entry(from: String, to: String, app: State<'_, App>) -> Result<(), IpcError> {
    app.current()?.move_entry(&from, &to)
}

#[tauri::command]
pub fn delete_entry(rel: String, app: State<'_, App>) -> Result<String, IpcError> {
    app.current()?.delete_entry(&rel)
}

#[tauri::command]
pub fn mirror_buffer(rel: String, text: String, app: State<'_, App>) -> Result<(), IpcError> {
    app.current()?.mirror_buffer(&rel, &text)
}

#[tauri::command]
pub fn discard_mirror(rel: String, app: State<'_, App>) -> Result<(), IpcError> {
    app.current()?.discard_mirror(&rel)
}

#[tauri::command]
pub async fn search_vault(query: String, limit: u8, app: State<'_, App>) -> Result<SearchResults, IpcError> {
    app.current()?.search(&query, limit).await
}

#[tauri::command]
pub fn ingest_all(app: State<'_, App>) -> Result<(), IpcError> {
    app.current()?.ingest_all();
    Ok(())
}

/// "Is setting embeddings up still on the table?" The window asks when it wants to offer the question again
/// after an earlier no, and gets `null` when there is nothing to offer — either the bytes are already on disk
/// or np was already told yes. Asked here rather than guessed in the webview because only the queue can ask
/// np what it actually has.
#[tauri::command]
pub async fn embeddings_offer(app: State<'_, App>) -> Result<Option<ConsentOffer>, IpcError> {
    Ok(Some(app.current()?.embeddings_offer().await).flatten())
}

/// The consent prompt's two buttons. The queue writes np's config; the registry remembers the answer for
/// the next launch (§8, §12).
#[tauri::command]
pub fn set_consent(granted: bool, app: State<'_, App>) -> Result<(), IpcError> {
    let session = app.current()?;
    session.set_consent(granted);
    app.store
        .set_consent(session.vault(), if granted { Consent::Granted } else { Consent::Declined })
        .map_err(|e| IpcError::io("", e.to_string()))
}

/// Asked when the window closes, so a save that was in the queue gets its pass (§11). Bounded by the
/// queue's own cap: closing never waits for a model.
#[tauri::command]
pub async fn flush_on_close(app: State<'_, App>) -> Result<(), IpcError> {
    // Taken out of the mutex first: a guard held across an `await` would sit on the whole shell while np
    // finishes what it was asked to do.
    let session = { app.session.lock().unwrap().clone() };
    if let Some(session) = session {
        session.close().await;
    }
    // A warm child nobody claimed — the window opened, and closed, without ever opening a folder. It deserves
    // the same ask; an np still running after Jott quits holds its index open on Windows.
    let warm = { app.warm.lock().unwrap().take() };
    if let Some(client) = warm {
        let _ = client.shutdown_within(std::time::Duration::from_millis(500)).await;
    }
    Ok(())
}

// The commands above are the whole surface, and `main.rs` registers them by name. `src/lib/ipc.ts` is the
// other half of that list: a name added on one side only shows up as `Command not found` in the window, so
// keep the two in step — the payload *shapes* they share are pinned by `crates/jott-core/tests/ipc_surface.rs`.
