# Jott 0.0.12 — the whole app was inspected; here is what it could not do yet

This release came out of reading every part of Jott — the vault code, the np client, the window, the log, the CI —
and asking what a person would find missing or misleading. It turned out to be mostly about *accuracy*: what the
window says when np refuses something, what the log knows about a run, and which keys actually do what they look
like they should.

Two bugs were found in np itself while checking this, and filed: [note-processor#4](https://github.com/Ex-Bot-01/note-processor/issues/4)
(a workspace is keyed by the exact path string, so `D:\Notes\` and `D:\Notes` can re-index a vault once) and
[#5](https://github.com/Ex-Bot-01/note-processor/issues/5) (a search cannot tell "never indexed" from "no matches",
and reports `hybrid` when only keywords ran). Jott now works around both without pretending the words mean more
than they do.

## Saying what actually happened

- **A refusal is no longer reported as a missing np.** When np answered `-32000 workspace … is not a directory that
  exists`, Jott told you to go install something that was running fine and had just refused you. Refusals now say
  *np said this*, in np's own words; only genuine silence or a dead child says np is gone; `-32601` says the build
  is too old and names the version Jott needs (0.3.1).
- **An empty search says which kind of empty it is.** A folder that has never been indexed gets *"This folder is not
  indexed yet (7 notes are waiting) — use Index now"*, not *"Nothing matched"*, because retyping a search that
  could never answer is nobody's idea of feedback.
- **The results header stopped claiming a mode.** np 0.3.1 answers `hybrid` for searches where only keywords ran, so
  the word on screen promised vectors that did not exist. It shows a count now: counts cannot lie.
- **Search answers in the order you typed.** Overlapping requests used to land whenever np felt like it, and the
  slowest one won — which read as the box ignoring what you had just typed.

## The log knows which run it is

`jott.jsonl` used to hold one line per np request and nothing else, so the two reports that actually arrive had no
answer in it. There are now three shapes of line, each with its own fixed keys:

- **a request** (`method`) — as before, plus timing and np's sentence on a fault;
- **the start of a run** (`jott`) — Jott's version, the exact `np.exe` this run talked to and how it was found, your
  platform, and where Jott keeps its own files. So "it worked yesterday" can be located in the log: each day's run
  names its own build and its own np;
- **file work** (`event`) — `save`, `created`, `write-failed`, `renamed`, `moved`, `deleted` (with where the note went
  and what the system trash said), `mirror-replayed`, `mirror-discarded`, `consent`, `index-now`,
  `watcher-unavailable`, `state-root-fallback`.

Still no note contents anywhere: a file line records *which* file and how many bytes, never what they spell. The
diagnostics drawer shows these as live rows too, and now names Jott's own folder beside np's — saying out loud when
that folder is a temporary fallback, which is the case where unsaved drafts are quietly landing in `%TEMP%`.

## Things that were always meant to work

- **You can make a folder.** `create_folder` and `move_entry` have had commands and tests since the first milestone
  and nothing in the window ever called them. Right-click a row (or empty ground) for *New note here · New folder
  here · Rename · Move to… · Move to trash*, or use the header buttons. Right-clicking `INDEX.md` offers nothing,
  because that file belongs to np.
- **The keyboard means what it says:** `Ctrl+N` new note, `Ctrl+Shift+N` new folder, `Ctrl+R`/`F2` rename, `Delete`
  trash (after asking), `Ctrl+F` search, `Ctrl+S` save now. F2 and Delete stand down while you are typing in a note —
  the difference between a shortcut and a way to lose a file.
- **The window remembers itself:** size and position come back, the title says `Jott — <folder>`, and the recent
  folders the registry has kept all along are now a menu beside *Open…*.
- **np's own files stopped waking the window.** `INDEX.md` changes on every indexing pass; the watcher reported that
  as an outside edit, which re-listed the tree once per pass — the flicker. It is np's file, and Jott treats it as
  one.

## Under the hood, where a stall used to be forever

- **A wedged np can no longer hang the window.** `initialize`, `models.list` and `status.get` wait 30 s, a search
  60 s, and then Jott stops waiting and says so. Indexing and embedding deliberately keep *no* deadline: embedding a
  large vault legitimately runs for minutes, and killing it mid-flight would throw away real work to guard against a
  stall that the quit path already bounds.
- **One bug no longer breaks every later save.** A panicked worker used to poison the journal's lock, and the next
  save would panic on it — a log that can stop your work contradicts the rule the log is built on. Logging, the trace
  ring and the stamp map now carry on, and report what failed where it happened.
- **A committed index brings the retry wait back down.** With embeddings switched off, a two-second lock climbed the
  ladder to 60 s and never came back down, because only a finished embed used to reset it.
- **One folder, one spelling** in everything sent to np (see note-processor#4), so alternating spellings cannot cost
  a full re-add of your vault.

## For whoever tests this

`docs/testing-on-windows.md` has the new section (§2b-ii) for the menu and shortcuts, plus what to watch for with the
window title and recents. §5 now describes all three log shapes so a report can name the right one.
