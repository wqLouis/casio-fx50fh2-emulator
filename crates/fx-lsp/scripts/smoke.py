#!/usr/bin/env python3
"""Minimal end-to-end check for fx-lsp.

Speaks framed JSON-RPC to the server over stdio: sends ``initialize``, opens a
document with a deliberate syntax error, prints the published diagnostic, then
shuts the server down.  Standard library only.

Usage::

    python3 scripts/smoke.py [path/to/fx-lsp]
"""

import json
import subprocess
import sys


def frame(message):
    body = json.dumps(message).encode("utf-8")
    return b"Content-Length: %d\r\n\r\n%s" % (len(body), body)


def read_message(stream):
    length = 0
    while True:
        line = stream.readline()
        if not line:
            return None
        line = line.strip()
        if not line:
            break
        if line.lower().startswith(b"content-length:"):
            length = int(line.split(b":", 1)[1].strip())
    return json.loads(stream.read(length))


def read_until(stream, method):
    while True:
        message = read_message(stream)
        if message is None:
            raise SystemExit(f"server closed before sending {method}")
        if message.get("method") == method:
            return message
        # Surface errors so a failure is obvious.
        if "error" in message:
            raise SystemExit(f"unexpected error reply: {message}")


def main():
    binary = sys.argv[1] if len(sys.argv) > 1 else "target/debug/fx-lsp"
    proc = subprocess.Popen(
        [binary], stdin=subprocess.PIPE, stdout=subprocess.PIPE
    )

    def send(message):
        proc.stdin.write(frame(message))
        proc.stdin.flush()

    send(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"processId": None, "rootUri": None, "capabilities": {}},
        }
    )
    response = read_message(proc.stdout)
    print("initialize capabilities:", json.dumps(response["result"]["capabilities"]))

    send({"jsonrpc": "2.0", "method": "initialized", "params": {}})
    send(
        {
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///demo.fx",
                    "languageId": "fx",
                    "version": 1,
                    "text": "A@B",
                }
            },
        }
    )
    published = read_until(proc.stdout, "textDocument/publishDiagnostics")
    for diagnostic in published["params"]["diagnostics"]:
        print(
            "diagnostic:",
            diagnostic["code"],
            diagnostic["range"],
            diagnostic["message"],
        )

    send({"jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": None})
    while True:
        message = read_message(proc.stdout)
        if message is None or message.get("id") == 2:
            break
    send({"jsonrpc": "2.0", "method": "exit", "params": None})
    proc.stdin.close()
    proc.wait(timeout=5)
    print("server exited with", proc.returncode)


if __name__ == "__main__":
    main()
