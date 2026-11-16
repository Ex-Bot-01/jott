# Plan: things you could not create, or could not see afterwards

Filed from testing on Windows after v0.0.16. Four symptoms, one root cause apiece — and two of the four are the same
cause wearing different clothes: **the window lets you ask for something it has no place to put, and then does not say
so.**

Each item below was read out of the code at head (`30442a9`) or reproduced with a test before being written down. Where
the tester's guess about the cause differs from what the code says, both are recorded: their description is the
symptom, and the symptom is not negotiable.

---

## A. `+ note` / `+ folder` with no folder open does nothing at all

**Symptom (as reported).** First run, no workspace. Click **+ folder**, type a name, press Enter: no message, no
folder, no change.

**What the code does.** `runAction('new-folder', null)` opens the dialog regardless of whether a vault is open; on
submit `createFolder('ideas')` calls `create_folder`, and core answers `IpcError::no_vault()` — `"no vault is open"`.
`call()` puts that sentence in `problem`, which renders here:

```svelte
{:else if listing && awaitingIndex > 0 && !selected}   …
{:else if listing && awaitingEmbed > 0 && !consent && embeddingsOffer}   …
  {#if problem}<p class="warn">{problem}</p>{/if}
```

That block is the *last* branch of a chain whose earlier branches need `listing`. With no folder open, none of them is
taken — so **the sentence is written to state and never drawn.** The window swallows its own refusal.

**Fix.** Two halves, both small:

1. `problem` gets rendered where it cannot be missed whether or not a folder is open (the empty-state branch of the
   right-hand pane), and the dialog stays open with what was typed still in it — same rule as the move dialog: closing
   on a failure says "done" about something that was not done.
2. The create buttons stop offering what cannot happen, and say what to do instead. Ruling needed: see **R1**.

**Tests.** Smoke: with no vault open, `+ folder` → name → Enter leaves an on-screen sentence naming the missing folder
and sends nothing; the dialog is still open; after picking a folder (stubbed), the same submission succeeds. The stub's
`create_folder` must refuse with `{ kind: 'noVault' }` when no vault is open — today it answers success
unconditionally, which is how this stayed invisible to the suite.

---

## B. A new folder appears, but nothing about it says "folder"

**Symptom (as reported).** "*I opened a workspace and clicked + folder → nothing happens? Ah, it was not updated on the
left side. It updated after I created a file.*"

**What the code does.** The tree *did* update: `createFolder` awaits `refresh()`, and core lists an empty directory as
a row — measured, not assumed:

```text
an empty folder is not a row, so the window cannot show what it just made:
[("ideas", "Folder"), ("inbox.md", "Note")]        ← the assertion failed because the row IS there
```

Everything that distinguishes a folder row is `font-weight: 600`, and the twisty glyph is drawn only when the folder has
children (`hasChildren`). A brand-new folder is therefore bold text with no marker, no twisty and no children — next to
`inbox.md` it reads as "nothing happened". The tester's own later sentence is the finding: *"the folder in the left
sidebar is not good differentiable from files."*

**Fix.** Make a folder look like a folder at every age:

- always draw the twisty slot for a folder — `▸` when closed, `▾` when open, and a dimmed `·` when it has nothing in it
  yet, so the column never jumps;
- say what an empty one is: a quiet `(empty)` after the name until it has children;
- keep the bold, and pin all three in the DOM (jsdom can read text and classes even if it cannot compute CSS).

**Tests.** Rust: `a_folder_with_nothing_in_it_is_still_a_row` (already written, currently failing on my wrong
expectation — core spells a folder's `rel` as `ideas`, not `ideas/`; the assertion gets corrected to what core says and
the test kept as the guard). Smoke: create a folder with **+ folder** in an open vault and assert a `[data-kind="Folder"]`
row exists *without* any further command, and that its text carries `(empty)`.

---

## C. A note created as `hello` is a file called `hello`, which nothing can open as a note

**Symptom (as reported).** "*I just entered the name of the file and it just created this, no markdown file.*"

**What the code does.** The dialog passes the typed name straight to `create_note`; core creates exactly what it is
told (`stays_off_np_catalog` is its only veto). So `hello` becomes a file with no extension, which the walk classifies
as `Attachment`, not `Note` — no index marker, nothing for np to embed, and it opens in an editor that will not treat it
as a note. The user's intent ("a note called hello") is lost at the last step.

**Fix.** Decide the name in one tested module (`src/lib/noteName.ts`), applied where the dialog submits:

- no dot → `hello` becomes `hello.md`;
- a dot whose suffix is a recognised text kind (`md`, `markdown`) → normalised to `.md`;
- a recognised non-markdown kind (`txt`, image extensions the tree already knows: `png jpg jpeg gif webp bmp`) → left
  exactly as typed, because asking for `hello.txt` means something;
- any other suffix that is shaped like an extension (`hello.bak`, `notes.new`, `hello.tar.gz`) → left exactly as typed,
  and the window says what it made and that note-processor will not read it as a note;
- trailing/leading dots, spaces, and a bare `.` are refused with a sentence rather than silently renamed.

Whether the recognised list is editable in Settings is **R2**.

