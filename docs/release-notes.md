# Jott 0.0.8 — one np per run, and folders let go when you close them

Asked whether np starts when Jott starts. It did — for about two milliseconds: the app started np, asked it what
version it was, **killed it**, and started a second one when you opened a folder. That child is now kept and
becomes the connection that does the work, so np runs from startup to shutdown instead of dying between the
question and the answer.

Measured against the real np 0.3.1 rather than guessed: a spawn plus `initialize` is **~2 ms**, and an idle child
is **~8 MB**. So this is not a speed-up, and starting a *second* np for search before anyone has typed in the
search box would be 8 MB for a maybe. The search connection still starts on first use — and then stays.

**The more important fix, found while measuring:** closing a vault only shut its np children down if an indexing
pass happened to run. Open a folder, search once, open another folder — and the first one's np processes stayed
alive holding that index open, every time. On Windows that is exactly the write-lock friction this project has
been careful about, and after quitting it would have been orphaned processes. Closing a session now always shuts
its pool, and quitting shuts a startup child that never got claimed by a folder.

**Assets**

| File | For |
| --- | --- |
| `jott-windows-x64.msi` | Windows 10/11, x64. This is the one to test: Jott is built for Windows first. |
| `jott-linux-x64.deb` | Debian/Ubuntu, x64 — a convenience for whoever maintains this. |
| `jott-linux-x64.AppImage` | Same, on anything else Linux. |
| `SHA256SUMS.txt` | Checksums of the three above. |

np 0.3.1 is bundled: fetched from its release and checked against a hash pinned in this repository
(`packaging/np-pin.json`) before it goes near an installer. `NP_PATH` outranks the bundled copy on purpose, which
is exactly the case About now makes visible — it says `bundled`, `$NP_PATH` or `$PATH` beside the path.

0.0.3 was unreadable on a Windows machine set to dark mode: white writing on near-white paper, which looks
exactly like an empty window. That is fixed here, along with the blank-window change that turned out to carry
its own risk. **np is still bundled**, so installing remains one download.

**Assets**

| File | For |
| --- | --- |
| `jott-windows-x64.msi` | Windows 10/11, x64. This is the one to test: Jott is built for Windows first. |
| `jott-linux-x64.deb` | Debian/Ubuntu, x64 — a convenience for whoever maintains this. |
| `jott-linux-x64.AppImage` | Same, on anything else Linux. |
| `SHA256SUMS.txt` | Checksums of the three above. |

The bundled np is 0.3.1, fetched from its release and checked against a hash pinned in this repository
(`packaging/np-pin.json`) before it goes anywhere near an installer. Prefer your own build? Set `NP_PATH` and
Jott will use it — an override you choose outranks the copy we ship.

**Install it**

Uninstall any earlier Jott first: the bundle identifier changed while fixing a Tauri warning, so a new `.msi`
installs beside an old one rather than replacing it. Windows will warn that the publisher is unknown — true:
this build is unsigned, and signing is separate work (§20). *More info → Run anyway* is the expected path. The
model for semantic search is **not** bundled; the first time something needs it, Jott names the model and the
folder and asks. Saying no leaves indexing and keyword search working.

**What changed since 0.0.4**

- **About panel** (`⚙`-equivalent, labelled *About* in the header): both versions, which of the three places the
  np binary came from and where it sits, the data folder, the log file, and links out. The wording of the copied
  block is built in Rust and has tests, so what you paste is what the tests pinned. A webview that refuses
  clipboard writes shows the text selected in a box instead of failing silently.
- Links open in your browser rather than navigating the app away, via Tauri's opener plugin — the one new
  dependency, and the panel's three permissions are `opener:default` and nothing else.

**What changed since 0.0.3**

- **Ink and paper are both named, in light and dark.** The stylesheet said `color-scheme: light dark` — "this
  page handles dark" — and then named neither colour, so WebView2 chose white text while the startup skeleton
  painted near-white underneath. A test now refuses that shape: any file opting into dark mode must say what
  its ink is ([`src/palette.test.ts`](https://github.com/Ex-Bot-01/jott/blob/main/src/palette.test.ts)).
- **The editor takes the palette.** CodeMirror's default surface is a white rectangle; it now uses the same
  paper and ink as the rest of the window, with the current line tinted from the ink rather than a fixed grey.
- **The window opens visible again**, painting the app's shape from HTML before any script runs. 0.0.3 hid it
  until the front end reported a paint, which is one more way to end up staring at nothing; a drawn skeleton
  that says *Jott is starting…* tells you more than an invisible window. Devtools are on in this build
  (`Ctrl+Shift+I`), so if something looks wrong you can read the console instead of describing the colour of
  the screen.
- **Two tests where there were none.** The app's real entry point is now mounted in a jsdom document — the
  white-screen class of bug at least announces itself as a failure now — and the palette rule above is pinned.

**Uninstalling leaves `%APPDATA%\jott` behind, on purpose.** That folder can hold a mirrored copy of text you
never saved and notes Jott moved to its own trash when Windows offered none. Deleting writing on uninstall is
worse than leaving a folder; [`docs/testing-on-windows.md`](https://github.com/Ex-Bot-01/jott/blob/main/docs/testing-on-windows.md)
names both folders and how to clear them when you are sure.

**What this version deliberately does not do**

- No search overlay yet — a field in the sidebar and plain results; no ⌘K palette, no keyboard shortcuts.
- Renaming reports which notes link to the one you renamed and rewrites nothing; that needs our own link index.
- macOS is not built, and there is no np asset for it either. Syntax colours in dark mode are CodeMirror's
  defaults, not a designed dark theme.

Built by `build.yml` from the tag this release is attached to; the run's log shows which np was bundled and
every test that gated these files.
