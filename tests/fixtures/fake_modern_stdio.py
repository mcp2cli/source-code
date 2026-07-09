#!/usr/bin/env python3
"""Hermetic fake MCP server speaking the 2026-07-28 (stateless) revision.

Implements the modern stdio contract: no initialize handshake, every
request must carry `_meta['io.modelcontextprotocol/protocolVersion']`,
`server/discover` is mandatory, MRTR interim results, the
io.modelcontextprotocol/tasks extension, and subscriptions/listen.
Used by tests/protocol_versions.rs.
"""
import json
import sys

META = "io.modelcontextprotocol/"


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def ok(mid, payload):
    send({"jsonrpc": "2.0", "id": mid, "result": payload})


def err(mid, code, message, data=None):
    error = {"code": code, "message": message}
    if data is not None:
        error["data"] = data
    send({"jsonrpc": "2.0", "id": mid, "error": error})


ECHO_SCHEMA = {
    "type": "object",
    "properties": {"message": {"type": "string"}},
    "required": ["message"],
}

task_polls = {}

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    method = msg.get("method")
    mid = msg.get("id")
    params = msg.get("params") or {}
    meta = params.get("_meta") or {}

    if mid is None:
        # Client notification (e.g. notifications/cancelled) — nothing to do.
        continue

    # A modern server rejects requests without per-request version metadata.
    requested = meta.get(META + "protocolVersion")
    if requested != "2026-07-28":
        err(
            mid,
            -32022,
            "Unsupported protocol version",
            {"supported": ["2026-07-28"], "requested": requested},
        )
        continue

    if method == "server/discover":
        ok(
            mid,
            {
                "resultType": "complete",
                "supportedVersions": ["2026-07-28"],
                "capabilities": {
                    "tools": {},
                    "resources": {"subscribe": True},
                    "prompts": {},
                    "extensions": {"io.modelcontextprotocol/tasks": {}},
                },
                "serverInfo": {"name": "fake-modern-server", "version": "1.0.0"},
                "ttlMs": 3600000,
                "cacheScope": "public",
            },
        )
    elif method == "tools/list":
        ok(
            mid,
            {
                "resultType": "complete",
                "ttlMs": 60000,
                "cacheScope": "private",
                "tools": [
                    {"name": "echo", "description": "Echo a message", "inputSchema": ECHO_SCHEMA},
                    {
                        "name": "guarded",
                        "description": "Completes only after an MRTR retry",
                        "inputSchema": {"type": "object", "properties": {}},
                    },
                    {
                        "name": "slow",
                        "description": "Task-backed long-running tool",
                        "inputSchema": {"type": "object", "properties": {}},
                    },
                ],
            },
        )
    elif method == "resources/list":
        ok(
            mid,
            {
                "resultType": "complete",
                "ttlMs": 60000,
                "cacheScope": "private",
                "resources": [
                    {
                        "uri": "fake://doc",
                        "name": "doc",
                        "description": "A fake document",
                        "mimeType": "text/plain",
                    }
                ],
            },
        )
    elif method == "resources/templates/list":
        ok(mid, {"resultType": "complete", "ttlMs": 60000, "cacheScope": "private", "resourceTemplates": []})
    elif method == "prompts/list":
        ok(mid, {"resultType": "complete", "ttlMs": 60000, "cacheScope": "private", "prompts": []})
    elif method == "resources/read":
        ok(
            mid,
            {
                "resultType": "complete",
                "ttlMs": 0,
                "cacheScope": "private",
                "contents": [
                    {
                        "uri": params.get("uri"),
                        "mimeType": "text/plain",
                        "text": "fake modern resource body",
                    }
                ],
            },
        )
    elif method == "tools/call":
        name = params.get("name")
        if name == "echo":
            text = "echo: " + str((params.get("arguments") or {}).get("message", ""))
            if META + "logLevel" in meta:
                text += " [logLevel=" + meta[META + "logLevel"] + "]"
            ok(mid, {"resultType": "complete", "content": [{"type": "text", "text": text}]})
        elif name == "guarded":
            # MRTR: first attempt returns an interim result whose opaque
            # requestState the client must echo verbatim on the retry.
            if params.get("requestState") == "state-token-1":
                ok(
                    mid,
                    {
                        "resultType": "complete",
                        "content": [{"type": "text", "text": "guarded completed after retry"}],
                    },
                )
            else:
                ok(mid, {"resultType": "input_required", "requestState": "state-token-1"})
        elif name == "slow":
            ok(
                mid,
                {
                    "resultType": "task",
                    "task": {"taskId": "task-42", "status": "working", "pollIntervalMs": 50, "ttlMs": 60000},
                },
            )
        else:
            err(mid, -32602, "unknown tool: " + str(name))
    elif method == "tasks/get":
        tid = params.get("taskId")
        polls = task_polls.get(tid, 0) + 1
        task_polls[tid] = polls
        if polls >= 2:
            ok(
                mid,
                {
                    "resultType": "complete",
                    "task": {
                        "taskId": tid,
                        "status": "completed",
                        "result": {"content": [{"type": "text", "text": "slow task finished"}]},
                    },
                },
            )
        else:
            ok(
                mid,
                {
                    "resultType": "complete",
                    "task": {"taskId": tid, "status": "working", "pollIntervalMs": 50},
                },
            )
    elif method == "tasks/cancel":
        ok(mid, {"resultType": "complete"})
    elif method == "subscriptions/listen":
        send(
            {
                "jsonrpc": "2.0",
                "method": "notifications/subscriptions/acknowledged",
                "params": {
                    "_meta": {META + "subscriptionId": mid},
                    "notifications": params.get("notifications") or {},
                },
            }
        )
        # The stream stays open; the client ends it with notifications/cancelled.
    elif method == "completion/complete":
        ok(
            mid,
            {
                "resultType": "complete",
                "completion": {"values": ["alpha", "beta"], "hasMore": False, "total": 2},
            },
        )
    else:
        # `ping`, `logging/setLevel`, `resources/subscribe`, … were removed
        # in 2026-07-28 — a modern-only server does not implement them.
        err(mid, -32601, "method not found: " + str(method))
