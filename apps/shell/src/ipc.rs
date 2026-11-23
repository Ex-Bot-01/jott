//! The Tauri layer: every command here is a name the webview calls and one call into `jott-core::ipc`.
//!
//! There is deliberately no logic in this file. The whole surface — what an escaping path does, what a save
//! answers with, which connection a search goes to — is decided in the core, where it is tested on any
//! machine (`crates/jott-core/tests/ipc_surface.rs`). This crate needs webkit2gtk or WebView2 to build at
//! all, so the most useful thing it can be is thin: what is untested here is argument passing and event
//! emission, nothing else.

use jott_core::about::AboutInfo;
use jott_core::app_paths::app_data_dir;
use jott_core::sync::guarded;
use jott_core::diagnostics::{Journal, LocalRecord, RunRecord};
use jott_core::history::HistoryStatus;
use jott_core::ingest::consent::np_config_path;
use jott_core::ingest::queue::ConsentOffer;
use jott_core::ipc::{
    self, ConsentAnswerDto, ConsentHelp, Diagnostics, EmbeddingsStatus, IpcError, NpMissingPayload, RerankerStatus,
    NoteContent, RegistryInfo, RenameOutcome, SearchResults, VaultListing, VaultSession, WriteResult,
};
use jott_core::np::client::NpClient;
use jott_core::np::resolve::{current_os, resolve_np, NpSource};
use jott_core::np::NpInfo;
use jott_core::vault::store::{to_abs, Consent, RegisterWarning, VaultStore};
use jott_history::Version;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::broadcast::error::RecvError;

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
    /// True when `data` is a temp folder because the real app data dir was unwritable. The run line says so, and
    /// every session opened from here inherits it — unsaved drafts land wherever Jott could write, and someone
    /// reading the log tomorrow should be able to tell that folder from the one they chose.
    state_fallback: bool,
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
        let (data, state_fallback) =
            app_data_dir().map(|data| (data, false)).unwrap_or_else(|_| (std::env::temp_dir().join("jott"), true));
        let store = VaultStore::new(data.clone());
        let journal = Arc::new(Journal::open(&data));
        App {
            store,
            data,
            np_bundle,
            journal,
            state_fallback,
            np: Mutex::new(None),
            problem: Mutex::new(None),
            session: Mutex::new(None),
            warm: Mutex::new(None),
        }
    }

    fn current(&self) -> Result<Arc<VaultSession>, IpcError> {
        guarded(&self.session).clone().ok_or_else(IpcError::no_vault)
    }

    /// Ask np once per run. A failure is remembered and turned into the fallback session's inputs, so the
    /// window keeps working on the parts of itself that do not need a model (§1).
    async fn np_pair(&self) -> (NpSource, NpInfo) {
        if let Some(pair) = guarded(&self.np).clone() {
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
                    *guarded(&self.warm) = Some(client);
                    (source.clone(), info)
                })
                .map_err(|gone| gone.payload(os)),
            Err(missing) => Err(ipc::np_missing_payload(&missing, os)),
        };
        match outcome {
            Ok(pair) => {
                *guarded(&self.problem) = None;
                *guarded(&self.np) = Some(pair.clone());
                let (source, info) = &pair;
                let _ = self.journal.record_run(&RunRecord::new(
                    env!("CARGO_PKG_VERSION"),
                    Some(info.version.clone()),
                    Some(i64::from(info.protocol)),
                    Some(source.path()),
                    jott_core::about::source_label(source),
                    &self.data,
                    self.state_fallback,
                ));
                pair
            }
            Err(payload) => {
                *guarded(&self.problem) = Some(payload);
                // Written even when nothing answered: a log with no run line for this morning is read as "Jott
                // never ran", which sends someone checking the wrong machine.
                let _ = self.journal.record_run(&RunRecord::new(
                    env!("CARGO_PKG_VERSION"),
                    None,
                    None,
                    None,
                    "not found",
                    &self.data,
                    self.state_fallback,
                ));
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
        //
        // And refuse here, before anything is switched. This line used to end `.ok().flatten()`, which read
        // `NotADirectory` as "no warning" and replaced a working session with one aimed at a folder somebody had
        // already deleted — the window went on drawing the old rows while every command after it answered "is not a
        // folder I can open", until *Open…* picked a folder that exists (issue #2). A pick that cannot be honoured
        // now leaves the vault that was working exactly where it was, and answers with the registry's own sentence.
        let warning = self.store.register(path)?;
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
        if let Some(current) = guarded(&self.session).clone() {
            if to_abs(current.vault()) == to_abs(path) {
                set_window_title(handle, path);
                return Ok(with_nested_warning(current.listing()?, nested));
            }
        }
        let consent = self.store.consent(path);
        let (source, info) = self.np_pair().await;
        // The handshake child, if nobody has used it yet. `None` by the second vault, which spawns its own.
        let warm = guarded(&self.warm).take();
        let session =
            VaultSession::open_in_with(source, info, warm, path, consent, np_config_path(), self.data.clone())
                .with_journal(self.journal.clone());
        bridge(handle, &session);

        let previous = guarded(&self.session).replace(session.clone());
        if let Some(previous) = previous {
            tauri::async_runtime::spawn(async move { previous.close().await });
        }

        // §8.2: a vault inside a vault is legal, and worth one sentence about the second index it means.
        set_window_title(handle, path);
        Ok(with_nested_warning(session.listing()?, nested))
    }
}

