# Consent you can see through, a download you can watch, and a log you can open

Design for v0.0.14. Short form: three reported problems, one approach each, decided together on 2026-11-14.

## Intent and the why

A tester clicked **Allow download** and got `keyword-only: embeddings.provider is not set` — an accurate sentence from
np that explained nothing and left them with no vectors and no idea why. Separately, keyword search found no note
containing *grandma* when they typed *grand*, and **About → Diagnostics → Open log folder** opened a tiny box of text
instead of the folder.

The person using Jott is not reading np's config or its source. Everything on screen has to be either something they
can act on, or np's own words about why it cannot act — and never a claim the bytes do not support (§4).

## What was found, measured

- **Consent is half-written by design.** Jott writes `[models] auto_fetch` into np's config and nothing else. np 0.4.0
  with only that key answers `embed` with `embedded: 0, model: null, fetched: null` — a *successful* report that
  embedded nothing. With `provider = "local-model"` and no `model` it faults `-32000 … needs embeddings.model to name
  an installed model`. With both set it fetches 309,855,456 bytes and embeds.
- **Nothing in np's surface can set those keys.** `serve` answers eight methods, none of which configures; no np
  command writes its own config; `np setup` — built "for an application that wants vectors to exist" — still exits 1
  on *no provider*. Filed as [note-processor#7](https://github.com/Ex-Bot-01/note-processor/issues/7).
- **Whole-word keyword matching is np's documented rule**, and there is no prefix door: `grandma` ✓, `grand` ✗,
  `grandm` ✗, `kestr` ✗ on a note containing all of them. After embedding the same note, all three fragments return
  the hit with `semantic_used: true` — so the gap is mostly about folders that have no vectors, which is what the
  consent bug caused. Filed as [note-processor#6](https://github.com/Ex-Bot-01/note-processor/issues/6).
- **np streams real download progress** once it is fetching: `phase: "fetching"` with `bytes_done`, `bytes_total`,
  `member`, `members_done`, `members_total`, about one line per second, then `embedding` with `chunks_done` /
  `chunks_total`, then `writing`, then `completed`. Jott's ingest queue already parses these into
  `Working { phase, done, total }`; only the screen ignores them.
- **"Open log folder" was refused, not opened.** The shell grants `opener:default`, which is `allow-open-url` +
  `allow-reveal-item-in-dir` — there is no `allow-open-path`. So `openPath(folder)` rejected, our `.catch()` fired,
  and the tiny box was the clipboard-failure textarea holding the *copied diagnostics text*.

## Rulings that shape this (theirs, not mine)

- **No Ollama, ever.** Local inference goes through llama.cpp directly — which is what np's `local-model` provider is:
  a statically linked, pinned llama.cpp. Nothing in Jott or np talks to a server.
- **Clear separation: Jott does not do the backend's job.** If something is missing to continue, it is an issue on
  note-processor, not a workaround in the front end. Consequence: Jott will **not** write `embeddings.provider` or
  `embeddings.model`, and no client-side substring search will be bolted on behind np's results.
- **The one existing config write stays**: `[models] auto_fetch = false` while consent is undecided, because a fresh
  np defaults to `true` and was measured fetching 310 MB during an unrelated search with nobody asked. It is a
  guardrail rather than configuration, it is named out loud in the UI, and it goes away when np#7 lands.

## Goals

1. After **Allow download**, if np still cannot embed, say so in a way that explains what Jott recorded, what np is
   waiting for, and how to give it — without Jott editing np's settings.
2. Show a real progress bar while a model downloads, and tell the person when it finishes.
3. Make **Open log folder** open the folder with the log selected.

## Non-goals

- Prefix or fuzzy search in Jott (np#6), and any local re-implementation of np's matching.
- Writing `provider`/`model`, or any new key, into np's config (np#7).
- A second diagnostics surface, a log viewer inside the window, or an in-app TOML editor.
- OS notifications while Jott is the focused window — that would interrupt the person who is already looking.

## Anti-goals

- A bar that moves on its own. If np has not said a byte, nothing advances.
- A card that says "downloading" or "done" when np refused. Verbs follow the answer document.
- A silent second write into somebody's config file, discovered later by them rather than told now by us.

## Approaches

**Getting embeddings switched on.** *(Chosen: report and ask.)*

- **Report and ask (chosen)** — Jott records its half (`auto_fetch`, plus its own consent decision), and when np
  answers that the provider is unset, a card states: what Jott wrote and to which file; np's sentence byte for byte;
  the exact lines np needs, copyable; a link to np#7; and a **Try again** that re-queues the work. Trades: one manual
  step for the user, and it stays honest while np has no method.
- *Write the two keys ourselves* — one-click today, at the cost of duplicating np's config validation outside np,
  where the failure mode is a successful-looking `embed` that embedded nothing. Rejected by the separation ruling.
- *Wait for np#7 and change nothing meanwhile* — purest, but leaves the reported confusion unfixed for a release.

**Telling them it finished.** *(Chosen: banner in the window, plus an OS notification only when unfocused.)*

A banner that stays until their next action is testable and cannot be missed by someone watching the window. Tauri's
notification plugin covers the person who alt-tabbed away during a 300 MB download; if permission is refused or the
webview has no notifier, the banner is still there and nothing errors.

**The progress bar.** One sensible way: render what np sends. Determinate when `bytes_total` is present (percent,
`159 / 310 MiB`, `model part 1 of 2` from `members_done`/`members_total`); an unquantified moving stripe plus
`159 MiB so far` when it is absent, because np omits the key precisely when the size is unknown and a `0%` bar reads
as a stall.

**The log folder.** One sensible way: `revealItemInDir(logPath)`, already granted by `opener:default`. The failure
fallback becomes a line naming the path with **Copy path**, not a box of unrelated text.

## Decided about the how

- **Additive wire change.** `working` gains optional keys for bytes and member position rather than becoming a new
  event, so an older front end keeps working and the contract test still pins every key. `total: null` continues to
  mean "np did not say".
- **The card's trigger is derived in Rust**, as a pure function over np's config and the last refusal — so "provider
  unset", "provider set but no model named" and "provider set to something we do not recognise" are three tested
  states, not markup conditions. In the third case Jott advises *nothing*: it quotes np and stays out of a deliberate
  choice.
- **Completion is an event, not a guess**: the ingest worker's transition from fetching to embedding (`outcome:
  current` after a fetch) is what shows "model ready", and the wording says which kinds of search are on now.
- **Journal**: what Jott wrote to np's config is recorded as a file-work line (keys and destination, never values or
  note contents), so the drawer and the log agree about whether Jott touched anything.
- **`revealItemInDir` gets the file path**, which `RuntimeInfo.logPath` already carries; the button is disabled with a
  tooltip when there is no log file at all.

## Left to the builder

Exact wording of the card and banner (tested as strings, kept short); whether the bar lives in the status line or the
drawer's header (status line: it is where ingest already speaks); how the unfocused check is done (the window API's
focus state, no new plugin beyond notifications).

## Verification

Rust tests for the derived card states and the additive event keys; a pure front-end module for percent/bytes/member
text including the unknown-size case; fake-np driven progress in the smoke suite (bar at a named percentage, then the
banner); a test that reveal is called with the log file path, not its folder; the existing np-absent discipline kept
honest by running the suite with np off `PATH`, and CI's passing-count floor raised again if the count moves.
