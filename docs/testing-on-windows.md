# Testing Jott on Windows

Jott is built for Windows first (spec §19): it is where the app will be used, and it is the only place the
things that matter can be seen — WebView2's text rendering, the write lock np holds on a workspace, and
whether an unsaved buffer ever comes back when it should not.

Everything below runs on a normal Windows machine. Nothing here needs this repository.

## 1. Install

1. **Jott** — go to <https://github.com/Ex-Bot-01/jott/releases>, open the newest release, and download
   `jott-windows-x64.msi`. Double-click it; Windows may warn that the publisher is unknown, because this
   build is unsigned until the signing story in spec §20 exists — *More info → Run anyway* is the expected
   path. (If you were sent a link to a workflow run instead of a release, that is an artifact: it needs a
   GitHub account with access to this repository and it disappears after 30 days. A release needs neither.)
   No download at all — because CI has not produced one yet, or you want a build newer than the last tag —
   then [`docs/building-on-windows.md`](building-on-windows.md) is four commands.
2. **np comes with it.** The installer carries the pinned np build inside it — that is why installing one
   program is enough. The bottom-left of the window says which np version is answering. If you would rather
   Jott used an np you built or installed yourself, set `NP_PATH` to
   it: an override you set beats the copy we ship, deliberately (§14).
3. **Check what you downloaded**, if you want the certainty rather than the assumption. In PowerShell, in the
   folder you downloaded to, next to `SHA256SUMS.txt` from the same release:

   ```powershell
   (Get-FileHash .\jott-windows-x64.msi).Hash.ToLower()
   Select-String jott-windows-x64.msi .\SHA256SUMS.txt
   ```

   The two strings should match. If they do not, the file did not arrive as CI built it — say so and download
   again.
