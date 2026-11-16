# Jott 0.0.21 — a deleted note stops answering, the dots get colours, and one word stops lying

Four things from the tester who has been finding all of Jott's real problems. Three were about how the window looks;
the fourth was a bug I had already written up as somebody else's, and measuring it changed my mind. That one goes
first, because it is the interesting one.

## The deleted note was ours

The report: *"when I index a file I can see it. But when I delete it and search again the deleted file is in my list
of found files."* Clicking it gave Jott's honest "…is not there" — a bad second answer to give after the first one
looked fine.

I started writing a feature request for note-processor: *search should skip deleted files, and offer a purge option*.
The source seemed to say exactly that — `scan.rs:161` reads *"Paths that are gone; their rows are marked stale, never
deleted (spec §2)"*, and a hit carries no field saying "this path is gone".

Then I ran it. With the np your installer ships: delete `ghost.md`, run `scan`, and it answers `removed: 1`. Search
again and it returns **`hits: []`**. np does retire the note — as soon as a walk of the folder asks it to.

Nothing in Jott ever asked. `delete_entry` moved the file to the trash, wrote its line in the log, and stopped there.
The one scan call in the whole crate sat behind `if owed.is_empty() { return }`, and a deletion is exactly the case
that owes no bytes: nothing changed, nothing needs embedding, so the batch returned early every single time. A note
deleted inside Jott stayed searchable until some unrelated save happened to walk the folder — which on a folder you
only read from is never.

So this release fixes it here, and files nothing upstream:

- **A delete, rename or move tells np to walk the folder again.** In the background: the reply you get about the file
  never waits on np, and it works with np not running at all.
- **Five deletes in a row cost one walk.** note-processor takes a workspace, not a path, so eight rescans would be the
  same directory read eight times — and each one rewrites its own catalogue files.
- **A file deleted outside Jott does the same.** The watcher only reports that a path *changed*, so "is it still
  there?" gets asked of the disk before anything is concluded about it.
- **If np is busy indexing for another program, the ask is kept and tried again** rather than dropped — otherwise the
  ghost survives until your next save.

Your point about moves was the one that mattered, and it is respected by construction: Jott asks for no deletion of any
vector. A moved file is reported by np as `moved`, and its row is relinked. Nothing here can make you re-download a
model because you tidied a folder.

If you still want an explicit *purge* switch — actually delete the rows for paths that are gone, rather than marking
them stale — that is a real feature request and I will file it with the move caveat spelled out. It just was not what
this symptom needed.

## The dots got colours, and a key to explain them

The markers beside your files used to be one grey glyph each: `·` embedded, `●` indexed, `○` not indexed, `!` refused.
They now have colour — green for embedded, half-filled teal for indexed-and-awaiting-embedding, hollow grey for not
indexed — and a key sits under the tree saying all four in small words.

Two decisions worth stating:

**A refusal keeps its `!`.** Amber is a colour, and colour is the first thing to disappear: colour-blindness, Windows'
high-contrast themes, a screenshot pasted into a bug report. The shape stays for the one state where being wrong costs
you a note.

**The key's words are the tooltip's words.** Literally — the legend is built from the same function that writes each
row's tooltip, so the key beside your tree and the sentence you get on hover cannot drift into two descriptions of one
dot. There are also two new tests that read the source of the marker logic itself: a colour with no state behind it, or
a state with no colour, now fails in CI rather than turning up in your screenshot. That is the third time this project
has shipped styling that could never apply — once a greyed-row rule that had been deleted, once a CSS variable
CodeMirror does not have — and I have stopped trusting myself to notice it by eye.

## "meanings on" was not stuck. It answered a different question

*"I change the bullet point to Words only, I close it — in the down left corner the 'meanings on' is not updating."*

It had updated correctly, because it never meant what those words suggest. That dot measures three things: whether
note-processor has an embedding engine named, whether the model's weights are on your disk, and how many notes in this
folder carry vectors. *Words only* is a different fact — what your next search will ask for. Both were true at once,
and the words made it sound like one claim that had failed to take effect.

So:

- The dot now says what it measured: **"notes embedded"**, instead of "meanings on".
- The corner beside it spells out your matching choice, in the same word Settings uses: pick *Words only*, close the
  panel, and the bottom-left reads **"Words only · ● notes embedded"**.

The name of each mode is now written down once in the code, with a test forcing the Settings sentences to open with it.
Two places describing one choice in two vocabularies is exactly how this got reported as a broken toggle.

## TF-IDF: you already have it, and bm25 stays off your screen

You asked whether TF-IDF could be used for long documents. note-processor already ranks keyword hits with SQLite's
**bm25** — the TF-IDF family plus length normalisation and term-frequency saturation, which is precisely the fix for
the problem you remember: raw term frequency keeps rewarding a forty-page note that mentions a word sixty times, bm25
flattens that. Nothing to add.

What was missing was visibility, and here I have deliberately done *less*: bm25 arrives with every hit and now appears
in the tooltip beside the score, not as a second number on every result row. It is a real measurement and a bad thing
to compare by eye — lower (more negative) is worse, and its scale depends on how many of your notes mention the word.
A second figure on each row would invite the same "is 87% good?" reading that the tooltip exists to prevent. Where note-processor sent nothing for a row — no vectors ran, so no cosine — nothing is said about it.

## Checked before this tag

Rust **320** tests passing with np off PATH (floor raised to 315), front end **390** passing with exit code zero,
`svelte-check` clean, clippy `-D warnings` clean, and the Windows target compiling core plus shell — that last gate
proven to have teeth by planting a deliberate compile error in `jott-core/src/ipc.rs` and watching that exact line
fail the build.

## What I still could not see

The colours and the key were judged on Linux, in light and dark, against the real components — but **not** in
WebView2. So the two open questions for you are whether those four dots stay distinguishable at your Windows font
scaling (125%, 150%), and whether anything in the new bottom-left corner crowds when your status text is long. Both are
one-line fixes if they are wrong; neither is something I can find from here.

§2b-y of the tester guide lists what to try.
