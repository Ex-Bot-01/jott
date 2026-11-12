# Jott 0.0.3 — the vault, the editor, and the `np` bridge

The first build meant for another person's machine, and the first one that arrives ready: **np is inside the
installer**, so installing Jott is one download and indexing and search work straight away.

**Assets**

| File | For |
| --- | --- |
| `jott-windows-x64.msi` | Windows 10/11, x64. This is the one to test: Jott is built for Windows first. |
| `jott-linux-x64.deb` | Debian/Ubuntu, x64 — a convenience for whoever maintains this. |
| `jott-linux-x64.AppImage` | Same, on anything else Linux. |
| `SHA256SUMS.txt` | Checksums of the three above. |

The bundled np is 0.3.1, fetched from its release and checked against a hash pinned in this repository before
it goes anywhere near an installer. Prefer your own build? Set `NP_PATH` and Jott will use it — an override you
choose outranks the copy we ship.

**Install it**

Run the `.msi`. Windows will warn that the publisher is unknown — true: this build is unsigned, and signing is
a separate piece of work (§20). If SmartScreen offers *More info → Run anyway*, that is the expected path. The
model for semantic search is **not** bundled; the first time something needs it, Jott names the model and the
folder it would download into and asks. Saying no leaves indexing and keyword search working.

**What to try, and what to send back**

[`docs/testing-on-windows.md`](https://github.com/Ex-Bot-01/jott/blob/main/docs/testing-on-windows.md) is the
walk-through: five things to test, the write-lock contention test that only Windows can answer, and exactly
which file to send back (`%APPDATA%\jott\logs\jott.jsonl`). That log holds one line per request Jott made to
`np` — method, folder, duration, outcome — and never the contents of a note.

**This release also fixes three things people noticed in 0.0.1/0.0.2**

- The window no longer opens blank: it stays hidden until the front end has painted, with a three-second
  watchdog so a broken build still shows something rather than nothing.
- Nothing is centred any more. The scaffold stylesheet set `#app { text-align: center }`, and your notes were
  inheriting it — prose in the editor included.
- Uninstalling still leaves `%APPDATA%\jott` behind, and now says why: that folder can hold a mirrored copy of
  unsaved text and notes Jott moved to its own trash. Deleting writing on uninstall would be worse than leaving
  a folder. The document names the two folders and how to clear them when you are sure.

**What this version deliberately does not do**

- No search overlay yet — a search field in the sidebar and plain results; no ⌘K palette, no keyboard shortcuts.
- Renaming reports which notes link to the one you renamed and rewrites nothing; that needs our own link index.
- macOS is not built, and there is no np asset for it either. Upgrades over an existing install are not handled:
  uninstall first.

Built by `build.yml` from the tag this release is attached to; the run's log shows the np version bundled and
every test that gated these files.
