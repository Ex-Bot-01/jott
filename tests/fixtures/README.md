# `np serve` protocol fixture

`np-serve-transcript.ndjson` is **verbatim stdout** of `np serve` 0.3.1 on this machine — not edited, not
pretty-printed, not hand-written. The reader loop in `NpBridge` and its "state vs failure" branch are
tested against this file so that nobody has to guess what a refusal looks like.

Regenerate with `scripts/capture-fixture.py` (needs `np` on PATH; it creates throwaway vaults under
`/tmp` and leaves nothing in the repo).

## What each part proves

| Lines | Content | Why it matters to us |
|---|---|---|
| 1–2 | `initialize` with `notifications: true` → result carries `"progress": true`, then a terminal `np/progress` line for request 1. | The handshake is where progress is bought. Absent flag ⇒ nothing arrives unsolicited. |
| 3–7 | `scan`: `locking` → `indexing` (opening) → `indexing` with `files_seen`/`notes_seen` (closing bookend, same second) → **response** → `completed`. | Scan is bookended, not live. The terminal line comes **after** the response: read the answer first, use notifications for continuity. |
| 8–9 | `search` result: hit keys, `mode`, `vectors: false`, `reason: null`, and a snippet with `[hash]` marking the match. | No `title`, no line number. `vectors`/`reason` are the provenance we render in the empty state. |
| 10–12 | `embed` **refused by configuration** (`provider = "none"`) — and it is a **result**, ending `completed`, with `outcome.configuration-disabled` carrying a `hint` sentence written for a human. | A degraded answer is not an error. Our consent gate reads this shape, and the hint is good enough to show verbatim. |
| 13–14 | `-32601` unknown method (`links.list`, which does not exist upstream) + `phase: "refused"` with `code`. | Fault *and* notification carry the same code; count lines by id, never blindly. |
| 15–16 | `-32602` bad enum (`mode: "telepathy"`). | Params are validated per key with a sentence naming the allowed values. |
| 17 | `-32700` parse error with **`id: null`**. | A malformed line has no id to echo; our writer must never assume `id` is present in what it reads back. |
| 18 | `-32602` missing required key (`search` without `query`). | Missing and unknown keys are both `-32602`; the message names the key. |
| 19–22 | A **second process**: `scan` of a workspace where another process is mid-embed (it succeeded — see below), then `status.get`. | Readers and even a second writer can get through while an embed computes vectors; `status.get` answers without touching the database. |

## Two things this capture settled by measurement

- A `search` queued **behind** a full-vault `embed` on the same connection waited **102.1 s** and then
  answered — the server is one-at-a-time with no cancellation. That is why the app keeps a separate
  reader connection instead of queueing searches behind embeddings.
- A `scan` from a second process committed in 60 ms **while** that embed was running elsewhere, so np does
  not hold the write lock for the whole embedding run. `locked` is therefore brief and rare, not the norm.

## Reproducing

```sh
np --version                     # must be np 0.3.1; re-capture if the pinned version moves
python3 scripts/capture-fixture.py
```

The script drives two vaults: a two-note one for the clean session, and a 200-note generated one whose
first `embed` runs ~106 s — enough to observe contention. It terminates the embedder with SIGTERM on
purpose; nothing is left behind (a killed process cannot leave a lock, per np's own troubleshooting doc).
