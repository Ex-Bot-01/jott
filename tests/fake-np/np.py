#!/usr/bin/env python3
"""A stand-in for `np serve`, so protocol behaviour is testable without the real binary.

Reads NDJSON on stdout-sized lines and answers from a scenario file named by
FAKE_NP_SCENARIO (JSON):

  initialize            the result object for `initialize`
  initialize_requires   param keys `initialize` must carry, else -32602
  initialize_progress_by_param  answer with `progress: true` only when the request
                                asked for notifications — what real np does
  replies               result object per method
  faults                {method: {code, message}} — an error object instead of a result
  sleep_ms              delay before answering a method
  die_on_call           {method: N} — exit(3) without answering the Nth call: a crashed child
  require_mode          refuse a `search` whose mode is not this one, naming what arrived

Anything unlisted gets np's own -32601 sentence. Exits 0 at EOF, the way np does.
Deliberately dumb: it exists to be wrong in specific ways on request.
"""
import json
import os
import sys
import time

SCENARIO = {}
CALLS = {}
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
    params = msg.get("params") or {}
    if method == "initialize":
        missing = [k for k in SCENARIO.get("initialize_requires", []) if k not in params]
        if missing:
            write({"error": {"code": -32602,
                             "message": f'params need a "{missing[0]}" value'},
                   "id": req_id, "jsonrpc": "2.0"})
            note(req_id, method, "refused", {"code": -32602})
            continue
        result = dict(INITIALIZE)
        if SCENARIO.get("initialize_progress_by_param"):
            # np advertises progress support by the key's presence, and only on a connection
            # that asked for it — so a client that hardcodes the flag cannot pass for one.
            if params.get("notifications") is True:
                result["progress"] = True
            else:
                result.pop("progress", None)
        write({"id": req_id, "jsonrpc": "2.0", "result": result})
        note(req_id, method, "completed")
        continue
    if method == "shutdown":
        write({"id": req_id, "jsonrpc": "2.0", "result": {"protocol": 1}})
        sys.exit(0)
    CALLS[method] = CALLS.get(method, 0) + 1
    if SCENARIO.get("die_on_call", {}).get(method) == CALLS[method]:
        # A child that dies mid-request: no answer, no goodbye, exit status nobody asked for.
        sys.stdout.flush()
        os._exit(3)
    if method == "search" and (want := SCENARIO.get("require_mode")):
        if params.get("mode") != want:
            write({"error": {"code": -32602,
                             "message": f"this stand-in answers only mode `{want}`, not `{params.get('mode')}`"},
                   "id": req_id, "jsonrpc": "2.0"})
            note(req_id, method, "refused", {"code": -32602})
            continue
    if sleep := SCENARIO.get("sleep_ms", {}).get(method):
        time.sleep(sleep / 1000.0)
    note(req_id, method, "locking")
    if fault := SCENARIO.get("faults", {}).get(method):
        write({"error": {"code": fault["code"], "message": fault["message"]},
               "id": req_id, "jsonrpc": "2.0"})
        note(req_id, method, "refused", {"code": fault["code"]})
        continue
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
