#!/usr/bin/env python3
"""A stand-in for `np serve`, so protocol behaviour is testable without the real binary.

Reads NDJSON on stdout-sized lines and answers from a scenario file named by
FAKE_NP_SCENARIO (JSON: {"initialize": {...}, "replies": {"scan": {...}}, "sleep_ms":
{"embed": 1500}}). Anything unknown gets np's own -32601 sentence. Exits 0 at EOF, the
way np does. Deliberately dumb: it exists to be wrong in specific ways on request.
"""
import json
import os
import sys
import time

SCENARIO = {}
if path := os.environ.get("FAKE_NP_SCENARIO"):
    with open(path) as fh:
        SCENARIO = json.load(fh)

INITIALIZE = SCENARIO.get("initialize", {
    "data_dir": "/fake/data",
    "features": {"fetch": True, "local_model": True, "vision": False},
    "methods": ["status.get", "initialize", "search", "scan", "embed", "models.list", "shutdown"],
    "np": "0.3.1",
    "progress": True,
    "protocol": 1,
})


def write(obj):
    sys.stdout.write(json.dumps(obj, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def note(req_id, command, phase, extra=None):
    params = {"command": command, "phase": phase, "since_ms": 0}
    if req_id is not None:
        params["request"] = req_id
    if extra:
        params.update(extra)
    write({"jsonrpc": "2.0", "method": "np/progress", "params": params})


for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        msg = json.loads(line)
    except json.JSONDecodeError:
        write({"error": {"code": -32700, "message": "expected ident at line 1 column 2"},
               "id": None, "jsonrpc": "2.0"})
        continue
    method, req_id = msg.get("method"), msg.get("id")
    if method == "initialize":
        write({"id": req_id, "jsonrpc": "2.0", "result": INITIALIZE})
        note(req_id, "initialize", "completed")
        continue
    if method == "shutdown":
        write({"id": req_id, "jsonrpc": "2.0", "result": {"protocol": 1}})
        sys.exit(0)
    if sleep := SCENARIO.get("sleep_ms", {}).get(method):
        time.sleep(sleep / 1000.0)
    note(req_id, method, "locking")
    result = SCENARIO.get("replies", {}).get(method)
    if result is None:
        write({"error": {"code": -32601,
                         "message": f"unknown method `{method}`: this build answers status.get, "
                                    "initialize, search, scan, embed, models.list, shutdown"},
               "id": req_id, "jsonrpc": "2.0"})
        note(req_id, method, "refused", {"code": -32601})
        continue
    write({"id": req_id, "jsonrpc": "2.0", "result": result})
    note(req_id, method, "completed")
