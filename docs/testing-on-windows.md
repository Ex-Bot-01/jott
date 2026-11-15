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

*(Historic: Jott ships np 0.5.1 since 0.0.15, so read every `0.4.0` below as `0.5.1`. The behaviours asked for here are unchanged — only the number on the About line moved.)*

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

## 3. One thing worth watching: np processes

Jott starts np when it opens and keeps one running; a second starts the first time you search and stays until you
switch folders or quit. When you close a folder or leave the app, they should go away within a second or two. If
Task Manager ever shows a `np.exe` still running after Jott has closed, that is a bug worth telling me about — on
Windows it means your vault's index stays locked for the next np.

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
  the console output.
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
