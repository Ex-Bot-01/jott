# Jott 0.0.30 — a search result now says which half found it

A tester searched `Familie`, got a three-line note on top with `meaning 0.70`, and asked how that can beat the family
birthday calendar. They were right to ask, and the answer is not in the ranking: **the number they compared was never
a ranking.** This release says what each result row actually is, and puts the measurement behind the figure where it
can be read before, not after, it misleads somebody.

## What you'll see

- **Rows reached without your words are marked.** A result that came out of the vector list now reads
  **"by meaning only"** beside its figures. Rows matched at a word beginning keep their existing *"from the start of a
  word"*, and rows where the typed words were simply there stay unmarked. Nothing new appears unless note-processor
  said something about that row.
- **Hovering a meaning figure tells you what those figures look like.** Not a rule about how to search — one measured
  sentence, with its provenance: on note-processor 0.6.0 with `embeddinggemma-2`, four unrelated notes all sat between
  0.63 and 0.78 similar to any query in the same language, and a search for a single word left about 0.03 between
  them. Differences in the hundredths are not a ranking to act on.

## Why this needed fixing

note-processor answers three things per hit, and Jott drew only two of them. `matched_exactly` is `true` when your word
was in the bytes, `false` when it widened to a word beginning — which the row has always said — and `null` when **the
row was never word-tested at all**. That third case drew nothing, so a meaning-only hit looked exactly like an exact-word
match, and its `meaning 0.72` was often the only number on it. Two rows side by side, one found by a word you typed and
one found because German prose sits roughly the same distance from other German prose: indistinguishable in the window.

What the measurements look like, from a folder of four notes (a birthday table, a coconut-macaron recipe, a shopping
list, and three lines of chatter), embedded with the pinned model:

| query | top hit | cosine spread across all four |
|---|---|---|
| `Familie` | **the three-line note** | 0.028 |
| `Geburtstag` | the three-line note | 0.065 |
| `Geburtstag Familie` | the birthday calendar | 0.096 |
| `Wer hat in meiner Familie Geburtstag?` | the birthday calendar | 0.141 |

Fusion is not the problem, and neither is Jott's ordering — it draws np's order unchanged. When the calendar contains
the literal word, the fused rank floats it to the top on its own (0.032522 against the chatter note's 0.016393) even
though that note has the *higher* cosine, 0.7195 to 0.7164. What failed was reading: a figure that looks like a verdict
and a row that looks like a word match.

## What this release deliberately does not do

- **No threshold invented here.** Jott does not hide the meaning figure for one-word queries, dim near-ties, or compute
  a spread — the window sees only the rows it asked for, never the distribution behind them. `note-processor#19` asks
  np for that as a fact it is allowed to quote; if it lands, this becomes a sentence rather than a tooltip about one
  model's behaviour.
- **No claim from silence.** "By meaning only" appears only when np sent *both* a missing word test and a cosine. A row
  where neither half spoke stays unmarked: two absences are not a finding, and Jott would rather say nothing than
  conclude something about a measurement nobody reported.

## Numbers

503 front-end tests, `svelte-check` clean. Two existing assertions changed on purpose — one that demanded the row stay
silent when np never word-tested it, one that modelled an old backend without the flag — and both carry the reasoning
where they now sit. No Rust change: `matched_exactly` has reached the window since 0.5.1 was the floor; this release
just reads it.

## Upgrading

Nothing to do, nothing to re-index. Same note-processor 0.6.0 inside as 0.0.29.