If something is wrong with the bundled copy — deleted by an antivirus, or you installed from before np started
shipping inside the installer — Jott still opens, everything except search and embedding works, and the
bottom-left of the window says what is missing and how to install it. That state is worth a look too: it is what
a broken install looks like. To install np by hand instead, follow
[`INSTALL.md` §2, "Windows (PowerShell)"](../INSTALL.md#windows-powershell--same-procedure) and point `NP_PATH`
at the result.

## 2. The five things to try

Opening a folder does not index it by itself: indexing writes `INDEX.md` into your vault, so it stays something
you ask for. The sidebar says *N notes are not indexed yet* with an **Index now** button. It means the whole
folder — every note in it, including the ones you did not touch this session — and since 0.0.9 it really does
that; before then the button only ran whatever a save had queued, so on a folder you had merely opened it did
nothing at all while looking like it had finished.

Take your time with each and note anything that looks wrong, in words — exact wording is deliberate, so if a
sentence reads oddly, say so.

1. **First run.** *Open a folder*, pick a notes folder with a few markdown files in it. The tree should list
   them, each with a `○` beside it — *not indexed yet*. An `INDEX.md` written by np appears greyed and italic;
   hovering says whose file it is, and clicking it does nothing, which is correct. A folder that already carries
   np's `INDEX.md` gets one sentence about being inside another vault.
2. **Index, then decide about embeddings.** Press **Index now**. The markers should move from `○` (not indexed)
   to `●` (*indexed · awaiting embedding*) and the sidebar line should disappear — that is the index half, and
   it needs no model and no download. Then the question comes:

   > semantic search needs a model (embeddinggemma-2, about 296 MiB) downloaded into
   > `C:\Users\you\AppData\Roaming\note-processor\models`. Allow?

   **Allow the download** fetches it and writes the vectors; markers end at `·` (*embedded*) and search starts
   answering with meanings, not just words. It is a few hundred megabytes, so the first pass takes a while —
   watch `fetching … / … MiB` in the status line. **Stay keyword-only** changes nothing about indexing: search
   keeps working on words, and the dots rest at `●`.

   Before 0.0.9 that question was never asked on a fresh install, because np's config file names no model and
   Jott read that as "nothing to ask about" — while np itself carries a built-in model and will download it
   unprompted on the first *search*. That hole is closed from both sides now: the question is always asked, and
   until you answer, Jott sets np's `auto_fetch` to off so nothing arrives behind the dialog. Check the folder
   named in the sentence before you answer — if a model is already in there, something fetched it earlier.
3. **Rename.** Select a note, press `rename`, give it a new name in the same folder. It must refuse to
   overwrite an existing name (and say so), and if another note links to it with `[[like this]]`, the notice
   should count how many notes refer to it. Nothing rewrites those links yet — that is a later plan — so a
   rename of a linked note is expected to leave the referring text alone.
4. **A file changed by another editor.** Open a note in Jott, then change the *same* file in Notepad or VS
   Code and save it there:
   - with no unsaved typing in Jott, the editor should quietly show the new text;
   - with unsaved typing (type something and do not save), Jott must say the file changed outside and offer
     *Reload the file* or *Keep typing*. Choose each once, in separate runs. It must never overwrite either
     side on its own.
5. **Closing mid-embed.** Start an embed (the `[embed]` button in the status line), then close the window
   while it says `embedding …`. Reopen: Jott should come back promptly — it waits a few seconds at most, and
   never for a model download. Whatever was saved before closing should be indexed on the next pass rather
   than lost.

3. **Change your mind about embeddings.** After *Stay keyword-only*, the sidebar should say
   *N notes are indexed but not embedded, so search finds their words and not their meanings* with a
   **Set up embeddings** button. Press it: the same sentence appears as a confirmation, and accepting writes
   the decision and starts the pass straight away — no save needed first. This is the way back for someone who
   said no months ago; the alternative used to be editing np's `config.toml` by hand.

Two more, if you have the patience:

- **Delete something** and check where it went. The notice says which: the system trash, or — on a machine
  where Windows offers no trash for that path — `%APPDATA%\jott\trash\`. A file in there is retrievable.
- **Type into a note and close the laptop lid** (or end the process from Task Manager) before the two-second
  autosave fires. On reopening, Jott should offer the text it had mirrored: "You had unsaved text from an
  earlier session", with *Use it* / *Discard it*. **This one is the test most worth reporting on**, in either
  direction — including "it offered me text I had deliberately deleted", which is a bug, not a nuisance.

## 2b. Since 0.0.10: the editor looks different, on purpose

Headings are bigger, `**this**` is bold, inline code has a grey tint, links are underlined, and the syntax
markers — `#`, `**`, backticks, `>` — sit at roughly 40% ink instead of full strength. The text you type is
still the file's own bytes: nothing is auto-closed, auto-converted or hidden, so what you see bold is exactly
what a plain text editor would show between two asterisks.

Two things worth checking while you are in there:

- **The caret.** It should be unmistakable — two pixels of the same ink as the text, on both light and dark
  paper. If it still reads faint at your display scaling, tell me the scaling percentage and the monitor.
- **Search marks.** Search for a word that appears in a note, look at the result row: your word should be
  marked inside the snippet. Click the row: the note should open scrolled to the first place that word appears,
  with every occurrence marked. A hit that came from meaning rather than words has nothing marked and does not
  jump — that is correct, not broken.

## 2b-ii. Since 0.0.12: the sidebar does what the keyboard says

Things that were always meant to be there and never were:

| Press | Expect |
| --- | --- |
| **Ctrl+N** | New note, in the folder you had selected |
| **Ctrl+Shift+N** | New folder — this is the first release where you can make one at all |
| **Ctrl+R** or **F2** | Rename the selected note or folder |
| **Delete** | Ask, then move it to trash. Neither key does anything while you are typing in a note. |
| **Ctrl+F** | Focus the search box |
| **Ctrl+S** | Save now. Autosave still runs two seconds after you stop; this answers the key instead of quietly covering for it. |

Right-click a row (or empty ground) for the same list as a menu: *New note here*, *New folder here*, *Rename*,
*Move to…*, *Move to trash*. Right-clicking `INDEX.md` offers nothing, because that file belongs to np.

Two more things to report on:

- **The title bar** now says `Jott — <folder>`. It should name the folder you are actually in, after switching
  folders too.
- **Recent folders** appear next to *Open…*. Picking one should reopen that vault. If a folder you deleted or moved
  is listed, opening it should fall back quietly to asking for a folder rather than showing an error.

## 2b-iii. Since 0.0.13: the np inside is 0.4.0, and search stopped guessing

*(Historic: Jott ships np 0.6.0 since 0.0.29, so read every `0.4.0` and `0.5.1` below as `0.6.0`. The behaviours asked for in §2b-v still apply; §2b-ze adds what 0.6.0 changes on top.)*

Four things to check, all of them about what the window claims:

- **About → the np line should read `0.4.0`.** So should the bottom-left of the window.
- **An empty search now says which kind of empty it is.** In a folder nobody has indexed: *"This folder is not
  indexed yet … use Index now"*. After Index now, searching for something genuinely absent: *"Nothing matched
  `q`"*. Getting these two the same was [note-processor#5](https://github.com/Ex-Bot-01/note-processor/issues/5);
  0.4.0 answers the question, so the sentence is no longer Jott's inference from the tree's markers.
- **A word-only result admits it.** In a folder without embeddings, the results header adds
  *words only · meanings not searched*. It used to print np's `mode` word, which said `hybrid` here and promised a
  half of the search that never ran.
- **A download prompt quotes a size.** If Jott asks about setting embeddings up, the number it names comes from np
  (0.4.0 reports what its own build would fetch). No number at all, when np quotes nothing — which is different from
  zero and should read that way.

If you set `NP_PATH` at an np you built months ago, Jott now refuses it and names the asset to download instead.
Unset the variable to get the copy in the installer; that is the normal case.

## 2b-iv. Since 0.0.14: consent you can see, and a download that moves

Five things, all of them about parts that used to be invisible:

- **"Allow the download" now ends in something you can act on.** If note-processor still cannot embed — no embedding
  engine turned on, an engine with no model named, or a `config.toml` that will not parse — the window shows a card
  instead of the old `keyword-only: embeddings.provider is not set`. It says what your click recorded, quotes
  note-processor's own sentence word for word, and lists the exact lines its settings file is missing as text you can
  copy. A file that does not parse gets that sentence and nothing else: no advice about which lines to add, because
  Jott cannot read the file to know where they would go.
- **Jott writes one key, and says which.** Your answer goes into `models.auto_fetch` in note-processor's `config.toml`
  and nowhere else. Jott does not pick your embedding engine or model, because there is no way to ask it to over the
  protocol — that gap is [note-processor#7](https://github.com/Ex-Bot-01/note-processor/issues/7). Until you answer,
  that key says `false`: a fresh np would otherwise fetch a ~300 MB model by itself, and that is not a decision to
  make on somebody's behalf.
- **A download has a bar.** While a model arrives, the line at the bottom counts np's own bytes and adds
  *model part 1 of 2* when np fetches more than one file. When np does not announce a total you get movement without a
  percentage — never a bar parked at zero.
- **"Model ready" is an announcement, not a rumour.** When a model finishes, the window keeps a line saying so and
  that search matches meanings as well as words now; if the window is in the background and you allow notifications,
  you get one OS line too. It appears only when np reports it fetched something — an ordinary embed run says nothing.
- **Diagnostics → Show log file opens Explorer with `jott.jsonl` selected.** It used to be "Open log folder", which
  the shell is not allowed to do, so its failure path handed you a small box of copied text instead. If the file
  manager ever refuses again, the line names the path and offers to copy it.

Search still matches whole words: typing *grand* will not find *grandma*. That is note-processor's index working as
designed, and the request for prefix matching is
[#6](https://github.com/Ex-Bot-01/note-processor/issues/6) — Jott deliberately does not bolt its own text search in
front of np's results.

## 2b-v. Since 0.0.15: the backend is np 0.5.1, and two of its doors are open

The np inside Jott is **0.5.1**. Protocol is still version 1, so nothing about how the two talk changed; what changed
is that np now answers two questions it used to refuse, and Jott asks them instead of working around them.

- **"Match word beginnings" is a checkbox beside the search box, off by default.** With it off, *grand* still finds
  only *grand* — the same search you have always got. Turn it on and *grand* reaches *grandmother*, which is
  [note-processor#6](https://github.com/Ex-Bot-01/note-processor/issues/6) fixed in np itself: Jott sends
  `{"prefix": true}` and np widens its own index query. It is off because a vault full of identifiers, ticket numbers
  and partial names reports notes you did not ask for when every term quietly grows. Your choice is remembered per app
  (not per folder), and survives closing the window.
- **When it widens, the results say so.** The header gains a line naming the words np widened — *also matched word
  beginnings (grand)* — using np's own list, never Jott's guess at it. A row that only matched by its first letters is
  marked *from the start of a word*. A row found by meaning alone gets no mark: np never tested its words, and saying
  otherwise would be a claim about a test nobody ran.
- **An empty search can offer the widening without taking it.** In a folder np says is indexed, if whole words found
  nothing you get *try matching word beginnings*. Pressing it re-runs that one query widened and does **not** move the
  checkbox: trying something once should not decide all your future searches. It is offered only where pressing it can
  change the answer — never in a folder that has not been indexed, and never when you are already widening.
- **"Allow the download" goes through np's door.** Pressing it now calls note-processor's own `embeddings.enable`
  method; np writes its own two settings and says which ones. The card quotes that answer — *note-processor wrote
  embeddings.provider and embeddings.model into the file C:\Users\you\AppData\Roaming\note-processor\config.toml*,
  plus what the download will cost in bytes — and beside it, one line about Jott's own part. If np refuses (an id this
  build cannot use, a file it cannot read), the card quotes the refusal unchanged and says that Jott left
  `models.auto_fetch` alone: after a refused *yes*, the download guardrail is exactly where it was, and the window says
  so instead of implying you had allowed something. The advice to paste lines into `config.toml` is gone; there is
  nothing left to paste by hand.
- **Jott still writes exactly one key.** `models.auto_fetch` — that key only, in `%APPDATA%\note-processor\config.toml`.
  Everything else about embeddings is np's, written by np. If you open that file after allowing a download you should
  see a comment naming the method that wrote it, over `np serve`, at a date:

  ```toml
  # note-processor set by the `embeddings.enable` method over `np serve` (np 0.5.1) at 2026-11-15T…
  [embeddings]
  provider = "local-model"
  model = "embeddinggemma-2"

  [models]
  auto_fetch = true     # <- the one key Jott writes
  ```

  `auto_fetch` stays there because np does not touch it: measured on 0.5.1, enabling embeddings through the new method
  leaves `auto_fetch = false` and embed still refuses to fetch. That is the guardrail doing its job — a fresh np
  defaults to *yes* and would download about 296 MiB (309,855,456 bytes) without asking.

**Six things worth trying:**

1. Type a word stem you know is only inside longer words, search, get nothing, then press **try matching word
   beginnings**. The header should name the widened word and the row should be marked.
2. Close Jott and reopen it: the checkbox should still be on. Turn it off and search again — the widened rows should
   disappear rather than go quiet-but-still-there.
3. Say no to embeddings in a fresh folder, then press **Set up embeddings** → Allow. The card should read as what np
   did (keys, file, size), not as lines to copy.
4. Put `provider = "local-model"` with no `model` line in `%APPDATA%\note-processor\config.toml`, index, and open a
   note so embedding runs: the card should quote np's sentence about needing a model, and say what Jott did not change.
5. After allowing a download, read `%APPDATA%\note-processor\config.toml`. You should be able to tell from the file
   alone which lines np wrote and which one line came from Jott. If you cannot, that is a bug worth reporting.
6. Get np to refuse — put `provider = "local-model"` with no `model` line and press **Allow the download**. The card
   should quote np's refusal and say Jott left `models.auto_fetch` alone, and the file should agree with it: still
   `false`. Then quit, reopen the same folder and search. It should still be keyword-only. A guardrail you said no to
   must not come back up because a window listed the folder again.

One rule behind all of this: **Jott writes one key**, `models.auto_fetch`, and only two places ever touch it — opening
a folder nobody has answered about holds it at `false`, and your answer is recorded once, after np has opened or closed
its own door. Nothing raises it beside a refusal. So if that file ever says `auto_fetch = true` when you never allowed
anything, that is a Jott bug, and the log line (`consent · called embeddings.enable …`) will say which button moved it.

## 2b-vi. Since 0.0.16: five things the window could not do before

All five came from testing rather than from a plan: closing a note, reading the index Jott used to refuse to open, the
caret in dark mode, moving a note somewhere that does not exist yet, and finding the settings at all.

- **A note can be closed.** There is a **close** button beside the note's name, and **Escape** does the same thing —
  but only when Escape has nothing above it to dismiss. One press dismisses one thing, topmost first: the row menu,
  then the settings panel, then About, then whatever you were being asked (a rename, a move, the trash confirmation),
  then the diagnostics drawer, then the note. Escape typed *inside* a note or the search box
  never closes that note, because there the key has a job already. Closing is not saving: nothing is written, and an
  unsaved typing is kept as a draft rather than lost, so reopening that note says there was something there.
- **`INDEX.md` opens, and says it cannot be edited.** It used to refuse to open at all, which read as a broken file.
  Now it reads like any other note — greyed in the tree, with a banner: *note-processor's own index — read only*. The
  refusal moved to where it belongs: writes. **Ctrl+S** on it sends no save and says why in one line; core still
  refuses the write even if something else asks it. The row stays greyed and italic so you can see it is not yours, its
  right-click menu offers nothing, and the toolbar's **rename** and **delete** stay disabled while it is selected — a
  window that offers a thing and then refuses it is worse than one that does not offer it. The shard files (`INDEX-2.md` and so on, when a vault is big
  enough to need them) behave the same way. Your own notes are unaffected: they stay editable, marker and all.
- **The caret should stop disappearing in dark mode.** The real fault was that CodeMirror was never told the window
  was dark: it drew its default one-pixel grey caret on a near-black page, which is invisible rather than faint. It now
  gets the scheme from the same place the palette does, draws two pixels of ink, and — because a caret that blinks off
  *is* gone for as long as it is off — dims instead of switching off. Windows' *show text cursor* setting makes it
  thicker still, and *high contrast* mode takes the system caret colour. If you still cannot find the caret, say which
  of these it is: absent entirely, thin and grey, or there but vanishing while you type. Those are three different bugs
  and the log cannot tell them apart.
- **"Move to…" can move a note into a folder that is not there yet.** The dialog always listed every folder in the
  vault, which in a vault with no folders was one entry and looked like an empty field. There is now a second half:
  *…or a new folder* — type a name, press **create and move**, and the note goes there. Nested names work in one go
  (`ideas/2026` makes both), either slash direction, because `create_folder` already builds parents. If the folder
  cannot be made, no move is sent: the dialog stays open with the name still typed in it, and says what went wrong
  inside itself rather than closing on a "done" that did not happen.
- **Settings is a panel now: Ctrl+comma, or the header button.** Three controls:
  1. **How search matches** — *words only*, *both*, or *meanings only*. This is the first release where the choice
     reaches note-processor: until now Jott asked for `hybrid` whatever you wanted, which means the option you can now
     see was not previously a thing. Your choice is remembered per app and re-runs the query in the box immediately.
  2. **Match word beginnings** — the same checkbox that has been beside the search box since 0.0.15, now also in here.
     It writes one setting, so the two places cannot disagree after a restart.
  3. **Embeddings** — *Allow the download* and *Keep it keyword-only*, which are the same two answers as the consent
     card's buttons and go through the same door: note-processor writes its own settings, Jott writes
     `models.auto_fetch`, and the panel quotes np's answer rather than a hope about what the click did.

**About *meanings only*, which is the one that cannot be honest yet.** Ask note-processor for meanings only in a folder
with no vectors and it answers with nothing at all — `"result": null`, no hits, no `indexed`, no reason, nothing on
stderr. That is [note-processor#11](https://github.com/Ex-Bot-01/note-processor/issues/11), filed while building this;
until it is fixed, Jott says what it can and does not fill the gap in:

- The panel shows a warning under that choice when your last search told it this folder has no vectors: *this folder
  has no vectors yet, so meanings-only search will find nothing here*. Before you have searched at all it says nothing,
  because Jott genuinely does not know and "no vectors" from silence would be an invention.
- An empty meanings-only answer never says *"nothing matched"*. It says either that the folder has no vectors to
  compare against, or — when np said nothing at all — that np returned nothing and gave no reason.
- A refusal is quoted with the issue number in it, so you can read why rather than wonder whether Jott broke.

**Seven things worth trying:**

1. Open a note, press **Escape**: it closes. Open another, open the settings panel with **Ctrl+,**, press **Escape**:
   the panel goes and the note stays. Press **Escape** again: *now* the note closes.
2. Click into the search box and press **Escape**. The note behind it should stay open — the key belongs to the box.
3. Open `INDEX.md`. It should read, be marked read-only, and refuse **Ctrl+S** with a sentence about whose file it
   is. Its row should still look quieter than your notes, its right-click menu should offer nothing, and **rename** and
   **delete** in the toolbar should stay disabled while it is selected — then select one of your own notes and watch
   both come back.
4. Press **Escape** to cancel a dialog: open *Move to…* and press it. The dialog closes, the note behind it stays open,
   and nothing is sent. Then press **Escape** again — *that* is the press that closes the note.
5. In a vault with no folders, right-click a note → **Move to…** → type `ideas` in the new field → **create and move**.
   The folder appears and the note is inside it. Then do it again with a name that cannot work (`..`) — the dialog
   should stay open, still holding what you typed.
6. Open **Settings**, choose *words only*, search something whose only match is a longer word: nothing should come
   back, and no line should claim the folder was searched by meaning. Choose *both* and the row should return. Quit and
   reopen Jott: it should still be on *words only*.
7. In a folder you have indexed but never embedded, search once so Jott learns there are no vectors, then open
   **Settings** and pick *meanings only*. You should get the warning under that choice, and an empty answer whose
   sentence blames the missing vectors rather than your notes.

## 2b-vii. Since 0.0.17: the first five minutes

Four reports, all from the part of Jott you only meet once — before and just after you open your first folder. The
backend inside is still **note-processor 0.5.1**; this is all window.

**A button that cannot do its job now says so.** With no folder open, **+ note** / **+ folder** used to accept a name,
press Enter, and produce silence. The window had the sentence — *"no vault is open"* — and drew it nowhere: the only
place it rendered needed a folder to be open first. Now the dialog itself carries the reason:

> No folder is open yet, so there is nowhere to make this. Choose one, and it will be made inside it.  **[Choose a folder]**

…with what you typed kept, so choosing a folder finishes the note you asked for instead of starting it over. Nothing is
sent while there is nowhere to put it. Any other refusal (a folder that went missing, a path that could not be opened)
appears in one place above the search box, whether or not a folder is open — check that by picking a folder that no
longer exists from the recents list.

**A new folder appears the moment you make it, and looks like a folder.** Two separate faults were behind "nothing
happened": a successful `create_folder` was being read as a failure (its whole answer is *nothing*, which crossed the
wire looking exactly like this window's marker for *failed*), so the tree was never asked again; and a folder row's only
visual distinction was bold text, with no twisty until it had children. Now:

- `▸` / `▾` for a folder with things in it, `·` for one that is empty, nothing at all for a note;
- an empty folder says `(empty)` beside its name — it is empty, not missing;
- the tree re-lists after the create, without needing another command to do its job.

**Clicking a folder means it.** One click selects the folder *and* opens or closes it, which is what one click has
always meant in a Windows file tree. The toolbar follows the selection: its tooltip reads **New note in notes (Ctrl+N)**,
the dialog label says `New note in notes/:`, and the file lands there. Open a note and the destination goes back to the
vault root, so the tooltip cannot promise one place while the button writes to another.

**A note gets a name that is a note.** Typing `trip plan` creates `trip plan.md`. An extension you typed on purpose is
kept exactly — `todo.txt` stays `todo.txt` — and the window then says what happens next rather than letting you discover
it by searching: *"Created todo.txt. note-processor reads only .md files as notes, so this one will not be searchable."*
Spellings note-processor cannot read are folded and announced (`hello.markdown` → `hello.md`, said out loud). Names that
cannot become files are refused in words with the dialog still open: `notes.`, `.`, `..`.

**A folder full of other formats explains itself.** If any file in the folder is not a note, under the tree:

> Of 12 files here, 3 files are notes note-processor can search; 9 are kept as attachments — tracked for links, never
> searched. note-processor reads .md files as notes.

That sentence matters most to whoever opens a folder of `notes.txt`, `ideas.org` and `scan.pdf` and finds search returns
nothing: the folder is not empty, nothing failed, and only `.md` is parsed. Measured against 0.5.1 for exactly that mix:
`scan` answers `added: 5, notes: 1`. Jott does not convert anything, and the rule itself is filed upstream as
[note-processor#12](https://github.com/Ex-Bot-01/note-processor/issues/12).

### Six things worth trying

1. **Uninstall-and-first-run check.** Quit, rename or move your recent-folder list away if you want the true first run:
   press **+ folder**. You should get the sentence and a **Choose a folder** button, no new anything on disk, and your
   typed name still in the box after you pick.
2. **Make a folder and look at it immediately.** `·` and `(empty)`, no other command needed. Add a note inside: it
   becomes `▸`/`▾` and stops saying `(empty)`.
3. **Click a folder, then check the tooltip** before pressing it: it should name that folder. Create one and confirm the
   file really lands inside it, not at the root. Then open a note and confirm the tooltip goes back to *the vault root*.
4. **Type `trip plan`** → you get `trip plan.md`. Type `todo.txt` → you keep `todo.txt` and get a sentence. Type
   `hello.markdown` → you get `hello.md` and it says so. Type `notes.` → nothing is created, and the reason is on screen.
5. **Open a folder with mixed formats** (a few `.md`, some `.txt`, an image). One line under the tree, counts plausible,
   and it must not read as blame or as an error. A folder of only `.md` says nothing at all.
6. **Move something.** Since 0.0.17 a move that works closes its dialog and refreshes the tree — in 0.0.16 a successful
   move left the dialog sitting there over a tree that had already changed, because *success* and *no answer* looked the
   same to the window.

## 2b-viii. Since 0.0.18: five asks from inside the editor

These came from someone with notes open and opinions, so they are about the window you are already using.

1. **The word-beginnings checkbox is in Settings now**, not beside the search box. The toolbar kept only the things you
   *do*; how a query matches is a setting. The offer under an empty result — *try matching word beginnings* — stays where
   it is, and pressing it still does not move your remembered choice.
2. **The note pane has its own close button.** It names the file at the top of the pane, says `read-only` for np's own
   `INDEX.md`, and puts **close** at the right edge. Escape still works, and with nothing open there is no button at all
   rather than a greyed one. The old sidebar **close** is still there for whoever reaches for it first.
3. **A number beside every search result.** That is note-processor's own figure — the fused keyword-and-meaning ranking
   that decided this row comes first. Hover it: the tooltip says what it is, including that it is **not a percentage**.
   On rows where meanings were compared you also get `meaning 0.82`. On a folder with no vectors you get no `meaning`
   figure at all, because there was nothing to measure; if you ever see `meaning 0`, that is a bug — say so.
   The honest answer to "how similar is it?" is that np quotes a rank fusion around `0.0164` for a keyword hit, and turns
   it into a percentage in nobody's interest but a screenshot's.
4. **A dot in the bottom bar with a word next to it**, and clicking it opens Settings. Six things it can honestly say:

   | word | what it means | where the fact came from |
   | --- | --- | --- |
   | `meanings on` (green) | the model is on your disk **and** notes here have vectors | np + Jott's per-file markers |
   | `model ready` (teal) | the model is on your disk; nothing in *this* folder is embedded yet | np's `models.list` |
   | `model not downloaded` (amber) | embeddings are allowed, the weights are not here — and it names the size | np's pinned table |
   | `embeddings off` (grey) | np's settings name no provider; search is words only | np's config file |
   | `model state unknown` (hollow) | **nobody could say**, and the tooltip names which of four: np did not answer; its settings file will not parse; np is running but cannot list its models directory (its own sentence is quoted); no model was named for this folder | nothing trustworthy |
   | `another provider` (blue) | embeddings run through something other than np's built-in `local-model`, so Jott will not pretend to know where its weights live | np's config file |

   The grey row is the one to argue with, because it is the only one that carries advice: `embeddings off` claims np's
   settings genuinely name no provider. If your file says otherwise — or will not open as TOML at all — you should get
   `model state unknown`, never a suggestion to turn on something that may already be on. And if you allowed the download
   and the dot says *unknown*, that is an np problem to report, not a reason to fetch 296 MiB again.
5. **The caret.** Third attempt, and this time the cause turned out to be ours. Jott never asks CodeMirror to draw its own
   caret — the extension that does it, `drawSelection()`, is not installed — so the caret has always been the browser's,
   and since 0.0.16 our editor stylesheet hid it with `caret-color: transparent`, under a comment claiming CodeMirror
   hides the native caret itself. Every earlier attempt recoloured a caret element this app never creates. It is now ink in
   both schemes, and Windows high-contrast mode gets the system's text colour.

   Two releases went by on a wrong premise, so treat this as *measured but not seen*: the machine that built it has no
   display. If there is still no caret, check 6 below says which bug is left.

**Six things worth trying**

1. Search for something and read a row's number, then hover it. It must not look like a percentage anywhere.
2. Type into a note. Is there a caret? Then click into the search box and back: does it come back? (Clicking between two
   lines should never lose it either.)
3. Look at the dot with a folder you have never indexed: `embeddings off` or `model not downloaded`, depending on what
   you answered when asked. Then press `[embed]` and watch whether it changes without restarting.
4. Click the dot. Settings should open.
5. Open `INDEX.md`: the pane header should say `read-only` and **close** should still work.
6. If the caret is *still* missing — **Ctrl+Shift+I** (which opens a console in an installed Jott only from 0.0.19 on:
   earlier builds had none, whatever their config said — see §2b-ix), paste this into the console, then click back into your note and
   type. It waits three seconds before reporting on purpose: clicking the console takes focus away from the editor, and a
   snapshot taken at that moment describes an unfocused window rather than the thing you were looking at. Send me the one
   line it prints — this box has no screen, and I would rather fix the right bug than ship a fourth guess.

   ```js
   setTimeout(() => {
     const e = document.querySelector('.cm-editor')
     const n = document.querySelector('.cm-content')
     const c = e && e.querySelector('.cm-cursor')
     const cs = c && getComputedStyle(c)
     console.log(JSON.stringify({
       editorClass: e && e.className,
       drawnCaret: cs ? cs.display + ' ' + cs.borderLeftWidth + ' ' + cs.borderLeftColor : 'no such element',
       nativeCaret: n && getComputedStyle(n).caretColor,
       paperBehindIt: n && getComputedStyle(n).backgroundColor,
     }))
   }, 3000)
   ```

   What it answers: whether a `.cm-cursor` element exists at all (it should read `no such element`, because Jott does not
   ask CodeMirror to draw one), and what colour the browser's own caret was told to be. `nativeCaret: "transparent"` means
   something is still hiding it; an ink colour with nothing visible points at something else — the paper colour behind it,
   say — which is a different fix again.

## 2b-ix. Since 0.0.19: the window arrives once, in the right place

Reported by you, from a fresh install: *it opens as a white window, then jumps about 5 cm to the right, then opens
correctly.* The jump is fixed and its mechanism is measured line by line. **The white is not fixed**: my first explanation
of it was wrong — checked one crate too shallow, see `docs/release-notes.md` — so this section asks you to say *which*
pale you see rather than promising one of them has gone away.

**What should happen now**

1. **No slide, when there is a saved position to restore.** Drag the window somewhere, close Jott, reopen: it should come
   back exactly there, and nothing should move after it appears. That is the plugin doing its job while the window is
   still hidden — which is the whole change.
2. **The very first run never moved, and never will,** because with no saved state there is nothing to restore. An
   install *over* an older Jott keeps that file: `%APPDATA%\dev.jott.desktop\.window-state.json`. To test from a genuinely
   fresh state, quit Jott and delete that one file — note it lives under `dev.jott.desktop`, not in the `%APPDATA%\jott`
   folder the rest of this guide describes.
3. **Say which pale you see.** In the moment before the page paints: flat white, like a browser with nothing in it, or
   warm near-white paper (`#fafaf9`)? Either is still possible and the difference decides what to investigate next, so
   look at it deliberately — start Jott while Alt-Tabbing back in, or record the screen if you can. On a dark-mode
   desktop, light paper for those frames is expected and is not a new bug: nothing in Jott knows your colour scheme until
   the page exists to be asked.
4. **One case where moving after appearing is normal.** A monitor change or a scaling change — plugging in a projector,
   moving between screens with different scales, dragging the DPI slider — makes Windows reposition and resize a window
   *after* it is shown. If your setup has two displays at different scales, say so: that is a different bug from the one
   fixed here, and this release does not claim to have it.

**If it still misbehaves, send one line instead of a description**

Every launch now records what the window came up as:

```powershell
Get-Content $env:APPDATA\jott\logs\jott.jsonl | Select-String '"event":"window"' | Select-Object -Last 3
```

It looks like this:

```json
{"event":"window","detail":"1200x760 with its top-left at +312+180; state file present","at_ms":1731000000000}
```

`no state file yet` on a second launch means the position was never saved, which is a different bug from a jump. Two
launches' worth of that line — before and after you moved the window — tells us more than any description of the move.
A screenshot of the first frame, if you can catch one, is worth having too: nobody building Jott has a display, so your
screen is the only evidence there is.

**A console, at last.** **Ctrl+Shift+I** (or right-click → Inspect) opens one in the installed app from this release.
It never worked before: Jott's config asked for devtools, and a release build ignored that unless tauri's `devtools`
feature was compiled in — which it was not. So instructions in §2b-viii and elsewhere that said "press Ctrl+Shift+I and
paste this" could not be followed on 0.0.4 through 0.0.18, and nobody should have felt clumsy for failing to find one.
Anything in this guide that asks for console output now means it.

**What was deliberately not done.** The standard trick for a white first frame is to keep the window hidden until the
page says it has painted. That turns a broken bundle into an app with no window at all — which is how 0.0.8 shipped a
blank screen nobody could diagnose — so Jott shows its window immediately, unconditionally, and if paper still does not
land the next step is a reveal-with-timeout rather than a hope.

## 2c. Since 0.0.11: press **Ctrl+Shift+D** when something is odd

A drawer opens at the bottom of the window with what this run has actually been doing:

- which np file it is running, its version and protocol, and whether each connection is connected, not started,
  or gone;
- every request it made to np — `scan`, `embed`, `search` — with how long it took, and np's own error sentence on
  a fault, in full;
- what np said while it worked — `fetching 128 / 295 MiB`, `embedding chunks 41/100` — which is the line you want
  open *during* a long wait, not described to me afterwards;
- whether anything is queued, whether np has your folder locked, when Jott will retry;
- how many notes are indexed and embedded, and whether any folder could not be read;
- where the log file is, how big it is, and buttons to copy all of it or show the file in the file manager.

**Press it while something is happening.** A save that has sat at "saving…" for a minute, or an *Index now* that
has not moved any dots. The drawer shows this run's history either way, but seeing the request arrive and then not
finish is a different diagnosis from seeing no request at all — and the second one means the button, not np.

It cannot show the contents of a note, and it never will: the rows are the same records that go into the log file,
whose key set is fixed by tests on both sides. If you ever see a sentence in there that looks like your writing,
tell me — that would be a bug, not a feature.

## 2b-x. Since 0.0.20: the skin — controls, file names, and dark mode's own colours

This release changed how the window is dressed, not what it does. I looked at it in a browser on Linux, because this
build box cannot paint a webview at all, so **you are the first person to see it in WebView2**. Four things worth a
look, roughly in the order you will meet them:

1. **The toolbar.** `+ note`, `+ folder`, `rename`, `delete`, `close` and `Index now` should each sit on one line,
   about the same height as the search box beside them, and disabled ones should look plainly unavailable rather than
   merely faint. Before 0.0.20 several of them wrapped in half in a narrow window.
2. **`INDEX.md` in the tree.** It is drawn italic on purpose — it is note-processor's own catalogue, not yours. Until now
   the italics made the box clip its own last letter, so it read as `INDEX.ma`. If you still see a name with a piece
   missing, that is the bug still living, and I want the screenshot.
3. **Writing a note.** Prose is now in the same face as the rest of the interface, at a width of roughly 74 characters,
   with code blocks left monospace. Two things to tell me either way: whether it reads better at your font size, and
   whether anything you write looks *worse* — a table, an indented list, a note full of front matter. This is the most
   opinionated change in the release and it is one rule to undo.
4. **Dark mode, if you use it.** Set Windows to dark (Settings → Personalization → Colors → Choose your mode → Dark)
   and look at the amber items: an unsaved note's marker, the "changed outside Jott" banner, the read-only
   note-processor index notice. Those were meant to be a brighter amber on dark paper since 0.0.12 and never were —
   the rules were written against a class nothing in Jott adds. If they now look noticeably *too* bright, that is the
   same change landing for the first time.

Two things I could not check from here, so treat them as open rather than fixed: how text is hinted by ClearType at
125% and 150% scaling, and what happens in **high-contrast** mode (Settings → Accessibility → Contrast themes). Earlier
releases added explicit rules for the caret and the status-line dot there; if a control now looks like a grey blob in a
contrast theme, that is new and it is my doing.

Nothing about your files moved: same np 0.5.1 inside, same index, same search. If something *behavioural* looks
different, that is a regression and worth a log file — `Ctrl+Shift+D` → Copy all.

## 2b-y. Since 0.0.21: deleted notes stop turning up in searches, and the dots explain themselves

**The one behaviour change.** Delete a note — inside Jott, or in Explorer — then search for a word that was only in
it. It should be gone from the results within about a second of the delete. Before 0.0.21 Jott moved your file to the
trash and never told note-processor, so the note kept answering searches until some unrelated save happened to walk
the folder.

Three things worth trying, because each takes a different path through the code:

1. **Delete one note**, wait a second, search for its most unusual word → no hit. Then open the Recycle Bin and put
   the file back: it should be findable again on the next save or after *Index now*.
2. **Rename or move a note** into another folder, then search for a word inside it → exactly one result, under its new
   name. Not two (the old path still answering) and not none. A move is not a deletion; nothing in Jott throws away
   embeddings because you tidied a folder.
3. **Delete five notes quickly**, then search. This should cost note-processor one walk of the folder, not five — so
   on a big vault it should feel like the first case, not like waiting.

If a deleted note still answers a search, press `Ctrl+Shift+D` and copy the log: what I need is whether a `scan`
line appears after the `deleted` one. That single detail says whether Jott failed to ask or note-processor was asked
and said nothing.

**The dots beside your files now have colour, and a key under the tree explains them:** green filled = embedded,
teal half-filled = indexed and waiting to be embedded, hollow grey = not in the index yet, amber `!` = note-processor
refused it (hover for its reason). Two questions I can only ask you:

- Are those four still tellable apart at your Windows text scaling — try 125% and 150%, and the tree with a long file
  name so the dots are pushed to the edge of their column?
- Does anything in the bottom-left corner overlap or get cut off when the status sentence is long? It now carries
  your matching choice ("Words only", "Both", "Meanings only") as well as the model state, and they read as two
  separate facts on purpose.

**And a wording change that was reported as a bug:** the green dot used to say "meanings on", which made switching to
*Words only* in Settings look like a toggle that had not taken effect. It never meant that — it reports whether the
model is on your disk and whether this folder has notes with vectors. It now says "notes embedded", and the matching
choice appears beside it, so both facts are visible at once.

Nothing else about search changed. note-processor is still 0.5.1 inside; keyword ranking was already bm25 (the TF-IDF
family with length normalisation), and its figure now sits in a result's tooltip rather than on the row.

## 3. One thing worth watching: np processes

Jott starts np when it opens and keeps one running; a second starts the first time you search and stays until you
switch folders or quit. When you close a folder or leave the app, they should go away within a second or two. If
Task Manager ever shows a `np.exe` still running after Jott has closed, that is a bug worth telling me about — on
Windows it means your vault's index stays locked for the next np.

**A second `jott.exe` in that list is now the thing to report.** Until recently, double-clicking the icon while a
window was already open started a *second* Jott. Nothing said so: the new window looked like a slow redraw, and then
the two of them quietly disagreed about your notes — both wrote the same stamp file, so a note one window marked done
could turn back into *waiting*, and the recents list lost whichever folder the slower one had never loaded. A second
launch now brings the existing window to the front (and if that ever stops working: two `jott.exe` lines in Task
Manager, please). If you are building Jott from source as well as installing it, expect the same rule — the lock is
named after the app identifier in `tauri.conf.json`, not after the `.exe`, so a debug build and an installed one are
the same program to Windows. That is deliberate here: two of them pointed at one folder is exactly the disagreement
this fixes.

## 2b-z. Since 0.0.22: indexing stops chasing its own tail, and a download you cannot miss

You reported two things in the same message, and they turned out to be one bug wearing two hats. I have kept them
apart below because the evidence for each is different.

**What you told me:** after installing over an old copy, Jott never asked about the model download; there was no
progress bar; and `jott.jsonl` grew continuously even after every file looked embedded.

**What your log said:** 6,885 `scan` lines in 142 seconds — about 48 a second, for over two minutes. That is Jott
and note-processor chasing each other. np renames a temporary catalogue (`INDEX.md.tmp`) into place on every scan;
my filter from 0.0.21 only recognised names ending `.md`, so a vanishing temporary read as "a note was deleted
outside Jott", which asked for another scan, which vanished another temporary. The file you watched growing was the
sound of that loop.

**And why nothing appeared on screen:** every one of those scans also sent an event to the window. When the queue of
events overflowed, the bridge between core and the webview treated that as "finished" and stopped delivering — so
your 295 MB download ran in a window that had gone deaf. That is not np's doing and it is not yours either.

### What to try

1. **The loop should be gone.** Open a folder with a few dozen notes, click **Index now**, and ten seconds apart run:

   ```powershell
   (Get-Item "$env:APPDATA\jott\logs\jott.jsonl").Length
   ```

   The number should creep up while work is happening and then stop. If it climbs by hundreds of thousands of bytes
   every second with nothing left to index, send me the file — that means the loop found another door.

2. **The download strip.** In Settings choose **Allow the download** (or click the button on the consent card).
   Within about two seconds a strip should appear across the top of the note pane, in bold:

   > **downloading embeddinggemma-2 · 137 of 295 MiB** [████████░░░░]

   It should keep moving until the model is on disk, then disappear. The numbers are np's own byte counts — no
   seconds-remaining figure appears because nobody can measure one honestly.

3. **Close Jott in the middle of a download and open it again.** You may see the strip for two or three seconds and
   then watch it vanish. That is deliberate: np keeps a marker file while fetching, and a marker left behind by a
   killed process says "bytes were counted", not "bytes are moving". The window only shows the strip while the count
   is actually advancing, because a bar that never clears teaches you to ignore the one light that means *wait*.

4. **Search after a delete still works** (that was 0.0.21 — §2b-y). Deleting and renaming no longer set the scan loop
   off either, so if a ghost comes back it is more likely to be a stale index than a storm.

### If the strip never appears but embedding clearly works

Send the log (**Ctrl+Shift+D** → **Copy all**), and say roughly what time you clicked Allow. The lines that matter:

| Line | What it tells me |
|---|---|
| `"event":"rescan"` with a `rel` | the watcher asked for a walk, and which path made it ask |
| `"event":"events-lagged"` | the window fell behind; 0.0.21 would have gone deaf here, 0.0.22 carries on |
| `"method":"models.list"` rows | whether np was asked about the marker at all |

### And since 0.0.24: *Allow the download* starts the download

One sentence because it matters: in 0.0.23 clicking Allow recorded the settings and then waited for a save or an
**Index now** before anything fetched — so with the receipt card gone the click looked like it had done nothing. It now
starts the pass straight away, and the strip should appear within about a second of the first bytes moving. If you
click Allow and see no strip within five seconds, that is the bug worth sending me a log for.

### And since 0.0.23: the receipt card after "Allow the download" is gone

You should now see **only the download strip** after clicking Allow — no paragraph about which settings np recorded,
and no *Copy what happened* / *Try again* / *Read the note-processor request* buttons. That wording is still there if
you want it: **Settings → embeddings** says what np recorded and into which file.

Two cases where the card still appears, on purpose:

* **note-processor refused.** Its own sentence, unchanged, with *Try again* beside it.
* **Jott could not write its own guardrail** (`models.auto_fetch`), or found no settings file to write it into.

If you click Allow and see *neither* a strip nor a card, that is the bug to tell me about — say roughly what time.

## 2b-za. Since 0.0.25: the consent card says one thing, and rows show one number per measurement

**The card.** It should now be a sentence and a button: *"Meaning search is off: nothing has been allowed to download
yet…"* with **Allow the download** beside it. No key names, no block of settings to paste, no GitHub link. Pressing
that button should start the download straight away — the strip across the note pane within about five seconds.

**Where the detail went:** Settings → embeddings. It still names `models.auto_fetch`, the file np read and what is
missing, and it has **Copy these settings** for when you need the text out of the window.

**Two things the card deliberately does not offer a button for:** a settings file note-processor cannot parse (it gets
*Try again*, because nothing can be written until that file is fixed) and an embedding engine Jott does not recognise —
pressing Allow there would overwrite a choice somebody made on purpose.

**Search rows.** With **Meanings only**, a row now shows one figure — `meaning 0.83`. With **Both**, it shows the fused
rank *and* the meaning figure, because those are two different measurements. With **Words only**, one figure whose
tooltip says keyword ranking. Hover any of them: the tooltip names which measurement it is and repeats nothing as a
percentage. If you see the same number twice on one row again, tell me which mode was set — that means note-processor
started returning something new.

## 2b-zb. Since 0.0.26: the window takes the shape you give it

**The sidebar.** Between the file list and the note there is now a thin strip that changes your cursor to a resize
arrow. Drag it right: the list gets wider and every label stays on one line — `Index now`, `+ note`, the legend. Drag
it as far left as it will go (266 px) and check what breaks: nothing should read as cut off, though a long folder name
will end in an ellipsis, and hovering it still shows the whole thing. **Close Jott and open it again: the width you
left comes back.** Double-click the strip, or click it and press `Home`, for the default.

**Keyboard, if you want it:** click the strip once, then `Left`/`Right` nudge 16 px (`Shift` + arrow = 48 px), `Home`
resets. Then put the caret in a note or in the search box and use the arrows to read along a line — the columns must
not move. That separation is deliberate.

**The diagnostics drawer.** `Ctrl+Shift+D`. It should now span the **whole width** of the window, with the runtime
facts on the left and the activity rows on the right (on a narrow window they stack), and the grey status strip should
still be the last thing at the bottom edge — not above the drawer. Read the **log** row: a path like
`C:\Users\chris\AppData\Roaming\jott\logs\jott.jsonl` should now fit on **one line**, because the drawer is wide
enough for it. If your window is narrow enough that it has to break, it may break anywhere in the path — that is what
CSS does with a word longer than the column, and 0.0.26 only changed *where* it prefers not to break (mid-word when
there is room, which there now is). What I want to hear about is a path that is broken even in a wide window.

**Its height is yours.** Put the cursor on the drawer's top edge until it turns into a resize arrow and drag up: you
should get **more activity rows**, not more empty space — and the edge should stay under your finger from the moment
you press it, with no jump. Drag it down to the floor (180 px) and let go. Going up, it stops at 70 % of the window or
at whatever leaves the note pane 200 px; in a very short window the 180 px floor wins instead, so on a small screen —
or a window you have dragged down near 300 px — the drawer can take more than half of it. That is the floor doing its
job, not the ceiling failing: tell me if the *note pane* vanishes or the bottom status strip leaves the screen at any
window size, because that one is a bug. `Home` while the edge has focus returns the default 320 px; double-clicking it
does too; reopen the drawer and your height is still there.

**About → Diagnostics.** Press **About**, then **Diagnostics**. Expect exactly one thing on screen: the drawer, with
About gone. While you are in About, press **Copy details** — the panel must stay open (before 0.0.26 any click inside
it dismissed it, including that button). Click the dark area outside the panel and it closes, as it should.

If the drawer opens with a black line across the top of it, or grabbing its edge starts highlighting text in your note
instead of resizing, that is a regression — tell me and say which window size you were at.

## 2b-zc. Since 0.0.27: formatting without asterisks

There is a bar above the note now, under its name: **B** *I* `</>` **link** **•** **☑** **H**. It writes markdown into
the file — it does not hide any, and there is no preview mode. What I want from you is whether it does what pressing
it suggests.

1. **Bold a word you are already looking at.** Click into a sentence, put the caret in the middle of a word — do not
   select anything — and press **B**. The word should come out `**like this**`, still bold on screen, with its asterisks
   faded. Then press **B** again: they should go away and leave the word exactly as it was, spaces included.
2. **Keep typing inside them.** Put the caret at the end of a line, in the whitespace, and press **B**. You should see
   `****` with the caret between the two pairs, and everything you then type should land inside them. Press Right (or
   End) to get out. If instead it starts writing outside them, that is a bug — say what you typed.
3. **Watch the button agree with the screen.** Click into a bolded phrase: **B** should look pressed. Click into
   ordinary text two words away: it should not. Same for *I*, and for `H` on a line starting `##`. A control that is
   lit when nothing there is bold is worse than no control, so tell me about any mismatch even if the formatting itself
   is right.
4. **Use the keyboard.** `Ctrl+B`, `Ctrl+I`, `Ctrl+E` (inline code) and `Ctrl+K` (link) should do what the buttons do.
   `Ctrl+K` on a selection gives you `[the words]()` with the caret between the parentheses, so you can type or paste
   the URL there — no dialog box appears, deliberately.
5. **Press Enter in a list.** Type `- [ ] buy flares` and press Enter: the next line should start `- [ ] ` on its own.
   Backspace at the very end of a bolded word should take both asterisks off at once, not one. Neither of those is new
   in 0.0.27 — they have worked since the markdown language went in, and I only wrote them down now — so if either has
   *stopped* working, that is this release breaking something, not this release failing to add it.
6. **Undo.** After pressing any control, `Ctrl+Z` once should put the text back exactly as it was — markers and caret
   together. If the words come back but the caret jumps somewhere odd, tell me which control you pressed.
7. **Open note-processor's own index** (`INDEX.md`, greyed in the tree). There should be **no bar at all**, and
   `Ctrl+B` should do nothing to it; the name row stays, because reading it is allowed. Then open an ordinary note
   again and confirm the bar comes back.
8. **The note's name.** The row above the editor shows the file name, with its folder muted in front of it when the
   note is not at the top of the vault — `field-log/scratch.txt` should show both halves. It should say it **once**: if
   you see the filename twice, that is the regression I fixed on the way to this release and it has come back.

What is a preference rather than a bug: the `☑` glyph is small and a little faint in dark mode, and `H` steps only
between `##` and `###` — `#` stays something you type yourself, so that a heading button cannot demote a note's title.
If either bothers you in practice, say so; it is a design argument, not a fault.

## 2b-zd. Since 0.0.28: the markdown styling starts working

This release changes no bytes in your notes. It switches on the styling that has been written since 0.0.9 and never
reached the screen, so what I need from you is mostly an eye test — including "it looks worse now", which is a
legitimate answer and not a bug report.

1. **Open a note with headings in it** — one with a `#` line and a `##` line under it, then a normal paragraph. The
   heading should be visibly *bigger* than its own paragraph, and none of them should be underlined. If every heading
   is the same size as prose, or has a line under it, this release hasn't taken effect on your machine.
2. **Check the underline belongs to links only.** A `[some label](somewhere.md)` link should have its label
   underlined and its brackets and address quieter than the words. Underline should mean "there is somewhere to go"
   and nothing else in the window should use it.
3. **Look at the markers, not just the words.** The `#` in front of a heading and the `**` around bold text should
   still be there — they are never hidden — but dimmer than the text they wrap. If they are as loud as prose, or have
   vanished, say which one.
4. **Wide window: does the format bar sit above your text?** Maximise the window and open a note. The row of buttons
   (`B` `I` `</>` link `•` `☑` `H`) should begin roughly over the first character of the buffer, not out in the empty
   margin to its left. On a wide window both move inward together — that's the text column being centred, not a bug.
5. **Pressed versus focused.** Click into a heading so `H` lights up. Then press Tab until the caret leaves the buffer
   and lands on a button. The lit one and the focused one should look like different things — pressed is filled with a
   shadowed inner edge, focused has a ring drawn outside it. If they still look the same to you, tell me which pair
   you compared; that's the finding I care about.
6. **Now the honest question.** Read a note you actually wrote and say whether it looks better or worse than it did in
   0.0.27. Too-big headings, too-grey code, list bullets shouting — all worth knowing. Nothing here is written in
   stone; it was designed in 0.0.9 and nobody had ever seen it until this week.

## 2b-ze. Since 0.0.29: the backend is np 0.6.0, and eleven issues close

The np inside Jott is **0.6.0**. Protocol is still version 1 and the handshake is the same shape Jott already parses,
so there is nothing for you to do and no folder to re-index: an index built by 0.5.1 opens in 0.6.0 with every note
reported `unchanged`, which was checked against the published binaries here before this shipped.

Three things are worth noticing, and one of them I could not check myself:

1. **The About line reads `note-processor (np) 0.6.0`.** If it still says 0.5.1 after installing 0.0.29, the
   installer carried the old binary — that is a packaging fault worth reporting, not something to work around.
2. **`INDEX.md` may rewrite itself once.** np writes that file; Jott only greys it out in the tree. Names containing
   `#`, `?`, `[`, `]`, `(`, `)`, `<`, `>` are now written percent-encoded, so a note called `roadmap#2.md` shows there
   as `roadmap%232.md`. No note is renamed and nothing else in the window changes. If you keep `INDEX.md` under git,
   expect one changed line per unusual name on the next index — measured here, exactly one line for a vault of five.
3. **A meanings-only search with nothing embedded now says why instead of saying "Nothing matched".** np 0.6.0
   answers a sentence of its own and Jott prints it verbatim. If you still get `Nothing matched “…”` in a folder that
   has no vectors, tell me what you typed and which build — that means the sentence did not arrive.

The rest of this release is the eleven open issues closed: quitting saves what you typed (#1), a recent folder that no
longer exists keeps your current vault open (#2), nested notes re-index after a save on Windows (#3), `nul.md` and
`AUX.md` refused with an explanation (#5), a Content-Security-Policy on the window (#6), one Jott per machine (#7),
renames off the UI thread with a journal line that counts them (#8), private permissions on unsaved drafts (#9),
symlink rows that admit what they are (#10), and a scan answer that can no longer be misread as "everything indexed"
(#11). Sections 1, 3 and 4b above are the parts those changes touch: if you checked them on 0.0.28 and they looked
fine, look at the same places once more.

One thing I know about and have deliberately not papered over: opening a folder for the very first time *and* typing
in the search box before its first index finishes can answer `note-processor refused: sqlite: table files already
exists`. It is upstream (note-processor#18), it happens with 0.5.1 inside as well, and the next keystroke clears it.
Filed as jott#23 with the transcripts. My measurement is about one try in eight — if you can make it happen twice in a
row in a real folder, that is a different finding and I want to hear about it.

## 2b-zf. Since 0.0.30: a result row says which half of search found it

Search results now carry a small mark on the rows that need one: **"from the start of a word"** (np widened your word,
as before) and **"by meaning only"** (nothing in this note matched your words — np reached it through vectors). Rows
where the typed words were simply there stay unmarked.

1. **Search for one word that is definitely in one note** — a surname, a project name. That note should be first and
   carry no mark. Anything below it marked *by meaning only* was reached without your words at all.
2. **Now the honest test of what meanings are worth.** Search the same word with the engine set to *meanings only*. In
   my folder of four German notes (a birthday table, a recipe, a shopping list, three lines of chatter) that put a note
   containing neither word on top: cosine 0.7195 against the calendar's 0.7101 — nine thousandths apart, which is
   nothing. Two words (`Geburtstag Familie`) separated them by 0.09 and the right note won. So if a meanings-only list
   looks arbitrary to you for a one-word query, it is not you; that is what this model does, and `note-processor#19`
   carries the measurements. Hovering a meaning figure now says so in as many words.
3. **Nothing about your index changed.** No re-index, no re-embed, same np 0.6.0 inside. This is a window change only.

If you think the new mark is noise on your results — for instance because nearly every row gets one and the pane looks
busy — tell me which kind of folder that is. It is a fair criticism and the wording is not sacred.

## 4. The question only Windows can answer: write-lock contention

np takes a lock on the workspace while it indexes, and on Windows that lock is a file others cannot open.
Jott must treat "someone else is indexing" as a state, not an error.

1. Open your vault in Jott and press `[embed]`.
2. While the status line says `embedding …`, open PowerShell in the same folder and run:

   ```powershell
   np scan .
   ```

   (An absolute path works the same way if you are not standing in the folder.)

3. What should happen: whichever of the two is waiting reports **`index behind · retrying in 2 s`** (then 5,
   15 and a minute at most — the ladder §11 names), the window stays responsive, and within a couple of
   minutes the markers settle. What must not happen: an error dialog, a frozen window, or a note that stops
   saving.

Then do it the other way round — `np scan .` running first, and a save in Jott while it works.

## 4b. If your vault is big: one rename, and what the log says about it

Jott has been tested against vaults of a few dozen notes. Renaming a note means answering "what refers to this?",
and that answer can only come from opening every note in the folder — a link written in `a/b.md` can point at
something in `c/`. Nothing about it is tuned for thousands of notes yet, and I would rather know than guess.

So if your vault is in the thousands: rename one note in a deep folder, and include the log. The line it writes now
says how much it read — `"detail": "from old/name.md; scanned 4312 notes"` — which turns "renaming feels slow" into
a number I can act on. Two things worth mentioning if you notice them: how long the window took to answer, and whether
the rest of Jott stayed usable while it thought (it should: the scan now runs off the thread that paints, so the
tree keeps moving).

## 5. What to send back

Open the drawer (**Ctrl+Shift+D**) and press **Copy details**, then send that text plus one file:

```
%APPDATA%\jott\logs\jott.jsonl
```

The drawer's text names the np binary and version, both connections, the consent answer, where Jott keeps its own
files, the log's size, what is queued, and this run's last requests and file work with their timings — which is most
of what I would otherwise ask you to describe. The log file is the same information in full history, so a bug that happened yesterday is still there.

That is the whole log. Three shapes of line share the file, and each is recognisable by the key only it has:

* **a request to np** (`method`) — which vault, how long it took, the outcome (`applied`, `locked`, `offline`,
  `disabled`, `refused`, `fault`), and np's own error sentence on a fault.
* **the start of a run** (`jott`) — Jott's version, the np version and the exact `np.exe` this run talked to and how
  it was found, your platform, and the folder Jott keeps its own files in. Look here first when a report spans "it
  worked yesterday": each day's run names its own build and its own np.
* **file work** (`event`) — `save`, `created`, `write-failed`, `renamed`, `moved`, `deleted` (with where the note
  went), `mirror-replayed`, `mirror-discarded`, `consent`, `index-now`, `watcher-unavailable`,
  `state-root-fallback`. This is the shape that answers "I saved and my note vanished", which no request line can.

It rotates at 2 MiB and keeps older copies behind it (`jott.jsonl.1`, `jott.jsonl.2`). **It never contains the
contents of your notes**: a file line records which file and how many bytes, never what they spell. Each line is one
JSON object with a fixed set of keys and nothing else — 7 for a request, 9 for a run, 5 for a file event. The drawer
shows these same facts live; opening it
asks np nothing and starts nothing, so it is safe to press on a machine where np is missing or unwell — which is
usually when you want it.

## If the window shows nothing

Press **Ctrl+Shift+I**. Devtools are left on in these builds, and the Console tab is the difference between
"everything is white" and "`invoke` rejected `list_vault` with `npGone`". Send what it says; a screenshot of a
blank window tells me the window works and nothing else.

**First, open *About* in the sidebar header.** It says which np this run is talking to — version, and the full
path of the binary — plus where Jott's data and log live, and **Copy details** puts all of it on your clipboard
as a block you can paste straight into a message. That block answers most of what I would otherwise ask for.

Three things worth knowing before you file it:

- **A window the colour of paper, with no text at all**, on a Windows set to dark mode was 0.0.3's bug — white
  ink on near-white background. Fixed in 0.0.4; if you are on 0.0.4 and see it, that is a new bug and I want
  the console output (an installed Jott has had a console only since 0.0.19 — see §2b-ix).
- **A black console window titled `np.exe`.** Windows gives a console to any console program a GUI starts, and
  np is one. Fixed in 0.0.7 (`CREATE_NO_WINDOW`). On 0.0.6 that window was the helper doing its job quietly — nothing
  had gone wrong, but it looked like something had.
- **Markers lagging behind the status line.** In 0.0.6 the tree kept the stamps from when it was listed, so
  "completed" could sit above dots saying *waiting for the index*. 0.0.7 re-lists after a pass finishes.
- **"np ended before answering"** was Jott's own bug, not yours: it started np without telling it to `serve`,
  so np printed its usage and left. Fixed in 0.0.6, where the message also carries whatever np said on stderr —
  if you see it again on 0.0.6, that quoted text is the thing I need.
- **"Jott is starting…" that never goes away** means the bundle loaded but the app did not mount. That text is
  drawn by plain HTML precisely so this case has something to show.
- **An editor that loads unstyled** — no colours, no line heights, controls stacked — is what a document policy that
  is one directive too strict looks like: it fails quietly rather than loudly. The window now runs under a
  Content-Security-Policy (it used to run with none), and CodeMirror paints by writing its own `<style>` elements,
  so the policy has to allow inline styles. If you ever see this, open the console: a CSP refusal is the first line
  in it, worded as "Refused to apply inline style …". Same for a blank window whose console says
  "Refused to load the script".
- **A status line naming no np version** means Jott could not find np. The installer carries it, so on a normal
  install that is a bug — unless `NP_PATH` points somewhere that no longer exists, which wins on purpose.

## Uninstalling leaves your files alone

Uninstalling Jott removes the program and keeps everything under `%APPDATA%\jott` — on purpose. That folder can
hold a mirrored copy of text you had not saved, and notes Jott moved to its own trash folder because Windows
offered none; an uninstaller that deleted either would be destroying writing. np keeps its own
`%APPDATA%\note-processor` (its index and any model it downloaded) for the same reason.

To clear them once you are sure you want nothing back:

```powershell
Remove-Item -Recurse -Force $env:APPDATA\jott
Remove-Item -Recurse -Force $env:APPDATA\note-processor   # also removes np's index and models
```

## Also useful if something lost data, and only if it did:

| What | Where |
| --- | --- |
| The tree's per-file state | `%APPDATA%\jott\state\<a hash of the vault's path>\state.json` |
| Mirrored unsaved buffers (§18.13) | `%APPDATA%\jott\buffers\` |
| Files Jott trashed when Windows offered no trash | `%APPDATA%\jott\trash\` |
| Registered vaults and the consent decision | `%APPDATA%\jott\vaults.json` |

A screenshot of a status line that confused you is worth as much as the log.
