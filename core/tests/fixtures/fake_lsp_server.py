#!/usr/bin/env python3
"""Minimal stdio LSP fixture for write-through ordering and freshness tests."""

import json
import sys
import time


def read_message():
    length = None
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        if line in (b"\r\n", b"\n"):
            break
        name, _, value = line.partition(b":")
        if name.lower() == b"content-length":
            length = int(value.strip())
    if length is None:
        raise RuntimeError("missing Content-Length")
    return json.loads(sys.stdin.buffer.read(length))


def send(message):
    body = json.dumps(message, separators=(",", ":")).encode()
    sys.stdout.buffer.write(f"Content-Length: {len(body)}\r\n\r\n".encode())
    sys.stdout.buffer.write(body)
    sys.stdout.buffer.flush()


methods = []
document_uri = None
while (message := read_message()) is not None:
    method = message.get("method")
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": message["id"], "result": {"capabilities": {}}})
    elif method == "shutdown":
        send({"jsonrpc": "2.0", "id": message["id"], "result": None})
    elif method == "exit":
        break
    elif method == "textDocument/didOpen":
        document_uri = message["params"]["textDocument"]["uri"]
    elif method in (
        "workspace/didChangeWatchedFiles",
        "textDocument/didChange",
        "textDocument/didSave",
    ):
        methods.append(method)
        if method == "textDocument/didSave":
            document_uri = message["params"]["textDocument"]["uri"]
            if document_uri and document_uri.endswith("slow.rs"):
                time.sleep(0.7)
            stale = {
                "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}},
                "message": "stale diagnostic must be ignored",
            }
            fresh = {
                "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}},
                "message": "fresh:" + ",".join(methods),
            }
            send({
                "jsonrpc": "2.0",
                "method": "textDocument/publishDiagnostics",
                "params": {"uri": document_uri, "version": 1, "diagnostics": [stale]},
            })
            send({
                "jsonrpc": "2.0",
                "method": "textDocument/publishDiagnostics",
                "params": {"uri": document_uri, "version": 2, "diagnostics": [fresh]},
            })
