# Jott 0.0.1 — the vault, the editor, and the `np` bridge

The first build meant for another person's machine. Jott opens a folder of markdown notes, lets you write
and save them, and quietly keeps [`np`](https://github.com/Ex-Bot-01/note-processor)'s index current —
indexing and embedding whatever changed, and telling the truth about what it did.

**Assets**

| File | For |
| --- | --- |
| `jott-windows-x64.msi` | Windows 10/11, x64. This is the one to test: Jott is built for Windows first. |
| `jott-linux-x64.deb` | Debian/Ubuntu, x64 — a convenience for whoever maintains this. |
| `jott-linux-x64.AppImage` | Same, on anything else Linux. |
| `SHA256SUMS.txt` | Checksums of the three above. |

**Install it**

1. Download the `.msi` and run it. Windows will warn that the publisher is unknown — that is true, this build
   is unsigned (the signing story is a separate piece of work). The app is not signed *and* not vetted by
   Microsoft yet; if SmartScreen offers "More info → Run anyway", that is the expected path.
2. Get `np` 0.3.1 or newer, as described in [`INSTALL.md`](https://github.com/Ex-Bot-01/jott/blob/main/INSTALL.md)
   §2 — for Windows, the section "Windows (PowerShell) — same procedure". Jott works without it for reading,
   writing and renaming, and says so in the status line rather than failing silently.

**What to try, and what to send back**

[`docs/testing-on-windows.md`](https://github.com/Ex-Bot-01/jott/blob/main/docs/testing-on-windows.md) is the
walk-through: five things to test, the write-lock contention test that only Windows can answer, and exactly
which file to send back (`%APPDATA%\jott\logs\jott.jsonl`). That log holds one line per request Jott made to
`np` — method, folder, duration, outcome — and never the contents of a note.

**What this version deliberately does not do**

- No search overlay yet. There is a search field in the sidebar that lists plain results, and nothing else:
  no ⌘K palette, no keyboard shortcuts of any kind, no pinned results.
- Renaming a note reports which other notes link to it, and rewrites nothing. Rewriting links needs our own
  link index.
- It will not download an embedding model without asking, and the ask names the model and the folder it goes
  into. Saying no keeps indexing and keyword search working.
- macOS is not built. Neither is an installer that upgrades over a previous one — uninstall first.

Built by `build.yml` from the tag this release is attached to; the run's log shows which `np` was on the
runner and every test that gated these files.
