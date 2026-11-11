import json, subprocess, sys

proc = subprocess.Popen(["np", "serve"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                        stderr=subprocess.DEVNULL, text=True, encoding="utf-8", bufsize=1)
vault = sys.argv[1] if len(sys.argv) > 1 else "/home/agent/tmp/np-smoke-vault"

def call(i, method, params=None):
    m = {"jsonrpc": "2.0", "id": i, "method": method}
    if params: m["params"] = params
    proc.stdin.write(json.dumps(m, separators=(",", ":")) + "\n"); proc.stdin.flush()

def read_until(i):
    notes = []
    while True:
        line = proc.stdout.readline()
        if line == "": return None, notes
        m = json.loads(line)
        if m.get("id") == i and "result" in m or m.get("id") == i and "error" in m: return m, notes
        if m.get("method") == "np/progress": notes.append(m["params"])

call(1, "initialize", {"workspace": vault, "notifications": True})
r, n = read_until(1); print("initialize ->", json.dumps(r["result"], sort_keys=True)[:300])
call(2, "scan", {"workspace": vault}); r, n = read_until(2)
print("scan ->", {k: r["result"][k] for k in ("notes","added","unchanged","elapsed_ms")}, "| progress:", [x["phase"] for x in n])
call(3, "search", {"workspace": vault, "query": "embeddings", "mode": "keyword"}); r, n = read_until(3)
print("search ->", json.dumps(r["result"]["hits"][0], sort_keys=True)[:220] if r["result"]["hits"] else "no hits")
call(4, "status.get", {"workspace": vault}); r, n = read_until(4)
print("status ->", json.dumps(r["result"], sort_keys=True))
call(5, "shutdown"); r, n = read_until(5); print("shutdown ->", r["result"], "| exit:", proc.wait())
