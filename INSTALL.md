# INSTALL — getting `np` (note-processor) onto a dev machine

This is the environment setup for the note-processor **frontend**. The frontend does not reimplement
anything: it spawns the `np` binary and talks to it. So step zero for any development work is a working
`np` on PATH.

Everything below was actually run, on this machine, on 2026-11-10, and the output blocks are verbatim.

- Host: Linux x86_64 (Ubuntu), `git 2.53.0`, `node v22.22.1`, `Python 3.14.4`, Rust toolchain present
- Backend under test: **np 0.3.1** (latest published release, tag `v0.3.1`, published 2026-11-09)

---

## 1. Clone the backend repo

```sh
cd ~
git clone https://github.com/Ex-Bot-01/note-processor.git
cd note-processor
```

Already cloned here; refreshed instead:

```sh
git fetch --tags origin && git status -sb   # -> ## main...origin/main  (up to date)
```

State found: `main` = `907122d`, which is **18 commits ahead of the `v0.3.1` release tag**. That gap
matters, see [Version skew](#6-version-skew-matters).

## 2. Install the binary (recommended path)

The published archives contain `np`, `LICENSE`, `THIRD_PARTY_NOTICES.md`. The inference runtime is
linked in; the model weights are **not** — those download later, on first `np setup` / `np embed`
(~309 MB, announced before byte one).

```sh
mkdir -p ~/tmp/np-install && cd ~/tmp/np-install
gh release download v0.3.1 -R Ex-Bot-01/note-processor \
  -p 'np-0.3.1-x86_64-unknown-linux-gnu.tar.gz' -p 'SHA256SUMS.txt'
```

Assets on the latest release: `np-0.3.1-x86_64-unknown-linux-gnu.tar.gz`,
`np-0.3.1-x86_64-pc-windows-gnu-cross.zip`, `SHA256SUMS.txt`, `run-stage5-7.ps1`.

Verify before executing anything:

```sh
grep 'linux-gnu' SHA256SUMS.txt | sha256sum -c -
# np-0.3.1-x86_64-unknown-linux-gnu.tar.gz: OK
tar tzf np-0.3.1-x86_64-unknown-linux-gnu.tar.gz   # np, THIRD_PARTY_NOTICES.md, LICENSE
tar xzf np-0.3.1-x86_64-unknown-linux-gnu.tar.gz
```

Install somewhere on PATH (`~/.local/bin` is already on PATH here):

```sh
install -m 0755 ./np ~/.local/bin/np
np --version          # -> np 0.3.1
```

### Windows (PowerShell) — same procedure

```powershell
$dir = "$env:USERPROFILE\Downloads\np-install"
New-Item -ItemType Directory -Force -Path $dir | Out-Null
Set-Location $dir
gh release download v0.3.1 -R Ex-Bot-01/note-processor `
  -p 'np-0.3.1-x86_64-pc-windows-gnu-cross.zip' -p 'SHA256SUMS.txt'

(Get-FileHash .\np-0.3.1-x86_64-pc-windows-gnu-cross.zip).Hash.ToLower()
# must equal the zip's line in SHA256SUMS.txt:
# acb7b200de5707f697134661f675fd86fec8d57d22367a0a4e817e0f98b04a00

Expand-Archive .\np-0.3.1-x86_64-pc-windows-gnu-cross.zip -DestinationPath $dir
New-Item -ItemType Directory -Force -Path "$env:LOCALAPPDATA\Programs\np" | Out-Null
Move-Item .\np.exe "$env:LOCALAPPDATA\Programs\np\np.exe" -Force
# add that folder to PATH (User scope), then in a NEW shell:
np --version        # -> np 0.3.1
```

Notes for Windows, straight from the backend README:
the asset is a **mingw-w64 cross-build, unsigned** (SmartScreen will complain on first run); it is not
an MSVC build; and the write-lock behaviour under a second concurrent writer has never been measured
on Windows. **No macOS asset exists at all** — macOS needs the source build below.

## 3. Alternative: build from source

Needs only a stable Rust toolchain for the default feature set (no C++, no cmake):

```sh
cargo install --git https://github.com/Ex-Bot-01/note-processor --tag v0.3.1 -p np-cli --locked
# or, from a clone of main:
cd ~/note-processor && cargo install --path . -p np-cli --locked
```

Not run here — the release binary satisfied everything in step 4. Use this path on macOS, or when you
need a feature that landed after `v0.3.1`.

## 4. Verify it actually works (not just `--version`)

### 4a. CLI smoke test

```sh
rm -rf ~/tmp/np-smoke-vault && mkdir -p ~/tmp/np-smoke-vault/projects
printf '# Welcome\n\nLinks to [[projects/timeline]] and a [[missing-note]].\n\n- [ ] buy milk\n' > ~/tmp/np-smoke-vault/welcome.md
printf '# Timeline\n\nMoved files are detected by content hash plus path.\n' > ~/tmp/np-smoke-vault/projects/timeline.md
printf '# Ideas\n\nSome prose about embeddings and hybrid search.\n' > ~/tmp/np-smoke-vault/ideas.md

np scan ~/tmp/np-smoke-vault
```

Observed (verbatim):

```text
added      3
modified   0
moved      0
removed    0
unchanged  0
notes      3
unresolved 1 — a target matched more than one file or none; spell its path to place it, e.g. [[notes/a.md]]
  missing-note
skipped    0
index      INDEX.md
elapsed    87 ms
```

Search without any model installed (keyword arm works with zero downloads):

```sh
cd ~/tmp/np-smoke-vault
NP_NO_AUTO_FETCH=1 np search . "embeddings" --mode keyword
# 0.5596 note   ideas.md  [bm25 -0.5596]   … Some prose about [embeddings] and hybrid search with …
```

Re-running `np scan` recomputes nothing: `unchanged 3`, ~30 ms.

### 4b. Protocol smoke test (this is what the frontend will use)

`scripts/smoke_serve.py` spawns `np serve`, does `initialize` with `notifications: true`, then
`scan`, `search`, `status.get`, `shutdown`:

```sh
python3 scripts/smoke_serve.py /home/agent/tmp/np-smoke-vault
```

Observed (verbatim):

```text
initialize -> {"data_dir": "/home/agent/.local/share", "features": {"fetch": true, "local_model": true, "vision": false}, "methods": ["status.get", "initialize", "search", "scan", "embed", "models.list", "shutdown"], "np": "0.3.1", "progress": true, "protocol": 1}
scan -> {'notes': 3, 'added': 0, 'unchanged': 3, 'elapsed_ms': 32} | progress: ['completed', 'locking', 'indexing', 'indexing']
search -> {"bm25": -0.5596…, "keyword_rank": 1, "kind": "note", "path": "ideas.md", "score": 0.5596…, "snippet": "# Ideas\n\n![diag …
status -> {"fetch_markers": [], "last": {"command": "search", "elapsed_ms": 0, "phase": "completed", "request": 3}, "running": null}
shutdown -> {'protocol': 1} | exit: 0
```

## 5. Verify the embedding path (real model download) — done

Run later in the same session, to get real numbers before designing the ingest UI.

Config first. **`provider = "local-model"` alone is refused** by this build:

```sh
mkdir -p ~/.config/note-processor
printf '[embeddings]\nprovider = "local-model"\nmodel = "embeddinggemma-2"\n' > ~/.config/note-processor/config.toml
```

Without the `model` line, every command dies with:

```text
np: configuration error: config io: embeddings.provider = "local-model" needs embeddings.model to name an installed model; none is set
```

Then one command does index → fetch → embed:

```sh
np setup ~/tmp/np-smoke-vault --yes     # real 0m13.057s
```

Verbatim tail of that run — note it downloaded the artifact (295.5 MiB) **and** embedded in the same
13 s, so auto-fetch works from this machine at roughly 24 MB/s:

```text
model      embeddinggemma-2
outcome    nothing owed
seen       3
current    0
embedded   3
refused    0
errors     0
images     links 1 · embedded 0 · refused 1 (missing-target 1)
verify     self-consistent, not verified
digest     re-checked against models.sha256: ok (295.5 MB)
threads    12 of 16 available (auto)
setup      indexed and embedded — semantic search is ready
```

Two llama.cpp lines on **stderr** are normal, not failures:
`init: embeddings required but some input tokens were not marked as outputs -> overriding`.

Hybrid search then returns real vectors:

```sh
np search ~/tmp/np-smoke-vault "measure embed cost of a saved note"
# 0.0164 note bench.md … (cosine 0.866)
```

### Measured costs (this host: 16 CPUs, one warm `np serve`, tiny vault)

| Operation | Wall time |
|---|---|
| `scan` (3–4 notes) | 12–31 ms |
| `embed` after editing **one** note | **0.58–0.67 s** |
| first `np setup` incl. 295 MiB download + embedding | 13.1 s |
| `np serve` RSS: idle / after embed / during vector search | 10 MB / 13 MB / **115 MB** |

Two consequences for the frontend, recorded here because they were measured rather than guessed:

- **Keep one long-lived `np serve`.** The ~0.6 s embed assumes the model is already loaded; spawning a
  process per save would pay model load every time and hold ~115 MB only briefly but constantly.
- Embedding on every save is affordable at this scale. It scales with note length (chunks), so a
  4 000-note vault costs a lot more on the *first* embed than on each subsequent single-note edit.

Also: artifacts are **per workspace** — `~/.local/share/workspaces/<key>.models/embeddinggemma-2` —
so a second vault downloads its own 295 MiB copy. Worth knowing before offering "add another vault".

Refusals to expect and respect: `NP_NO_AUTO_FETCH=1` (env, beats config), `[models] auto_fetch = false`,
or `--no-auto-fetch` for a single run. A frontend must never trigger that download without saying so —
the backend deliberately has **no `setup` method** on the stdio protocol for exactly this reason, so an
app composes it: `scan`, then `embed`, with its own consent dialog in front.

## 6. Version skew (matters)

| | released `v0.3.1` (installed) | repo `main` (`907122d`) |
|---|---|---|
| `status.get` result | `{last, fetch_markers, running}` | adds **`last_scan`** (unix seconds of the last committed scan) — commit `ac31ce9` |
| diagnostics | `-v`, `NP_LOG`, `[log] file`, per-request ids on stderr/JSONL | same, hardened (stderr cap, rule names on refusals) |

`last_scan` is the honest answer to "is the index I'm showing stale?", which a note app asks constantly.
Decide early whether the frontend **pins a released np version** (users install it themselves, we detect
`np --version` and degrade) or **requires main / a new release** (nicer API, but nobody can install it
until you tag one). Recommended: pin to the newest release that exists when we ship, feature-detect via
`initialize`'s `methods` list, and treat missing keys as unknown rather than as zero.

## 7. Where things live (Linux)

| What | Where |
|---|---|
| binary | `~/.local/bin/np` |
| index DB + model artifacts | `~/.local/share/workspaces/<key>.sqlite`, `<key>.models/` (`initialize` reports `data_dir`) |
| config (optional) | `$XDG_CONFIG_HOME/note-processor/config.toml`; Windows: `%APPDATA%\note-processor\config.toml` |
| generated catalog | `INDEX.md` (+ shards) **inside the vault** — written by np, owned by a fingerprint header |

## 8. Backend facts the frontend has to respect (learned while verifying)

- Transport is **stdio JSON-RPC 2.0 / NDJSON**, one request per line, no port, no auth, no watcher.
  `np serve` handles **one request at a time**; a search queued behind an embed waits.
- Methods: `initialize`, `search`, `scan`, `embed`, `models.list`, `status.get`, `shutdown`. There is
  **no file-writing method and no watch mode** — the app owns creating, reading, saving notes, and the
  app decides when to call `scan`. That is exactly what an "ingest" button is.
- Progress is opt-in per connection (`initialize` with `notifications: true`), phases are
  `locking / resolving / indexing / fetching / embedding / writing`, and every request ends with
  `completed` **or** `refused`. `locked` and `offline` arrive as *data* in a `completed` result — they
  must not render as failures.
- A search hit has **no `title` key**; keys are `path, snippet, kind, score, bm25, cosine,
  keyword_rank, semantic_rank, linked_from, linked_notes, source_note`, and `snippet` can contain newlines.
- Multi-word keyword queries behave like AND over tokens: `"how are moved files detected"` matched
  nothing on a vault whose sentence is "Moved files are detected by content hash", because `how`/`are`
  are absent. Worth telling users in the search box placeholder.
- Never write to the child's stdin from two threads; one writer, or serialize behind a lock.

## Reference docs in the backend repo

| File | Read it for |
|---|---|
| `docs/reference-serve.md` | the whole stdio protocol: methods, fault codes, progress phases, transcripts, client walkthrough |
| `docs/reference-cli-and-reports.md` | every `--json` field and exit code |
| `docs/reference-config.md` | all 15 config keys with defaults |
| `docs/reference-troubleshooting.md` | every sentence np prints that makes people pause |
| `docs/reference-diagnostics.md` | `-v`, `NP_LOG`, `[log] file` |
