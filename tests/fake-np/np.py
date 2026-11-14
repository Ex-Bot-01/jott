#!/usr/bin/env python3
"""A stand-in for `np serve`, so protocol behaviour is testable without the real binary.

Reads NDJSON on stdout-sized lines and answers from a scenario file named by
FAKE_NP_SCENARIO (JSON):

  initialize            the result object for `initialize`
  initialize_requires   param keys `initialize` must carry, else -32602
  (default) progress is opt-in: `initialize` answers with `progress: true` and streams
                              `np/progress` only on a connection that sent `"notifications": true`
  initialize_always_progress    answer with `progress: true` and stream regardless, as np 0.3.1 did
  replies               result object per method
  faults                {method: {code, message}} — an error object instead of a result
  sleep_ms              delay before answering a method
  die_on_call           {method: N} — exit(3) without answering the Nth call: a crashed child
  progress_counters     {method: [{phase, chunks_done, chunks_total, ...}]} — extra `np/progress`
                        lines written before the answer, with np's counter keys kept as sent
  record                a file path: one JSON line per request received, `{"method", "params"}`.
                        Two connections append to it, so a test can read what np was asked for and
                        in what order.
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
    "methods": ["status.get", "initialize", "search", "scan", "embed", "models.list", "links.list", "shutdown"],
    # What this stand-in emulates. Bump it with `packaging/np-pin.json`, and read the release notes before you do:
    # 0.4.0 changed when `progress` appears (below) and added keys to `search` and `models.list`.
    "np": "0.4.0",
    "protocol": 1,
})

# Progress is a favour the client asked for. np 0.4.0 answers an `initialize` that did not send
# `"notifications": true` with no `progress` key and sends no notification at all on that connection — a client
# reading a fixed number of lines per request must not be surprised by one. So the flag is per connection here,
# as it is in the real thing, rather than a scenario switch: Jott's writer asks, and a writer that forgot to ask
# goes mute for the life of the run, which is precisely the bug this used to hide.
PROGRESS = False


def record(method, params):
    path = SCENARIO.get("record")
    if not path:
        return
    # Opened per line on purpose: the writer and reader connections both append, and a short
    # O_APPEND write keeps one call inside one line.
    with open(path, "a") as fh:
        fh.write(json.dumps({"method": method, "params": params}) + "\n")


def write(obj):
    sys.stdout.write(json.dumps(obj, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def note(req_id, command, phase, extra=None):
    if not PROGRESS:
        return
    params = {"command": command, "phase": phase, "since_ms": 0}
    if req_id is not None:
        params["request"] = req_id
    if extra:
        params.update(extra)
    write({"jsonrpc": "2.0", "method": "np/progress", "params": params})


# Real np is `np serve`. With no subcommand it prints its usage on stderr and exits 2 without reading a
# byte of stdin — which is exactly what happened on the first machine that ran the shipped installer, and
# no test saw it because this stand-in happily speaks NDJSON with any arguments at all. A scenario can ask
# for the same manners.
if want := SCENARIO.get("require_args"):
    if sys.argv[1:] != list(want):
        sys.stderr.write("note-processor command line interface\nUsage: np <COMMAND>\n")
        sys.exit(2)

# A child that dies mid-flight, saying why on stderr: what Jott has to report when it never answers.
if text := SCENARIO.get("die_with_stderr"):
    sys.stderr.write(text)
    sys.exit(int(SCENARIO.get("die_code", 1)))

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
    # Recorded before anything can delay or refuse it: a test asking "was embed ever sent?" wants
    # the request as it arrived, not as it was answered.
    record(method, params)
    if method == "initialize":
        missing = [k for k in SCENARIO.get("initialize_requires", []) if k not in params]
        if missing:
            write({"error": {"code": -32602,
                             "message": f'params need a "{missing[0]}" value'},
                   "id": req_id, "jsonrpc": "2.0"})
            note(req_id, method, "refused", {"code": -32602})
            continue
        result = dict(INITIALIZE)
        if SCENARIO.get("initialize_always_progress"):
            # For a test that wants the 0.3.1 manners on purpose — advertising and streaming regardless.
            result["progress"] = True
            PROGRESS = True
        elif params.get("notifications") is True:
            # Advertised by the key's presence, and only to the connection that asked for it.
            result["progress"] = True
            PROGRESS = True
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
    if method in SCENARIO.get("silent", []):
        # Takes the request and says nothing, for a client that has to stop waiting at some point. The child
        # stays alive on purpose: a wedged request and a dead child are different failures, and only one of them
        # should cost the caller its connection.
        continue
    if sleep := SCENARIO.get("sleep_ms", {}).get(method):
        time.sleep(sleep / 1000.0)
    note(req_id, method, "locking")
    for extra in SCENARIO.get("progress_counters", {}).get(method, []):
        # np's own counter keys travel untouched: `chunks_done`/`chunks_total` while embedding,
        # `bytes_done`/`bytes_total` while fetching. Absent means unknown, and this stand-in keeps it
        # absent rather than filling in a zero.
        extra = dict(extra)
        phase = extra.pop("phase", "working")
        note(req_id, method, phase, extra)
    if seq := SCENARIO.get("sequence", {}).get(method):
        # Answers in order, repeating the last entry once they run out: a queue that has to notice a state
        # *change* — locked, then committed, then locked again — cannot be tested against one fixed reply.
        step = seq[min(CALLS[method] - 1, len(seq) - 1)]
        if fault := step.get("fault"):
            write({"error": {"code": fault["code"], "message": fault["message"]}, "id": req_id, "jsonrpc": "2.0"})
            note(req_id, method, "refused", {"code": fault["code"]})
            continue
        if reply := step.get("reply"):
            write({"id": req_id, "jsonrpc": "2.0", "result": reply})
            note(req_id, method, "completed")
            continue
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
