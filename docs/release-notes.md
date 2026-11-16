# Jott 0.0.17 — the first five minutes

Four reports, all from the part of Jott you meet once: before you open a folder, and just after. A button that answered
nothing, a folder that seemed never to have been made, a note created as a file no one can search, and a sidebar where a
folder was indistinguishable from a note and impossible to choose. The backend inside is still **note-processor 0.5.1**
on protocol 1; every change here is in the window.

## What you'll notice

- **A button that cannot do its job says so, where you were already looking.** With no folder open, **+ note** /
  **+ folder** used to take a name, accept Enter, and produce silence. The sentence existed — *no vault is open* — and
  was drawn in small grey text at the bottom of the sidebar, while the dialog you had just pressed Enter in closed
  itself whether or not the command worked. True information in the wrong place, arriving after the thing you were
  looking at disappeared, is experienced as no answer. The dialog now carries the reason itself: *"No folder is open yet, so
  there is nowhere to make this. Choose one, and it will be made inside it."* beside a **Choose a folder** button. What
  you typed is kept, so picking a folder finishes the note you asked for rather than starting it over, and nothing is
  sent while there is nowhere to put it. Every other refusal now appears in one place above the search box, folder open
  or not.
- **A new folder appears at once, and looks like a folder.** Two faults were stacked here. A successful `create_folder`
  was being read as a failure — its whole answer is *nothing*, which crosses the Tauri wire as `null`, which is exactly
  what this window uses to mean *failed* — so the tree was never asked again; and a folder row's only distinguishing
  feature was bold text, with no twisty until it had children. Now a folder gets `▸`/`▾` when it holds things and `·`
  when it does not, an empty one says `(empty)` beside its name, and the tree re-lists after the create without waiting
  for some other command to do it.
- **Clicking a folder means it.** One click selects the folder *and* opens or closes it, as one click has always meant
  in a Windows file tree. The toolbar follows: its tooltip reads **New note in notes (Ctrl+N)**, the dialog label says
  `New note in notes/:`, and the file lands there. Open a note and the destination returns to the vault root — a tooltip
  that promises one folder while the button writes to another is worse than no tooltip. `Ctrl+N` and `Ctrl+Shift+N` take
  the same path as the buttons.
- **A note gets a name that is a note.** `trip plan` becomes `trip plan.md`, because that is what a note *is* to
  note-processor, and an extension-less file is not one: no index marker, nothing to embed. An extension you typed on
  purpose survives exactly as typed — `todo.txt` stays `todo.txt` — and the window then says what happens next instead of
  letting you find out by searching: *"Created todo.txt. note-processor reads only .md files as notes, so this one will
  not be searchable."* Spellings note-processor cannot read are folded to `.md` **and announced** (`hello.markdown` →
  `hello.md`). Names that cannot become files — `notes.`, `.`, `..` — are refused in words with the dialog open and your
  name kept; nothing is quietly renamed, and a trailing full stop is not trimmed because Windows would not keep it
  either. The rule is deliberately boring: the **last** dot decides, so `hello.tar.gz` stays itself rather than becoming
  something nobody typed.
- **A folder of other formats explains itself.** If anything in the folder is not a note, you get one line under the
  tree: *"Of 12 files here, 3 files are notes note-processor can search; 9 are kept as attachments — tracked for links,
  never searched. note-processor reads .md files as notes."* Counted from the rows Jott's own walk already classifies,
  shown only when there is something to explain, and worded so a folder of `.txt` and `.pdf` is not described as broken.

## The one backend-shaped thing

Only `.md` is parsed as a note. That is note-processor's rule, stated in its own walker — a file is a note when the
lowercased relative path ends in `MARKDOWN_SUFFIX` (`.md`) — and everything else it finds is an attachment: "hashed,
tracked and linkable but never parsed" (np spec §10). Measured against 0.5.1 over JSON-RPC on a folder of `note.md`,
`plain.txt`, `file.org`, `file.rst`, `sub/other.txt`, all containing the searched word:

```json
{"method":"scan","result":{"added":5,"notes":1,"skipped":[],"unindexable":null}}
```

