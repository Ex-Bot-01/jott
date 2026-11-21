# Jott 0.0.31 — the backend moves to note-processor 0.6.1

The np inside the installer goes from **0.6.0** to **0.6.1**. Upstream spent that release moving code around: `crates/np-cli/src/main.rs` went from 3,234 lines to 328 with one module per command, `np-core`'s `db.rs` split into nine modules along the seams its own schema already had, and 1,133 comment lines that cited design documents, round ids and issue numbers were deleted. Their release page promises that nothing a client can see changed.

That promise is the kind you check rather than accept, because a Jott release that ships a backend nobody verified is how somebody's note index becomes the test case. So: **the success criterion for this release is that nothing happens on your machine.** If something does happen — a folder re-indexing itself, a search ranking differently, a number moving — that is a bug report, not background noise.

## What you'll see

- **About says `note-processor (np) 0.6.1`.** That line is the only visible change.
- **Your folder does not re-index.** A vault indexed under 0.0.29 or 0.0.30 opens with its green dots intact: no scan pass, no re-embed, no `INDEX.md` rewrite.
- **Search gives back the same numbers, in the same order, with the same marks** — including last release's *by meaning only*.
- **The download strip is unchanged**, because it had to keep working: a second child watching a fetch still sees np's `fetching` rows with `bytes_done` climbing toward `bytes_total 309855456`.

## What I measured before shipping it

Against the published assets — both downloaded, both checked with `sha256sum -c` against the release's own `SHA256SUMS.txt`, the Linux binary driven over fixture vaults next to 0.6.0's own binary, one scratch `HOME` per side:

| surface | 0.6.0 → 0.6.1 |
|---|---|
| handshake | same ten methods, protocol `1`, same `features` — only the version string differs |
| `scan`, `search`, `links.list`, `models.list`, `status.get`, `shutdown` | transcripts diff clean apart from the version |
| `INDEX.md` | identical bytes: `sha256 e11a2c01b67dd647…`, percent-encoding of `deep%weird [x].md` included |
| prefix widening | still `matched_exactly: false` on widened hits, `true` on verbatim ones — the two marks a result row shows are keyed to those values |
| an index **and embeddings** built by 0.6.0 | opens with every note `unchanged`; no re-scan, no re-embed |
| the same queries over that existing index | identical cosines to the last digit: `0.7195178188075159`, `0.7139600217985886` |
| a fresh fetch + embed on 0.6.1 | artifact digest-verified at 309,855,456 bytes, six chunks embedded, same ranking |
| the installer path | `scripts/fetch-np.sh` downloads the pinned asset, `sha256sum -c` passes, and the bundled binary answers `np 0.6.1` |

The last row matters more than it looks: the pin in `packaging/np-pin.json` is by hash (`f3fc23f7…` Linux, `cc54b917…` Windows), so a half-downloaded or swapped asset fails the build instead of becoming somebody's index.

## What this release deliberately does not do

- **The floor stays 0.5.1.** Jott ships at or above the floor, never below it, and refusing a person's working older np is not an upgrade for them. Nothing in Jott needs 0.6.x behaviour — that is what the diff-clean transcripts above are evidence *of*.
- **No client-side retry for the first-index race.** `note-processor#18` (two children creating a brand-new index can answer `sqlite: table files already exists`) is still open upstream. I re-ran it for this release: two children over ten brand-new vaults, no refusal on either build — which is "did not see it today", not "fixed", since inside Jott's real flow it came up about once in eight. `jott#23` keeps the client-side options; none of them go in as a side effect of a version bump.
- **No new claim about search quality.** `note-processor#19` — one-word queries leaving a whole folder inside ~0.03 of cosine — is still open, and the meaning tooltip still quotes a measurement rather than inventing a threshold. One thing did get corrected while re-measuring: the tooltip's quoted pair now reads **0.7195 against 0.7140**, both from a single index and identical on both builds. It previously quoted `0.7101` for the calendar, taken from a differently built index of a near-identical folder. The four-thousandths drift between two indexes of nearly the same notes is not a contradiction of the sentence — it is another instance of it, and it is written down where the number lives.

## Numbers

380 Rust tests (26 integration binaries plus the library's own), `cargo clippy --all-targets -- -D warnings` clean, 503 front-end tests, `svelte-check` with 0 errors and 0 warnings. The real-backend gate — open a folder, save a note through Jott's own write path, index it, find it again, close np politely — ran against the exact binary this installer will carry:

```text
JOTT_REAL_NP=apps/shell/np/np cargo test -p jott-core --test np_real_session
test a_real_np_indexes_a_note_jott_just_wrote_and_finds_it_again ... ok
test a_vault_full_of_attachments_still_indexes_and_searches ... ok
```

## Upgrading

Nothing to do, nothing to re-index, nothing to re-embed. The floor is unchanged, so an ambient np of 0.5.1 or newer still works if you keep one on `PATH` — Jott will simply use the one inside the app unless you point `$NP_PATH` somewhere else.