/// One line per launch about the window itself, written the moment it appears.
///
/// Nobody who builds Jott can see it: this crate needs WebView2 or webkit even to compile, and no machine in our
/// loop runs one with a display attached. So when somebody reports "it opened white, then moved", the answer has to
/// come out of their log — what size the window came up at, where its top-left landed, and whether there was a saved
/// state file for this folder profile at all. Numbers and one word about a file; no paths, no note text.
///
/// "State file present" is not "the position was restored": the plugin restores only if the label is in that file
/// *and* the saved rectangle still intersects a monitor, so the sentence stops at what can be checked here.
pub fn note_window(handle: &AppHandle, window: &tauri::WebviewWindow) {
    let app = handle.state::<App>();
    // The plugin's own constant, not a string copied from its source: it honours `with_filename`, and a literal
    // here would quietly start reporting "no state file yet" about a file that exists under another name.
    let state_file = handle
        .path()
        .app_config_dir()
        .map(|dir| dir.join(tauri_plugin_window_state::DEFAULT_FILENAME).is_file())
        .unwrap_or(false);
    let saved = if state_file { "state file present" } else { "no state file yet" };
    let detail = match (window.inner_size(), window.outer_position()) {
        (Ok(size), Ok(pos)) => {
            format!("{}x{} with its top-left at +{}+{}; {saved}", size.width, size.height, pos.x, pos.y)
        }
        // Reachable on platforms whose getters refuse; on Windows tao does not return here — `outer_position`
        // panics on a failed `GetWindowRect` (tao-0.37.2 windows/window.rs:177-181). No story is attached to this
        // arm beyond "we could not read it": inventing "the window was mid-move" would be exactly the sort of
        // sentence this log exists to make unnecessary.
        _ => format!("geometry unreadable at show time; {saved}"),
    };
    let _ = app.journal.record_local(&LocalRecord::new("window").with_detail(detail));
}

