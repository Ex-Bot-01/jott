# Five window asks — closing a note, reading np's catalogue, the caret in dark mode, moving into a folder that is not there yet, and a settings panel

Requested by the tester after using 0.0.15 on Windows. Five things, to be done **one after another**, each
shippable on its own. The shape of the settings panel was proposed and accepted before this document; nothing
here reopens it.

## Why

All five are the same complaint wearing five faces: the window makes someone do work that belongs to the
window. Finding out how to stop looking at a note. Reading a file the app already shows them, by name, in the
tree. Seeing where their typing will land. Moving a note into a folder that does not exist yet. Changing an
answer they gave a month ago about embeddings without opening a TOML file.

## Goals

1. **Close the open note** — one click, and one key.
2. **Read `INDEX.md`** — visible in the tree as it is now, openable, and impossible to damage.
3. **See the caret** in dark mode, without depending on which stylesheet CodeMirror injected last.
4. **Move a note into a folder that does not exist yet** — name it, create it, move, one dialog.
5. **A settings panel** with search matching (words / words + meanings / meanings only), word-beginning
   matching, and embeddings allowed or revoked through note-processor's own methods.

## Non-goals

- Tabs, multiple open notes, split views. One note is open or none is; that model is not changing.
- Editing `INDEX.md` in any form, or an "unlock" affordance for it.
- A general preferences framework (themes, fonts, keybindings). The panel holds the three controls this
  release has real behaviour behind, and nothing else.
- Any Jott-side search ranking. Matching is note-processor's job; Jott asks and reports (§ clear separation).

## Anti-goals

- A "meanings only" switch that answers *nothing matched* when the truth is *there are no meanings yet*. That
  is the exact failure v0.0.13 was built to remove, and it is reachable today — see measurements.
- A read-only view that still lets Ctrl+S write, or that greys the file out so completely it reads as missing.
- A caret fix that works by being later in the cascade than CodeMirror's runtime styles. That is how 0.0.10's
  caret rule was written, and it is why this one is not.

## Constraints already on the books

- §18: np's own catalogue files (`INDEX.md`, `INDEX-*.md`) are not notes. They stay out of ingest, out of the
  watcher, and unwritable by Jott.
- Jott writes exactly one np setting — `models.auto_fetch` — and an answer has exactly one writer
  (`consent::answer_consent`). A settings panel is a new *caller*, not a new writer.
- Front-end logic that can be tested headlessly lives in pure TS modules with Vitest tests, not in markup.
- No fake facts: a control states what np answered, and absence stays absence (`null`, not `false`).
- The command surface lives in `crates/jott-core/src/ipc.rs`; the shell stays thin.

## What is true today (read from the code, measured against np 0.5.1)

**Closing.** There is no close affordance and no `Escape` handler. `selected` changes only when another tree
row is clicked. So "close" is missing, not hidden.

**`INDEX.md`.** `walk.rs` finds it and marks it generated; the tree greys it with an ownership sentence;
`files.rs::read_note` refuses it outright:

```rust
if !rel.contains('/') && !rel.contains('\\') && crate::vault::walk::is_generated_index(name) {
    return Err(FileError::NpGenerated { rel: rel.to_string() });
}
```

Clicking it therefore does nothing visible, which reads as a broken file. The refusal is right for *writing*;
it was never asked to cover reading.

**The caret.** Nothing in the app ever tells CodeMirror it is dark — `grep` for `darkTheme` and `matchMedia`
across `src/` returns nothing. So CM renders its **light** facet while our CSS paints dark paper, and CM's own
rules are light ones:

```
".cm-cursor, .cm-dropCursor": { borderLeft: "1.2px solid black", … }
"&dark .cm-cursor":           { borderLeftColor: "#ddd" }        // never applies here
"&light .cm-content":         { caretColor: "black" }
selection background          // light blue, on near-black paper
```

Our override (`.cm.svelte-x .cm-editor.cm-focused .cm-cursor { border-left: 2px solid var(--ink) }`) does win
on specificity today — but it wins a race rather than removing it, and it says nothing about the selection
colours or the blink. The blink is worth naming on its own:

```
"&.cm-focused > .cm-scroller > .cm-cursorLayer": { animation: "steps(1) cm-blink 1.2s infinite" }
@keyframes cm-blink: 50% { opacity: 0 }
```

The caret is **fully invisible half the time**, on a hard step, in both schemes. "Hard to see where the cursor
is" is a fair description of that.

**Moving.** The dialog renders `moveTargets(rows, rel)` — existing folders only. A vault with no folders gets
an empty list and nothing else to press. Core already has `create_folder` and `move_entry`; there is no new
backend needed.

**Search mode.** `pool.rs` always sends `Mode::Hybrid`. Measured on 0.5.1, one folder of two notes, scanned:

| request | hits | what np said alongside |
|---|---|---|
| `mode:"keyword"` | correct notes | `indexed:true`, `keyword_used:true`, `semantic_used:false`, no reason, no note |
| `mode:"hybrid"`, provider off | correct notes | `reason:"provider-disabled"` + np's sentence |
| `mode:"hybrid"`, provider set, `auto_fetch=false` | correct notes | `reason:"fetch-disabled"` + np's sentence |
| `mode:"semantic"`, no vectors | **empty** | `indexed`, `keyword_used`, `semantic_used`, `reason`, `note` all **absent** |

