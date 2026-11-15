# Jott 0.0.16 — five things the window could not do before

Every change here came from someone using it: a note that could not be closed, an index that would not open, a caret
that vanished in dark mode, a Move dialog with nothing in it, and a set of settings you had to already know about. The
backend inside is still **note-processor 0.5.1** on protocol 1 — this release is the window, with one exception worth
reading about below.

## What you'll notice

- **A note can be closed.** There is a **close** button beside the note's name and **Escape** does the same, but only
  when Escape has nothing above it to dismiss: one press, one thing, topmost first — row menu, settings panel,
  diagnostics drawer, note. Escape typed inside a note or the search box never closes that note, because in a text
  field the key already has a job. Closing writes nothing; an unsaved buffer keeps its marker if you come back.
- **`INDEX.md` opens and says it is read-only.** It used to refuse to open, which looked like a broken file. Now it
  reads normally — still greyed in the tree, with a banner saying it is note-processor's own index — and the refusal
  happens where it belongs, on writes: **Ctrl+S** sends no save and says whose file it is in one line, and core still
  refuses the write if anything else asks. Shard catalogues (`INDEX-2.md` and friends) behave the same; your own notes
  are untouched.
- **The caret should stop disappearing in dark mode.** The real fault was that CodeMirror was never told the window was
  dark, so it drew its default one-pixel grey caret on a near-black page — invisible rather than faint. It now takes the
  scheme from the same source as the palette, draws two pixels of ink, and dims instead of switching off while it
  blinks, because a caret that is *off* is gone for as long as it is off. Windows' *show text cursor* setting still makes
  it thicker and *high contrast* mode takes the system caret colour. If you still cannot see it, tell us which of the
  three it is — absent, thin and grey, or there but vanishing while you type — because those are three bugs and a log
  file cannot tell them apart.
- **"Move to…" can move a note into a folder that does not exist yet.** The dialog listed every folder in the vault,
  which in a vault with none was a single entry that read as an empty field. It has a second half now: type a name
  under *…or a new folder* and press **create and move**. Nested names work in one go (`ideas/2026` makes both levels,
  either slash direction) because `create_folder` already builds parents. If the folder cannot be made, no move is sent:
  the dialog stays open with your name still in it and says what went wrong inside itself, rather than closing on a
  "done" that did not happen.
