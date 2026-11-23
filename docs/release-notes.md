# Jott 0.0.32 — a ranking model reorders the top of every search

The np inside the installer goes from **0.6.1** to **0.7.0**, and Jott uses what it adds: a ranking model
(`bge-reranker-v2-m3`, a cross-encoder) that reads your query beside each of the top results and puts the one that
actually answers it first. It changes **only the order** — which notes match is still decided by words, meanings or
both, exactly as before. It works in all three search modes, including *Words only*, and needs no embeddings.

## What you'll see

- **Search answers at once, then reorders.** The list appears as before; the line under it says *reranking the top
  results…*, and a few seconds later the rows settle into the model's order and the line says *top 5 reordered by the
  ranking model*. A row the model moved says so — *moved up from #4* — and hovering it shows the model's raw score.
- **Settings → Reranking.** On by default. Switch it off and search is exactly what it was in 0.0.31, and the model is
  never downloaded. **Advanced** sets how many top results it reorders: 5 by default, 1 to 64, saved as
  `[search] rerank_top_n` in note-processor's own `config.toml`.
- **The model downloads when a folder opens** (606 MB, once per folder — note-processor keeps one copy per vault).
  Settings shows the progress. A search never downloads it; until it is there, results arrive unreordered with
  *not reranked — the ranking model is not ready (see Settings)*.
- **Closing Jott mid-download is fine**: the next open starts the download again by itself.

## If the download fails

On networks that inspect encrypted traffic — common at work, measured here behind Zscaler — note-processor cannot
download any model itself and says *could not resolve the configured host*, which is a mislabelled certificate
refusal. Settings then offers the way round it:

1. **Open the download in the browser** — your browser trusts the network's certificate, note-processor does not.
2. **Choose the downloaded file…** — note-processor copies it into the folder's index and refuses it unless it hashes
   to `d32a877b7266f5275172458616824ce6728502416672052c2da795da3389201e`, so a half-finished download is refused, not
   used.

The same network blocks note-processor's download of the embedding model too; that is not new in this release, only
newly noticed.

## What it costs

note-processor loads the model for every reranked search. Measured on the Windows asset: about **2.2 s plus 0.25 s per
result reordered** — roughly 3.4 s at the default of 5. That is why the reorder is a second step you never wait for,
and why the Advanced number says what more results cost. Retrieval alone stays in milliseconds.

## What I measured before shipping it

Both 0.7.0 assets downloaded and checked against the release's own `SHA256SUMS.txt` (`47fda373…` Windows,
`dc052c0c…` Linux). Against the binaries themselves: the handshake still names the same ten methods at protocol `1`;
`search` takes `rerank: true | false` and answers three new keys per hit plus a `rerank` object; a reranked search with
no model on disk is a refusal Jott now answers with retrieval's list instead of an error; 0.6.1 ignores the new key, so
an older np keeps working without the stage. The real-backend gate ran against the Linux asset, including a reranked
search after a hand install and a download interrupted by closing and resumed on the next open.

## What this release deliberately does not claim

- **No measured improvement in ranking quality.** note-processor ships the stage without one, and so does Jott. Try
  your own one-word and question-shaped searches with it on and off — that comparison is the report I want.
- **The floor stays 0.5.1.** Only the ranking step needs 0.7.0; on an older np Settings says so and search works as
  before.

## Numbers

401 Rust tests, `cargo clippy --all-targets -- -D warnings` clean, `svelte-check` with 0 errors and 0 warnings, the
front-end suites green. What to try on a real Windows machine is in `docs/testing-on-windows.md` §2b-zi.

## Upgrading

Install over 0.0.31. Nothing re-indexes and nothing re-embeds; the first open of each folder downloads the ranking
model unless you switch reranking off first.