…followed by a search that finds one file. Nothing was lost, so `unindexable` is correctly empty — which also means
there is no field a front end can quote as the reason. Jott could have stayed silent and let the count speak; instead it
says the sentence above from its own walk, **and** the missing capability is filed upstream rather than worked around
here: [note-processor#12](https://github.com/Ex-Bot-01/note-processor/issues/12) asks for either `[index]
note_suffixes` or an attachment count in the scan report, so the numbers in that sentence can come from the process that
decided them. Jott converts nothing and rewrites nobody's files.

## Also fixed

- **Four more, found by a fresh review of the whole branch before shipping.** A refusal from one dialog used to follow
  the next one around — fix `notes.` into a good name, then press **+ folder**, and the fresh dialog opened saying
  *"A file name cannot end in a full stop."* about an empty field. A chosen destination survived trashing or renaming
  the folder it named, so the tooltip promised to re-create a directory you had just thrown away (core makes parent
  folders, so nothing else stopped it) — the destination is now checked against what the walk still shows. `+ note`
  opened whatever it had made, which meant an empty `photo.png` arrived in an editable buffer inviting you to type a
  paragraph into an image; text-shaped results open, images and other binaries do not, and the sentence about what was
  made says so. And a refused rename closed its dialog and discarded the name you typed, contradicting the rule this
  release states twice.
- **A move that worked left its dialog open.** Same wire confusion as above: `Ok(())` looked like failure, so the window
  refused to believe its own success. Commands whose whole answer is "it happened" now resolve to a value that cannot be
  mistaken for the failure marker — which also covers `mirror_buffer`, `discard_mirror` and `ingest_all`.
- **Opening a folder did not ask about embeddings.** The offer that tells you your notes are indexed but unsearchable by
  meaning was only asked after a *re-list*, so a folder reached at startup or through the picker never asked at all.
  Every path that installs a listing asks now.
- **A first run no longer begins with an error it cannot show.** With nothing in the recents list, Jott used to ask
  note-processor to list a folder that had never been opened; the honest refusal went into state with nowhere to be
  drawn. It asks for nothing until you choose.

## What is behind this

Three of the four reports were invisible to the test suite because the test doubles were more generous than the real
program: `list_vault` answered an empty listing where core answers a refusal, `create_folder` resolved an object where
core sends `null`, and **`create_note` was not stubbed at all** — so note creation had never been exercised end to end.
All three fictions are gone, the scene's tree now comes from one function shared by opening and re-listing, and the "no
folder is open" tests finally describe a window with no folder open.

The review then found five more doubles still lying: it accepted `delete_entry` and `rename_entry` only by rejecting
them ("a command nobody stubbed"), answered the mirror pair with `undefined` where core sends `null`, kept answering
with a folder the test had just deleted, and — worst — one test that claimed an intention finished after picking a
folder while its own stub was still refusing every command with *no vault is open*. The stub now keeps a session like
core does, remembers what it created and what it trashed, and answers for the folder that is actually open. Two of the
new tests were then checked by putting the bug back: one passed for the wrong reason at first (asserting before Svelte
had repainted made "nothing was opened" look true), and it was rewritten to wait for the sentence that proves the flow
settled.

## Numbers

- **302** Rust tests pass (`cargo test -p jott-core`, note-processor off `PATH`); CI prints the count and fails under
  its floor (**295**).
- **341** front-end tests pass (was 299 at 0.0.16), judged by Vitest's exit code rather than by a grep of "passed".
- `svelte-check`: 0 errors, 0 warnings. Windows build target type-checks as `jott-shell v0.0.17`.

## If you test it

Two of the new tests were checked by putting the bug back — deleting the image guard and un-deriving the destination
each turn four tests red and restoring them turns all 71 green again — because a test that has never failed is an
opinion about code nobody has tried to break.

`docs/testing-on-windows.md` §2b-vii has six checks. The three worth doing first: press **+ folder** on a true first run
and check the sentence, the kept name and that nothing landed on disk; make a folder and look at it *immediately*; open
a folder of mixed formats and read the line under the tree — it should not sound like blame.