- **Settings is a panel: Ctrl+comma, or the header button.** Three controls — **how search matches** (*words only*,
  *both*, *meanings only*), **match word beginnings** (the same remembered choice as the checkbox beside the search
  box; one setting, so the two cannot disagree), and **embeddings** (*Allow the download* / *Keep it keyword-only*, the
  same two answers as the consent card, through the same door: np writes its settings, Jott writes `models.auto_fetch`,
  and the panel quotes np's answer). The engine choice is remembered per app and re-runs whatever is in the search box
  immediately.

## The one control that cannot be honest yet

*Meanings only* is new, and it is the one place where Jott has to say "the backend will not tell me". Ask note-processor
for meanings only in a folder with no vectors and it answers `"result": null` — no hits, no `indexed`, no `reason`,
nothing on stderr. That is
[note-processor#11](https://github.com/Ex-Bot-01/note-processor/issues/11), filed while building this release, and until
it is fixed:

- The panel warns under that choice when your last search showed the folder has no vectors: *this folder has no vectors
  yet, so meanings-only search will find nothing here*. Before you have searched it says nothing, because Jott does not
  know, and "no vectors" inferred from silence would be an invention.
- An empty meanings-only answer never says *"nothing matched"*. It says either that there are no vectors to compare
  against, or — when np said nothing at all — that np returned nothing and gave no reason.
- A mute answer arrives as a refusal naming the issue, so you can read why instead of wondering whether Jott broke.

## Measured, on np 0.5.1

- Same folder, same query, one second apart: `mode:"hybrid"` → hits plus `indexed:true`, `keyword_used:true`,
  `semantic_used:false`, `vectors:false`, `reason:"fetch-disabled"` and a sentence explaining it; `mode:"keyword"` →
  the same keys with `reason:null`; **`mode:"semantic"` → `"result": null`**. Nothing on stderr, and no fault: np is
  answering, with nothing.
- The three modes are otherwise real: `keyword` and `hybrid` both return hits in a folder whose model was never fetched,
  and what crossed the wire was checked by reading what the stand-in recorded being asked, per label — not by trusting
  the call site.
- Embedding numbers are unchanged from 0.0.15: the built-in default is 309,855,456 bytes (about 296 MiB), and Jott still
  writes exactly one key, `models.auto_fetch`, in `%APPDATA%\note-processor\config.toml`.

## Not user-visible, but worth knowing

- **The search mode is a parameter that used to be a constant.** `NpPool::search` asked for `hybrid` regardless of what
  the window wanted, so the control would have been missing while every other test still passed. An unknown label is now
  refused by core with a sentence naming the labels it accepts (`ErrorKind::BadMode`, pinned in Rust and against
  `src/lib/ipc.ts`) instead of quietly becoming `hybrid`: guessing is how a search ends up answering a question nobody
  asked, which is the failure this project keeps having to design against.
- **A mute answer is a refusal, not an absence.** np's `result: null` used to surface as "np's search result is not
  shaped like one" (blaming np for doing the thing it does) and its code mapped to *could not talk to np* — a
  contradiction about a program that had just replied. It now faults with `-32000` and a sentence naming
  note-processor#11, which the surface reads as a refusal and quotes.
- **Read-only is a fact on the wire, not a guess in markup.** `read_note` answers `readonly: true` for the root-level
  catalogues; the editor mounts non-editable, Ctrl+S is intercepted with a sentence, and the write guard is unchanged —
  so the file is refused by core even if the window were tricked. The key is pinned on both sides.
- **CodeMirror learns the colour scheme when the palette changes**, through its own `darkTheme` extension rather than a
  stylesheet guess, and stamps a stable class on its element so a test can see which facet it drew. Forced-colors mode
  and *reduce motion* are handled explicitly.
- **The Escape order lives in one tested function** (`escapeAction`), not in markup conditions: menu → settings →
  drawer → note, one per press, with the typing-target rule standing down only the last step.
- **One settings key per choice, written from either place.** `jott.search.mode` and `jott.search.prefix`; the prefix
  key is unchanged from 0.0.15 so upgrading keeps somebody's toggle instead of silently turning it off. Anything in
  those keys that this build cannot spell reads as the default — including `SEMANTIC`, which *means* something to a
  person and would switch them to the mode that cannot explain an empty result.
- **The stand-in np can now answer literal null** (`method in replies`, not `replies.get(method) is not None`). Being
  unable to be wrong in this specific way was the difference between testing the reading and asserting a hope.
- **Harness honesty, twice over.** The smoke suite clears `localStorage` between tests: a remembered mode leaking from
  one test into the next made a second window ask for `keyword` and looked exactly like the feature working. And the
  move recorder was pushing to the wrong object, which `call()` swallowed — so the window appeared to send nothing when
  it sent everything.

Tests: **300** Rust, **278** front-end. The CI job prints whether np was on `PATH` (several tests are about np being
absent and pass for the wrong reason on a machine that has it) and fails if the Rust count drops below its floor.

## What to look at, if you tested 0.0.15

1. Open a note, press **Escape** — it closes. Reopen one, open **Settings** with Ctrl+comma, press **Escape**: the
   panel goes, the note stays. Press again: now the note closes.
2. Click into the search box and press **Escape**. The note behind it should stay.
3. Open `INDEX.md`. It should read, say it is read-only, and refuse **Ctrl+S** in words. Renaming it from the tree menu
   should still be refused — renaming is a write.
4. In a vault with no folders, right-click a note → **Move to…** → type `ideas` → **create and move**. Then try a name
   that cannot work (`..`): the dialog should stay open holding what you typed.
5. Set *words only*, search something whose only match is a longer word, get nothing with no line claiming meanings ran;
   set *both* and watch the row come back. Restart Jott: still on *words only*.
6. In an indexed-but-never-embedded folder, search once, then open **Settings** → *meanings only*. You should get the
   warning under that choice and an empty answer whose sentence blames the missing vectors rather than your notes.

Previous releases are listed at <https://github.com/Ex-Bot-01/jott/releases>. Docs:
[testing on Windows](https://github.com/Ex-Bot-01/jott/blob/main/docs/testing-on-windows.md) ·
[building from source](https://github.com/Ex-Bot-01/jott/blob/main/docs/building-on-windows.md).