/// The title says which folder is open.
///
/// A window titled "Jott" is fine until there are two of them, or until the same person works in `work-notes` and
/// `holiday-notes` on alternate days and types a paragraph into the wrong one. The folder name costs nothing and is
/// the only fact that tells those windows apart — in the taskbar too.
fn set_window_title(handle: &AppHandle, path: &Path) {
    use tauri::Manager;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
    if let Some(window) = handle.get_webview_window("main") {
        let _ = window.set_title(&format!("Jott — {name}"));
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
    let watched = session.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => {
                    let _ = sink.emit("ingest://event", &event);
                }
                // A window that fell behind is a window still there. Ending the loop here is what left the
                // status line frozen at whatever it last showed — including through a 295 MB model download
                // that had plenty to say. `Closed` is the only real end.
                Err(error) if ipc::bridge_stays(&error) => {
                    let missed = match &error {
                        tokio::sync::broadcast::error::RecvError::Lagged(n) => *n,
                        _ => 0,
                    };
                    watched.note_events_lagged(missed);
                    continue;
                }
                Err(_) => break,
            }
        }
    });

    let mut changes = session.subscribe_changes();
    let sink = handle.clone();
    tauri::async_runtime::spawn(async move {
        while let Ok(rel) = changes.recv().await {
            let _ = sink.emit("vault://changed", json!({ "rel": rel }));
        }
    });

    // Live backend activity for the diagnostics drawer. Lagging is survivable and said so: a ring that overflowed
    // while the drawer was closed still shows its own history on open, so dropping rows here loses nothing but
    // duplicates. Killing the pump on lag would silently stop the screen mid-run, which is the failure this whole
    // feature exists to prevent.
    let mut rows = session.trace_events();
    let sink = handle.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            match rows.recv().await {
                Ok(row) => {
                    let _ = sink.emit("trace://row", &row);
                }
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => break,
            }
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
    let problem = { guarded(&app.problem).clone() };
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
    guarded(&app.problem).clone()
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
    let problem = guarded(&app.problem).clone();
    Ok(jott_core::about::about(
        env!("CARGO_PKG_VERSION"),
        Some(&source),
        Some(&info),
        problem.as_ref(),
        &app.data,
        &app.journal.path(),
    ))
}

/// What the diagnostics drawer opens with. Built from memory and a warm digest cache, so opening it never waits
/// on np — and never starts it (§4: the screen people reach for when the backend is unwell must not itself need
/// the backend).
#[tauri::command]
pub async fn diagnostics(app: State<'_, App>) -> Result<Diagnostics, IpcError> {
    Ok(app.current()?.diagnostics().await)
}

// ---- versions and sync: each one call into `VaultSession`, which runs git on a blocking thread ----

#[tauri::command]
pub async fn history_status(app: State<'_, App>) -> Result<HistoryStatus, IpcError> {
    app.current()?.history_status().await
}

#[tauri::command]
pub async fn history_check_git(app: State<'_, App>) -> Result<HistoryStatus, IpcError> {
    app.current()?.history_check_git().await
}

#[tauri::command]
pub async fn history_set_git_path(path: Option<String>, app: State<'_, App>) -> Result<HistoryStatus, IpcError> {
    app.current()?.history_set_git_path(path).await
}

#[tauri::command]
pub async fn history_enable(app: State<'_, App>) -> Result<HistoryStatus, IpcError> {
    app.current()?.history_enable().await
}

#[tauri::command]
pub async fn history_disable(app: State<'_, App>) -> Result<HistoryStatus, IpcError> {
    app.current()?.history_disable().await
}

#[tauri::command]
pub async fn history_set_identity(name: String, email: String, app: State<'_, App>) -> Result<HistoryStatus, IpcError> {
    app.current()?.history_set_identity(name, email).await
}

#[tauri::command]
pub async fn history_set_options(
    quiet_secs: u64,
    sync_on_open_close: bool,
    app: State<'_, App>,
) -> Result<HistoryStatus, IpcError> {
    app.current()?.history_set_options(quiet_secs, sync_on_open_close).await
}

#[tauri::command]
pub async fn history_version_now(app: State<'_, App>) -> Result<HistoryStatus, IpcError> {
    app.current()?.history_version_now().await
}

#[tauri::command]
pub async fn history_versions(rel: String, app: State<'_, App>) -> Result<Vec<Version>, IpcError> {
    app.current()?.history_versions(rel).await
}

#[tauri::command]
pub async fn history_version_text(id: String, rel: String, app: State<'_, App>) -> Result<String, IpcError> {
    app.current()?.history_version_text(id, rel).await
}

#[tauri::command]
pub async fn history_restore(
    rel: String,
    id: String,
    version_rel: String,
    app: State<'_, App>,
) -> Result<WriteResult, IpcError> {
    app.current()?.history_restore(rel, id, version_rel).await
}