So words-only is expressible and honest today; meanings-only is expressible but **silent about why it found
nothing**. That silence is note-processor's to fix and will be filed there. Until it is fixed, Jott does not
offer a control that would lie: *meanings only* is unavailable, with its reason on screen, until np says this
folder has vectors.

## Approaches considered

**A. Five small changes in the existing shapes** — chosen. Close = state + button + `Escape`. Read-only view =
core answers catalogue reads with a flag and the editor gets a `readonly` prop. Caret = CodeMirror's own theme
mechanism plus its dark facet. Move = one extra row in the dialog that creates then moves. Settings = a panel
holding three controls, wired to commands that mostly exist. Trades: the settings panel needs one new command
(search mode) and one DTO; nothing else grows.

**B. A general "preferences" object written to Jott's own config file.** Rejected: only three of its keys would
have behaviour behind them today, and an inert setting is a promise the window cannot keep — §4's invented value
again, in a settings dialog.

**C. Widen the existing consent card into the settings surface.** Rejected: the card answers one question at one
moment ("may I download?"). A place to revisit decisions needs to be reachable when nothing is wrong, and it
would have to stop being a card.

**D. Fix the caret with `!important` CSS.** Rejected on the anti-goal above: it keeps the cascade race and fixes
only the caret.

## The how, as decided

**1. Caret and dark facet.** New pure module `src/lib/darkMode.ts` — `schemeOf(prefersDark)` and
`watchScheme(onChange)`, which subscribes to `matchMedia('(prefers-color-scheme: dark)')` and returns an
unsubscribe; no `window` at import time, absent `matchMedia` means light. Editor gets a compartment holding
`jottTheme(scheme)`: an `EditorView.theme(…, { dark })` that sets the caret (`2px solid var(--ink)`), the paper
and ink from the palette, and a scheme-appropriate selection background, **plus**
`EditorView.darkTheme.of(dark)` so CodeMirror's own `&dark` variants stop contradicting us. The cascade race is
gone because the colours now arrive through CM's style module, in the right facet.

Blink: Jott overrides the cursor layer's keyframes so the caret dims instead of switching off, and stands
completely still under `prefers-reduced-motion`. A forced-colors (Windows high contrast) rule paints the caret
`CanvasText`, because forced colours ignore custom properties. Observable in jsdom: with a dark media stub the
editor DOM carries `cm-dark`.

**2. Close the note.** `selected = null` plus clearing `note`/dirty state, through one function so the button and
the key cannot drift. A `close` button beside rename/delete, disabled with nothing open, and `Escape` when focus
is not in a text target (the standing rule: shortcuts stand down inside inputs, textareas, contenteditable and
`[role="textbox"]`) — so Escape closes the note, and does nothing while someone is mid-sentence in the search box
or the editor. Closing never writes; an unsaved buffer keeps its §18.13 mirror and says so when the note is
reopened.

**3. `INDEX.md` read-only.** Core stops refusing catalogue *reads*: `read_note` returns the content with
`readonly: true` on the DTO (a new key, pinned in `ipc_surface.rs` and cross-checked against `src/lib/ipc.ts`),
and `save_note` keeps refusing with the same `NpGenerated` error it uses today — the refusal moves from "you
cannot see this" to "you cannot change this". The editor takes a `readonly` prop: CodeMirror editable off, typing
refused, Ctrl+S answers "read-only" instead of writing, and the header says what it is — *note-processor's index;
Jott will not edit it*. Tree keeps its grey glyph, and clicking opens it like any other row.

**4. Move into a folder that is not there yet.** The move dialog gains a second field: a name plus
*create and move*. Empty or duplicate-of-existing names do nothing new — an existing folder is chosen from the
list instead; `create_folder` then `move_entry`, both already in core, and one refresh. Failure keeps the dialog
open with np's/Jott's sentence, because a note left where it was is better than a note whose whereabouts are a
guess.

**5. Settings panel.** A modal opened from the menu (and `Ctrl+,`), holding:

- **How search matches**: words only / words + meanings / meanings only. Sent to np as `mode` — new parameter on
  `search_vault`, threaded through `ipc.rs` → `NpPool::search` → `methods::search`, replacing the hard-coded
  `Mode::Hybrid`. Remembered per app in `localStorage`, default *words + meanings*, which is what happens today.
  *Meanings only* is disabled while np has not said this folder has vectors, and the control carries the reason
  on screen rather than being quietly absent. When it is chosen and a search comes back empty with nothing said
  about why, the results header says plainly that meanings-only found nothing and words are still available —
  never "nothing matched".
- **Match word beginnings**: the existing toggle's value, moved into the panel and kept in sync with the toolbar
  checkbox (one state, two places to see it).
- **Embeddings**: allowed or not, through `embeddings.enable` / `embeddings.disable` — the same code path as the
  consent card, so no second writer of anything. It shows what np answered (keys moved, file, size) and Jott's
  own key outcome, in the words `consentHelp.ts` already owns.

Settings are remembered per app, not per folder, except *meanings only*, which is a request rather than a fact:
choosing it in a folder without vectors is refused on the spot with its reason, and does not become someone's
saved preference.

## Left to the builder

Exact wording of sentences (tested, as everywhere else); where in the menu each entry sits; how the panel's three
sections are spaced; whether the close button reads `close` or `×` — it must be a word, and it must have a title.
Anything that would change what np is asked stays pinned by a wire test, as usual.
