# Jott

A desktop markdown note editor for a folder on disk, with [`np`](https://github.com/Ex-Bot-01/note-processor)
as its index and search backend. `np` indexes your notes, resolves the links between them, embeds them with
a small local model and answers hybrid keyword+semantic queries over stdio; Jott is the window you write in.

Windows first, Linux supported (it is where the code gets built). macOS: no `np` asset exists, so a source
build only — out of scope for now.

**Status:** M1 is built — vault, editor, and the `np` bridge — and waiting on its first run on a Windows
machine by somebody other than whoever wrote it. Private repository; MIT and public later by choice.

| Read | For |
|---|---|
| [`docs/superpowers/specs/2026-11-10-np-note-editor-design.md`](docs/superpowers/specs/2026-11-10-np-note-editor-design.md) | The design: what Jott is, the window, the ingest rules, and every backend fact measured rather than assumed (§18 is authoritative where it disagrees with an earlier section) |
| [`docs/testing-on-windows.md`](docs/testing-on-windows.md) | The hand-off walk-through: install, five things to try, the write-lock test only Windows can answer, and which log file to send back |
| [`docs/building-on-windows.md`](docs/building-on-windows.md) | Building the installer by hand, prerequisites included |
| [`INSTALL.md`](INSTALL.md) | Getting `np` onto a dev machine, verified end to end, Linux and Windows |
| [`NOTES.md`](NOTES.md) | The decision log — what was said, what was proposed, what got measured |
| [`tests/fixtures/README.md`](tests/fixtures/README.md) | A verbatim `np serve` transcript, and what each line proves to a client |

## Ground rules

Jott is **purely a frontend**. Nothing in `note-processor` gets edited from here — bugs found there are
filed as issues and we work around them; if one blocks us, we say so rather than fixing it ourselves.