#[tauri::command]
pub async fn history_connect(url: String, app: State<'_, App>) -> Result<HistoryStatus, IpcError> {
    app.current()?.history_connect(url).await
}

#[tauri::command]
pub async fn history_disconnect(app: State<'_, App>) -> Result<HistoryStatus, IpcError> {
    app.current()?.history_disconnect().await
}

#[tauri::command]
pub async fn history_sync(app: State<'_, App>) -> Result<HistoryStatus, IpcError> {
    app.current()?.history_sync().await
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

/// The one command whose cost grows with the size of the vault rather than with what the person just did: a
/// rename opens every note to answer "what refers to this?" (§18.7) before it moves one file.
///
/// A non-`async` command is resolved inline wherever the IPC message was handled — `tauri-macros` wraps it with
/// `body_blocking`, which runs the body on the calling context rather than handing it to the async runtime — so
/// until now that scan ran on the thread that paints, with nothing on screen to say work was happening. Async plus
/// `spawn_blocking` puts it on the blocking pool: the window keeps painting, and a vault of a few thousand notes
/// (plus whatever Windows' antivirus does per opened file) no longer freezes it.
///
/// What the front end depends on is unchanged: the reply still arrives after the move, and the watcher-driven
/// refresh still follows it. `save_note`, `create_note` and `delete_entry` stay synchronous deliberately — each
/// touches one file, so moving them would add a handoff and buy nothing.
#[tauri::command]
pub async fn rename_entry(from: String, to: String, app: State<'_, App>) -> Result<RenameOutcome, IpcError> {
    // The session is taken out here rather than inside the closure: `State` borrows the managed value for the
    // request's lifetime, and a blocking task must own everything it touches. `Arc` is what makes that cheap.
    let session = app.current()?;
    // Kept for the refusal path: `to` moves into the blocking closure, and a join failure still has to name the
    // note the person asked about. (CI found this: borrowing `to` after the move is E0382, and nothing on this box
    // can compile the shell to tell me before push.)
    let asked_for = to.clone();
    tauri::async_runtime::spawn_blocking(move || session.rename_entry(&from, &to))
        .await
        .map_err(|e| IpcError::io(&asked_for, e))?
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
pub async fn search_vault(
    query: String,
    limit: u8,
    prefix: bool,
    mode: String,
    rerank: bool,
    app: State<'_, App>,
) -> Result<SearchResults, IpcError> {
    // The label is checked in core rather than here: the verdict and the request belong to the same code, and this
    // file's job stays "carry it".
    app.current()?.search(&query, limit, prefix, &mode, rerank).await
}

/// The ranking model's standing for the open folder: can this np rerank, are the bytes here, is a download running,
/// and where to get the file by hand.
#[tauri::command]
pub async fn reranker_status(app: State<'_, App>) -> Result<RerankerStatus, IpcError> {
    Ok(app.current()?.reranker_status().await)
}

/// Start downloading the ranking model if np says it is missing. Answers at once; the download runs on.
#[tauri::command]
pub async fn prepare_reranker(app: State<'_, App>) -> Result<RerankerStatus, IpcError> {
    Ok(app.current()?.prepare_reranker().await)
}

/// Hand np a ranking model the person downloaded themselves; np checks it against the pinned digest.
#[tauri::command]
pub async fn install_reranker(path: String, app: State<'_, App>) -> Result<RerankerStatus, IpcError> {
    Ok(app.current()?.install_reranker(Path::new(&path)).await)
}

/// How many top results the ranking model reorders — np's `[search] rerank_top_n`.
#[tauri::command]
pub fn set_rerank_top_n(n: i64, app: State<'_, App>) -> Result<(), IpcError> {
    app.current()?.set_rerank_top_n(n)
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

/// What the consent card should say: which np setting is still short of vectors, and the lines that would close
/// it. Read from np's config here rather than reasoned about in the webview — the file is the fact, and a second
/// copy of the rule in TypeScript is a second place to be wrong.
#[tauri::command]
pub fn embeddings_help(app: State<'_, App>) -> Result<ConsentHelp, IpcError> {
    Ok(app.current()?.embeddings_help())
}

/// The status line's model dot: configured, downloaded, and whether this folder uses it — three questions, three
/// different sources, all of them note-processor's answers rather than the webview's guesses.
#[tauri::command]
pub async fn embeddings_status(app: State<'_, App>) -> Result<EmbeddingsStatus, IpcError> {
    Ok(app.current()?.embeddings_status().await)
}

/// The consent prompt's two buttons. The queue writes np's config; the registry remembers the answer for
/// the next launch (§8, §12).
#[tauri::command]
pub async fn set_consent(granted: bool, app: State<'_, App>) -> Result<ConsentAnswerDto, IpcError> {
    let session = app.current()?;
    // The answer goes to np over the pipe, and what np said comes back to the window: the card quotes it rather than
    // describing the file from memory.
    let receipt = session.answer_consent(granted).await;
    app.store
        .set_consent(session.vault(), if granted { Consent::Granted } else { Consent::Declined })
        // The registry's sentence, unchanged: it already names the folder and what is wrong with it.
        .map_err(IpcError::from)?;
    Ok(ConsentAnswerDto::from(receipt))
}

/// Asked when the window closes, so a save that was in the queue gets its pass (§11). Bounded by the
/// queue's own cap: closing never waits for a model.
///
/// This is the webview's door, and since issue #1 it is no longer the only one: `close_for_quit` below runs the
/// same core body from the event loop, which is what actually happens when somebody closes the window. The two
/// share one implementation precisely so the copy in this file cannot drift from the one the headless suite covers.
#[tauri::command]
pub async fn flush_on_close(app: State<'_, App>) -> Result<(), IpcError> {
    // Taken out of the mutex first: a guard held across an `await` would sit on the whole shell while np
    // finishes what it was asked to do. Taken rather than cloned because this means *leaving* — a second
    // close-out after this one has nothing left to ask.
    let session = { guarded(&app.session).take() };
    let warm = { guarded(&app.warm).take() };
    jott_core::ipc::flush_on_quit(session, warm).await;
    Ok(())
}

/// The quit path: §11's close-out run from the shell's own event loop, where there is a thread to wait on.
///
/// Called from `RunEvent::ExitRequested`, which Tauri emits when the last window is destroyed and then waits for
/// this callback to return before it leaves the loop (`tauri-runtime-wry-2.12.3/src/lib.rs:4255-4270`). That is
/// what makes it the only reliable place for this: an `invoke` fired from a webview unload hook may never reach
/// Rust at all, which is why `Editor.svelte`'s `pagehide` handler saves through `save_note` and nothing ever
/// promised more than that (§11).
///
/// Bounded by what it awaits, not by a timer of its own: the queue's cap for a pass in flight, two seconds on the
/// pool, 500 ms on an unclaimed child. A stuck np can slow somebody's quit by those few seconds and no further —
/// pinned by `quitting_never_waits_for_a_model_that_is_still_thinking`.
pub fn close_for_quit(handle: &AppHandle) {
    let Some(app) = handle.try_state::<App>() else { return };
    // Take, don't clone: whatever is left to shut down is shut down once. A second call — a restart, say, which
    // asks for exit again — finds both slots empty and returns without asking anything of anything.
    let session = guarded(&app.session).take();
    let warm = guarded(&app.warm).take();
    if session.is_none() && warm.is_none() {
        return;
    }
    // Written before the wait, not after: if the machine pulls the plug mid-drain, the log at least says Jott was
    // asked to leave and what it still held.
    let _ = app.journal.record_local(
        &LocalRecord::new("quit").with_detail(format!(
            "close-out: vault {}, np child {}",
            if session.is_some() { "open" } else { "none" },
            if warm.is_some() { "warm" } else { "none" }
        )),
    );
    tauri::async_runtime::block_on(jott_core::ipc::flush_on_quit(session, warm));
}

// The commands above are the whole surface, and `main.rs` registers them by name. `src/lib/ipc.ts` is the
// other half of that list: a name added on one side only shows up as `Command not found` in the window, so
// keep the two in step — the payload *shapes* they share are pinned by `crates/jott-core/tests/ipc_surface.rs`.
