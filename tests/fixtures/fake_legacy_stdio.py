#!/usr/bin/env python3
"""Hermetic fake MCP server speaking only the 2025-11-25 (legacy) revision.

Requires the classic initialize handshake and answers unknown
pre-initialize methods (like the modern `server/discover` probe) with a
plain method-not-found error, which must drive mcp2cli's fallback to the
legacy handshake. Used by tests/protocol_versions.rs.
"""
import json
import sys


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def ok(mid, payload):
    send({"jsonrpc": "2.0", "id": mid, "result": payload})


def err(mid, code, message):
    send({"jsonrpc": "2.0", "id": mid, "error": {"code": code, "message": message}})


initialized = False

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    method = msg.get("method")
    mid = msg.get("id")
    params = msg.get("params") or {}

    if mid is None:
        if method == "notifications/initialized":
            initialized = True
        continue

    if method == "initialize":
        ok(
            mid,
            {
                "protocolVersion": "2025-11-25",
                "capabilities": {"tools": {}, "logging": {}},
                "serverInfo": {"name": "fake-legacy-server", "version": "1.0.0"},
            },
        )
    elif method == "tools/list":
        ok(
            mid,
            {
                "tools": [
                    {
                        "name": "echo",
                        "description": "Echo a message",
                        "inputSchema": {
                            "type": "object",
                            "properties": {"message": {"type": "string"}},
                            "required": ["message"],
                        },
                    }
                ]
            },
        )
    elif method == "resources/list":
        ok(mid, {"resources": []})
    elif method == "prompts/list":
        ok(mid, {"prompts": []})
    elif method == "tools/call":
        arguments = params.get("arguments") or {}
        ok(
            mid,
            {
                "content": [
                    {"type": "text", "text": "legacy echo: " + str(arguments.get("message", ""))}
                ]
            },
        )
    elif method == "ping":
        ok(mid, {})
    elif method == "logging/setLevel":
        ok(mid, {})
    else:
        err(mid, -32601, "method not found: " + str(method))
