# Jott 0.0.10 — faster, a caret you can find, and markdown that reads like markdown

Four asks in one sentence, so four things changed. The measurements are from this machine (Linux, release
build); Windows with an antivirus in the way will differ in size, not in shape.

**Listing a folder is now ~10× cheaper.** A tree walk hashed every note in it — 34.9 ms for a 2,000-note vault —
and each file was read *twice*: once whole to check it was text, once streamed to hash it. On top of that, every
ingest event asked for the tree again, so one indexing pass paid for the same unchanged bytes three times in half
a second. Now a note is read once, its digest is remembered for the run against its size and timestamp, and the
events coalesce into one re-list per pass: **3.3 ms** for that same folder. What that cache trusts is written in
the code — a file whose timestamp was preserved by whatever wrote it can hold one marker at yesterday's state
until it genuinely changes again. That is acceptable for a dot; §18.13 refuses the same trust for unsaved text,
and the editor still reads and hashes fresh.

**The caret is two pixels of ink.** The window set its own text colour and let CodeMirror pick the caret — one
pixel of CodeMirror's grey, which on a scaled Windows display is not visible. Search box and dialogs get the
same colour for theirs.

**Search marks what it matched.** np sends a snippet with no offsets, so Jott finds the words itself: every word
of your query, in any case, inside longer words too (its keyword scoring hits `kumquat` inside `kumquats`). The
result row shows the marks; clicking it opens the note scrolled to the first one. A hit that came only from the
model's meaning has no matching words, and Jott does not pretend otherwise by jumping to the top of the file.

**Markdown is styled, not rendered.** Headings take size, `**bold**` is bold with its asterisks faded to 42% ink,
inline code gets a tint, links get an underline. Choosing this over a side-by-side preview was a deliberate call:
a preview costs half your typing width and puts a third-party HTML renderer between your notes and your screen
(spec §20 has things to say about that), while the buffer staying *the file's bytes* means nothing can drift out
of sync with the file. Say the word if you want the preview anyway — it is an addition, not a rewrite.

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
