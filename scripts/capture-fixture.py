"""Regenerate tests/fixtures/np-serve-transcript.ndjson against the np on PATH.

Creates two throwaway vaults under /tmp (a two-note one for the clean session, a 200-note one whose
first embed runs ~106 s so contention is observable), drives `np serve`, and writes stdout verbatim.
The embedder is terminated with SIGTERM on purpose; np leaves no lock behind a killed process.
Run next to the repo root:  python3 scripts/capture-fixture.py
"""import json, os, subprocess, time

FIX = "tests/fixtures/np-serve-transcript.ndjson"
V = "/tmp/np-fixture-vault"
W2 = "/tmp/bench2-vault"
out = open(FIX, "w")
env = dict(os.environ)
env["NP_NO_AUTO_FETCH"] = "1"


def log(*a):
    print(*a, flush=True)


def make_bench_vault(path, notes=200, words=140):
    """A throwaway vault big enough that its first embed runs for minutes."""
    import random
    from pathlib import Path
    random.seed(7)
    root = Path(path)
    vocab = ("chunk budget embedding vector note link index scan search model token file path "
             "vault title heading item hash digest thread worker writer reader lock query rank "
             "fusion score snippet folder rename alias target").split()
    for d in ("notes", "archive/2026", "inbox"):
        (root / d).mkdir(parents=True, exist_ok=True)
    for i in range(notes):
        folder = ["notes", "archive/2026", "inbox"][i % 3]
        body = " ".join(random.choice(vocab) for _ in range(words))
        link = random.choice(["[[note-%03d]]" % random.randint(0, notes - 1),
                              "[x](notes/note-%03d.md)" % random.randint(0, notes - 1),
                              "[[missing-%02d]]" % random.randint(0, 9)])
        (root / folder / ("note-%03d.md" % i)).write_text(
            "# Note %d\n\n%s\n\nSee %s for the rest.\n\n- [ ] a checkbox line\n- [x] another\n" % (i, body, link))


import shutil
shutil.rmtree(W2, ignore_errors=True)
make_bench_vault(W2)
log("bench vault built at %s" % W2)


def spawn():
    return subprocess.Popen(
        ["np", "serve"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL, text=True, bufsize=1, env=env, cwd="/tmp")


def send(p, s):
    p.stdin.write(s + "\n")
    p.stdin.flush()


def req(p, i, m, params=None):
    d = {"jsonrpc": "2.0", "id": i, "method": m}
    if params is not None:
        d["params"] = params
    send(p, json.dumps(d, separators=(",", ":")))


def wr(l):
    out.write(l)
    out.flush()


def pump_until(p, pred):
    while True:
        l = p.stdout.readline()
        if not l:
            return None
        wr(l)
        try:
            m = json.loads(l)
        except Exception:
            continue
        if pred(m):
            return m


def is_resp(i):
    return lambda m: m.get("id") == i and ("result" in m or "error" in m)


p = spawn()
req(p, 1, "initialize", {"workspace": V, "notifications": True}); pump_until(p, is_resp(1)); log("init ok")
req(p, 2, "scan", {"workspace": V}); pump_until(p, is_resp(2)); log("scan ok")
req(p, 3, "search", {"workspace": V, "query": "hash", "mode": "keyword", "limit": 5}); pump_until(p, is_resp(3)); log("search ok")
req(p, 4, "embed", {"workspace": V}); pump_until(p, is_resp(4)); log("embed refusal ok")
req(p, 5, "links.list", {"workspace": V}); pump_until(p, is_resp(5)); log("-32601 ok")
req(p, 6, "search", {"workspace": V, "query": "hash", "mode": "telepathy"}); pump_until(p, is_resp(6)); log("-32602 mode ok")
send(p, "this is not json"); pump_until(p, lambda m: (m.get("error") or {}).get("code") == -32700); log("-32700 ok")
req(p, 7, "search", {"workspace": V}); pump_until(p, is_resp(7)); log("-32602 missing key ok")
p.stdin.close(); p.wait(); log("session A exited " + str(p.returncode))

a = spawn()
req(a, 1, "initialize", {"workspace": W2}); wr(a.stdout.readline()); log("writer initialised")
req(a, 2, "embed", {"workspace": W2}); log("full embed started (holds the write lock for ~106 s)")
time.sleep(3)
b = spawn()
req(b, 1, "initialize", {"workspace": V}); wr(b.stdout.readline())
req(b, 2, "scan", {"workspace": W2}); r = pump_until(b, is_resp(2)); log("contender scan: " + str(r)[:200])
req(b, 3, "status.get", {"workspace": W2}); pump_until(b, is_resp(3)); log("status ok")
b.stdin.close(); b.wait()
a.terminate(); a.wait(timeout=15); log("writer terminated")
log("locked present: " + str('"locked":true' in open(FIX).read()))