**Tests.** Table-driven unit tests for the rules above (including `hello.MD`, `.md`, `hello.`, `my note`,
`hello.tar.gz` — which keeps `.gz`, because the last dot decides and a rule nobody can predict is not a rule; an earlier
draft of this plan promised `hello.tar.md` there, and that was a worse promise than silence). Smoke: typing `hello` in **new note** creates and opens `hello.md`; typing `hello.txt`
creates `hello.txt` and says what it made.

---

## D. You cannot pick a folder to create something inside

**Symptom (as reported).** "*I cannot select the folder in the left sidebar if I want to create a new file or folder
inside the folder?*"

**What the code does.** Two facts add up to the complaint:

```ts
function click(row: Row) {
  if (row.kind === 'Folder') ontoggle(row.rel)   // folders toggle and are never selected
  else onselect(row.rel)
}
```

and both toolbar buttons call `runAction('new-note', null)` — `null` meaning "the vault root", always. The capability
exists one interaction away: right-click a folder row and its menu offers *New note here* / *New folder here*, which go
through `targetFolder(row)`. So the feature is real, undiscoverable, and contradicted by the toolbar two pixels above.

**Fix.** One click on a folder row selects it **and** toggles it (what every file tree on Windows does); the toolbar
buttons stop passing `null` and pass the selection instead, so **+ note** makes `ideas/new-note.md` when `ideas` is
selected and says where it will put the file before the dialog closes. `actionsFor`/`targetFolder` already handle a
folder row, so this is wiring rather than new logic — plus the tooltip on each button naming the destination folder.

**Tests.** Unit: `targetFolder` for a selected folder / nested folder / note (already covered — extended to assert the
toolbar's choice function). Smoke: select `notes/`, press **+ note**, and the created `rel` is `notes/…`; with nothing
selected it stays at the root.

---

## Rulings needed before implementing

- **R1 — what `+ note` / `+ folder` should do when no folder is open.** Options:
  - **(a, recommended)** the buttons stay enabled; pressing one puts a sentence in the empty right-hand pane — *"There
    is no folder open yet. Choose one, and the note will be made inside it."* — beside an **Open a folder** button, and
    the dialog stays open with the typed name kept. After picking, submitting continues without retyping.
  - (b) pressing one goes straight to the folder picker, then creates in whatever was chosen: one click fewer, but the
    person did not ask to pick a folder.
  - (c) disable both buttons until a folder is open, with a tooltip. Simplest; leaves the first-run window with dead
    buttons and no sentence.

- **R2 — is the extension list editable in Settings?** My recommendation: **not yet.** Ship the fixed list above, show
  it in the panel as text so it is at least discoverable, and make it editable if someone actually wants a different
  rule — an editor for a list nobody has disagreed with yet is a setting that only grows. Say the word if you want it
  editable now and I will build it as part of item C.

- **R3 — answered: keep it, and explain it.** A person opening a folder of `notes.txt`, `ideas.org` and `scan.pdf` needs
  to be told what happens to them, so the explanation is in the window (a line under the tree, counted from the rows the
  walk already produced) and *new note* keeps any extension that was typed on purpose. Filed upstream as
  [note-processor#12](https://github.com/Ex-Bot-01/note-processor/issues/12): measured over JSON-RPC, `np 0.5.1` answers
  `scan` with `added: 5, notes: 1` for a folder where four of five files were plain text — the rule is documented in np's
  own walker (`MARKDOWN_SUFFIX`, matched case-insensitively) and `unindexable` stays `null` because nothing was lost. The
  ask is either `[index] note_suffixes` or an attachment count a front end can quote instead of inferring.

- **R3a — the older form of this question:** should `+ note` Today the dialog creates any
  name, which is how `hello.txt` gets made by accident or on purpose. Keeping it (per C) means Jott will happily create
  files it cannot edit; alternative is refusing non-text names in *new note* and pointing at the file manager.
  My recommendation: keep it and say what was made — refusing is a smaller world for the person who wants `todo.txt`.

## What shipped, and where it differs from the plan above

| item | commit | difference |
| --- | --- | --- |
| A first-run buttons | `4eee5d6` | Also found: `Ok(())` crosses the wire as `null`, which is this window's failure marker — so every *successful* `create_folder` skipped its re-list and every successful `move_entry` left its dialog open. Half of "nothing happens" was that, not presentation. The stubs were fictions: `list_vault` invented an empty listing where core refuses, `create_note` was never stubbed at all, and a folder reached by *opening* it never asked for the embeddings offer. |
| B a folder looks like a folder | `4eee5d6` (cause) + this item's commit (appearance) | The twisty now exists at every age (`▸` / `▾` / `·`) and an empty folder says `(empty)`. |
| C the name a note gets | `14af15d` | Last dot decides, so `hello.tar.gz` stays itself rather than becoming `hello.tar.md`; `.keep` is a name someone means. Added: the formats line under the tree, and [note-processor#12](https://github.com/Ex-Bot-01/note-processor/issues/12) upstream for the rule itself. |
| D pick a folder to create inside | this item's commit | One click selects *and* toggles, as a Windows file tree has always meant; opening a note drops the destination so the tooltip cannot disagree with the button. `Ctrl+N` / `Ctrl+Shift+N` go through the same path. |

## Order of work

A → B → C → D, one commit each, tests first. A and B are what made the first five minutes of using Jott feel broken;
C stops the window creating things np will never index; D is the one that is a genuine new capability rather than a
missing word.
