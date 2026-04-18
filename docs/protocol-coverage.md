# Protocol Coverage

**mcp2cli** implements the [Model Context Protocol 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25) end-to-end. This page is the canonical reference for **what is supported, how it surfaces on the CLI, and where the implementation lives in source**.

The MCP spec groups protocol features into lifecycle, primitives (tools, resources, prompts, completion), server-initiated operations (elicitation, sampling, roots), notifications, and utilities. mcp2cli is organised the same way; this page walks the spec in that order.

- [Lifecycle](#lifecycle)
- [Discovery](#discovery)
- [Tool invocation](#tool-invocation)
- [Resources](#resources)
- [Prompts](#prompts)
- [Completions](#completions)
- [Server-initiated requests](#server-initiated-requests)
  - [Elicitation](#elicitation)
  - [Sampling](#sampling)
  - [Roots](#roots)
- [Notifications](#notifications)
  - [Progress](#progress)
  - [Logging](#logging)
  - [Cancellation](#cancellation)
  - [List-changed](#list-changed)
  - [Resource updates](#resource-updates)
- [Tasks (long-running operations)](#tasks-long-running-operations)
- [Transports](#transports)
- [Known gaps](#known-gaps)

---

## Lifecycle

| Method / notification | Direction | mcp2cli |
|---|---|---|
| `initialize` | client → server | **Supported** — sent on first request of every session |
| `notifications/initialized` | client → server | **Supported** — sent after `initialize` response |
| `ping` | bidirectional | **Supported** — exposed as `mcp2cli ping` and used for `doctor` health checks |

**What mcp2cli does.** Every transport ([`StdioMcpClient`], [`StreamableHttpMcpClient`], [`DaemonMcpClient`]) runs its own `initialize` handshake through [`ProtocolEngine::initialize`](../src/mcp/protocol.rs) and advertises client capabilities (elicitation, sampling, roots). The negotiated server capabilities are cached in the state store and inspected by `doctor` / `inspect` to short-circuit operations the server doesn't support.

**CLI surface.**

```bash
mcp2cli ping              # liveness probe — minimal round-trip
mcp2cli doctor            # runtime health: init round-trip, cap intersection, auth status
mcp2cli inspect           # full capability dump for the active config
```

**Source.** [`src/mcp/protocol.rs`](../src/mcp/protocol.rs) — `ProtocolEngine::initialize`, `complete_initialize`, protocol version constant `DEFAULT_MCP_PROTOCOL_VERSION`.

---

## Discovery

| Method / notification | Direction | mcp2cli |
|---|---|---|
| `tools/list` | client → server | **Supported** |
| `resources/list` | client → server | **Supported** |
| `resources/templates/list` | client → server | **Supported** |
| `prompts/list` | client → server | **Supported** |
| `notifications/tools/list_changed` | server → client | **Supported** — invalidates cache |
| `notifications/resources/list_changed` | server → client | **Supported** — invalidates cache |
| `notifications/prompts/list_changed` | server → client | **Supported** — invalidates cache |

**What mcp2cli does.** `ls` issues the four `*/list` requests and persists the merged inventory to the state store as a [`DiscoveryInventoryView`]. Subsequent commands read from the cache rather than re-querying on every invocation. Inbound `list_changed` notifications write a stale marker file; the next `ls` refreshes the cache.

The cache is what powers the **dynamic CLI** — [`apps::dynamic::build_dynamic_cli`](../src/apps/dynamic.rs) reads a [`CommandManifest`](../src/apps/manifest.rs) built from the inventory and materialises a `clap` tree where every tool / resource template / prompt becomes a subcommand with flags.

**CLI surface.**

```bash
mcp2cli ls                         # populate / refresh the cache
mcp2cli ls --category tools        # filter by primitive
mcp2cli ls --stale                 # force re-discovery ignoring cache
```

**Source.** [`src/mcp/protocol.rs`](../src/mcp/protocol.rs) — `discover_method_name`, `map_discovery_to_request`. Cache lives in [`src/runtime/state.rs`](../src/runtime/state.rs). Dynamic CLI in [`src/apps/dynamic.rs`](../src/apps/dynamic.rs).

---

## Tool invocation

| Method | Direction | mcp2cli |
|---|---|---|
| `tools/call` | client → server | **Supported** — with progress token + optional `_meta.task` |

**What mcp2cli does.** Each discovered tool becomes a clap subcommand with flags derived from its JSON Schema input. Type coercion handles strings, numbers, booleans, enums, arrays, and nested objects; complex shapes fall back to a `--config <JSON>` escape hatch. `--background` elevates the call to a [task](#tasks-long-running-operations) so the command returns immediately with a job id while the server works.

Progress tokens are attached automatically via `_meta.progressToken`; matching [`notifications/progress`](#progress) are correlated back to the invocation and rendered as runtime events.

**CLI surface.**

```bash
# Dynamic (typed flags per tool):
work email send --to user@example.com --body "Meeting at 3"
work search --query "metrics" --limit 10

# Static/protocol-shaped (any server, opaque args):
mcp2cli invoke email.send --arg to=user@example.com --arg body=@body.txt
mcp2cli invoke slow-job --background          # returns job id; see `jobs`
```

**Source.** Request mapping: [`src/mcp/protocol.rs`](../src/mcp/protocol.rs) — `map_operation_to_request` (InvokeAction arm), progress-token injection, task `_meta` injection. CLI generation: [`src/apps/dynamic.rs`](../src/apps/dynamic.rs), [`src/apps/manifest.rs`](../src/apps/manifest.rs). Static surface: [`src/apps/bridge.rs`](../src/apps/bridge.rs).

---

## Resources

| Method / notification | Direction | mcp2cli |
|---|---|---|
| `resources/read` | client → server | **Supported** |
| `resources/subscribe` | client → server | **Supported** |
| `resources/unsubscribe` | client → server | **Supported** |
| `notifications/resources/updated` | server → client | **Supported** — surfaced as a `RuntimeEvent::ResourceUpdated` |

**What mcp2cli does.** Concrete resources read with `get <URI>`; parameterised resource templates (e.g. `file:///{path}`) surface as typed commands whose flags fill the template parameters. Subscriptions keep running after the initial `subscribe` returns — updates flow through the event broker to stderr, webhooks, Unix sockets, or SSE endpoints, depending on [event sink configuration](features/event-system.md).

**CLI surface.**

```bash
mcp2cli get file:///etc/hosts                  # read a concrete URI
work get "file:///{path}" --path docs/index.md # parameterised template
mcp2cli subscribe file:///config/current.yaml  # stream updates
mcp2cli unsubscribe file:///config/current.yaml
```

**Source.** [`src/mcp/protocol.rs`](../src/mcp/protocol.rs) — `map_operation_to_request` (ReadResource, SubscribeResource, UnsubscribeResource). Update handling: [`src/mcp/handler.rs`](../src/mcp/handler.rs) — `handle_resource_updated`.

---

## Prompts

| Method | Direction | mcp2cli |
|---|---|---|
| `prompts/get` | client → server | **Supported** |

**What mcp2cli does.** Each prompt becomes a subcommand. Typed flags are derived from the prompt's declared arguments; dotted argument names (e.g. `context.thread_id`) nest under `--context-thread-id` by default, with overlay support for renaming.

**CLI surface.**

```bash
work prompt review-diff --diff-file hunk.patch
work prompt summarise --context-thread-id 123
```

**Source.** [`src/mcp/protocol.rs`](../src/mcp/protocol.rs) — `map_operation_to_request` (RunPrompt arm), `flatten_prompt_arguments`.

---

## Completions

| Method | Direction | mcp2cli |
|---|---|---|
| `completion/complete` | client → server | **Supported** — with `ref.context` per MCP 2025-11-25 |

**What mcp2cli does.** `complete` asks the server for suggested values for a reference point (a resource URI template variable or a prompt argument). The MCP 2025-11-25 extension lets callers include a `ref.context` object so the server can answer context-sensitively (for example, completing only files inside a previously chosen directory).

**CLI surface.**

```bash
mcp2cli complete \
  --ref-type prompt \
  --ref-name summarise \
  --arg-name context.thread_id \
  --value 12
```

**Source.** [`src/mcp/model.rs`](../src/mcp/model.rs) — `McpOperation::Complete` with context. [`src/mcp/protocol.rs`](../src/mcp/protocol.rs) — `map_operation_to_request` (Complete arm).

---

## Server-initiated requests

MCP is bidirectional. Servers may send the client requests of their own; all three supported kinds have terminal-first UX but emit structured events for headless or UI-driven deployments.

### Elicitation

| Method | Direction | mcp2cli |
|---|---|---|
| `elicitation/create` | server → client | **Supported** — form + URL modes |

Servers request structured input from the user mid-operation — e.g. a destructive action that wants confirmation or a missing parameter. mcp2cli renders:

- **Form mode** — a terminal prompt per field from the JSON Schema, with type validation.
- **URL mode** — prints a URL and waits for the user to complete the flow out-of-band, then continues.

See [`docs/features/elicitation-and-sampling.md`](features/elicitation-and-sampling.md).

**Source.** [`src/mcp/handler.rs`](../src/mcp/handler.rs) — `handle_elicitation_request`.

### Sampling

| Method | Direction | mcp2cli |
|---|---|---|
| `sampling/createMessage` | server → client | **Supported** |

Servers ask the client to run an LLM completion on their behalf. mcp2cli always keeps the human in the loop: the inbound request is displayed with the tool context and pending message, and the user approves, edits, or rejects before the reply is forwarded.

See [`docs/features/elicitation-and-sampling.md`](features/elicitation-and-sampling.md).

**Source.** [`src/mcp/handler.rs`](../src/mcp/handler.rs) — `handle_sampling_request`.

### Roots

| Method / notification | Direction | mcp2cli |
|---|---|---|
| `roots/list` | server → client | **Supported** |
| `notifications/roots/list_changed` | client → server | **Supported** |

The client advertises a list of filesystem or URI roots that scope where a server may read/write. Configure roots in the app config ([`roots` section](reference/config-reference.md)); they are returned on demand and a `list_changed` notification is sent whenever the set changes.

**Source.** [`src/mcp/handler.rs`](../src/mcp/handler.rs) — `handle_roots_list`.

---

## Notifications

### Progress

| Notification | Direction | mcp2cli |
|---|---|---|
| `notifications/progress` | server → client | **Supported** |

Every long-running request (`tools/call`, `resources/read`, `prompts/get`, task operations) is stamped with a unique `_meta.progressToken`. Incoming progress notifications are correlated back to the operation and emitted as `RuntimeEvent::Progress` — rendered on stderr for human users, streamed as NDJSON to configured event sinks for programmatic consumers.

**Source.** Token injection: [`src/mcp/protocol.rs`](../src/mcp/protocol.rs) — `inject_progress_token`. Notification routing: [`src/mcp/handler.rs`](../src/mcp/handler.rs) — `handle_progress`.

### Logging

| Method / notification | Direction | mcp2cli |
|---|---|---|
| `logging/setLevel` | client → server | **Supported** |
| `notifications/message` | server → client | **Supported** |

Clients can instruct the server to turn verbosity up or down mid-session. Server logs flow through the handler and are surfaced both in the local `tracing` subscriber (visible to `mcp2cli daemon` log files) and as `RuntimeEvent::Message`.

**CLI surface.**

```bash
mcp2cli log level debug     # ask the server for debug-level logs
mcp2cli log level warn      # back off
```

**Source.** [`src/mcp/protocol.rs`](../src/mcp/protocol.rs) — `map_operation_to_request` (SetLoggingLevel arm). [`src/mcp/handler.rs`](../src/mcp/handler.rs) — `handle_server_log`.

### Cancellation

| Notification | Direction | mcp2cli |
|---|---|---|
| `notifications/cancelled` | bidirectional | **Supported** |

Pressing Ctrl+C during a pending request sends `notifications/cancelled` to the server referencing the outstanding request id. Incoming cancels (a server declining to finish an operation it acknowledged) are accepted and surfaced as a runtime event; no state is corrupted if a cancel arrives after the response.

**Source.** [`src/mcp/handler.rs`](../src/mcp/handler.rs) — `handle_notification` (matches on `notifications/cancelled`). [`src/mcp/client.rs`](../src/mcp/client.rs) — `cancel_request`.

### List-changed

| Notification | Direction | mcp2cli |
|---|---|---|
| `notifications/tools/list_changed` | server → client | **Supported** — writes stale marker |
| `notifications/resources/list_changed` | server → client | **Supported** — writes stale marker |
| `notifications/prompts/list_changed` | server → client | **Supported** — writes stale marker |

When a server publishes a list-changed notification, the handler writes a marker file alongside the discovery cache. The next `ls` (or any command that needs the cache) detects the marker and re-runs discovery before serving the command.

**Source.** [`src/mcp/handler.rs`](../src/mcp/handler.rs) — `handle_list_changed`.

### Resource updates

| Notification | Direction | mcp2cli |
|---|---|---|
| `notifications/resources/updated` | server → client | **Supported** |

Delivered for resources previously passed to `resources/subscribe`. Emitted as a `RuntimeEvent::ResourceUpdated` for every configured sink.

**Source.** [`src/mcp/handler.rs`](../src/mcp/handler.rs) — `handle_resource_updated`.

---

## Tasks (long-running operations)

| Method / field | Direction | mcp2cli |
|---|---|---|
| `_meta.task` on `tools/call` | client → server | **Supported** (via `--background`) |
| `tasks/get` | client → server | **Supported** |
| `tasks/result` | client → server | **Supported** |
| `tasks/cancel` | client → server | **Supported** |

MCP 2025-11-25 formalised long-running operations as **tasks**. Passing `--background` on an `invoke` call sets `_meta.task` so the server creates a task and returns a `task_id`; mcp2cli persists a [`JobRecord`](../src/runtime/state.rs) and returns control immediately. Later invocations use `jobs` to poll, wait, cancel, or stream updates — even across separate process invocations because the record lives on disk.

**CLI surface.**

```bash
mcp2cli invoke big-index-build --background
# returns: { "job_id": "...", "remote_task_id": "...", "status": "pending" }

mcp2cli jobs show <job-id>
mcp2cli jobs wait <job-id> --timeout 300
mcp2cli jobs watch <job-id>            # stream progress events
mcp2cli jobs cancel <job-id>
```

See [`docs/features/background-jobs.md`](features/background-jobs.md).

**Source.** Operation definitions: [`src/mcp/model.rs`](../src/mcp/model.rs) — `McpOperation::TaskGet`, `TaskResult`, `TaskCancel`. Request mapping + `_meta.task` injection: [`src/mcp/protocol.rs`](../src/mcp/protocol.rs). Job persistence: [`src/runtime/state.rs`](../src/runtime/state.rs).

---

## Transports

mcp2cli speaks four transports, all behind the single [`McpClient`](../src/mcp/client.rs) trait. Transport selection comes from the active config (`server.transport`) or CLI flags (`--url`, `--stdio`).

| Transport | Source | When to use |
|---|---|---|
| **Stdio** | [`StdioMcpClient`](../src/mcp/client.rs) | Local MCP server packaged as a command (`npx @modelcontextprotocol/server-everything`, Python packages, custom binaries) |
| **Streamable HTTP** | [`StreamableHttpMcpClient`](../src/mcp/client.rs) | Remote or local-networked servers. JSON-RPC POST + Server-Sent Events on the response body for streaming and server→client messages |
| **Daemon IPC** | [`DaemonMcpClient`](../src/mcp/client.rs) | Automatic when `mcp2cli daemon` is running for the active config — reuses a warm connection instead of paying init cost every call |
| **VSOCK / Unix shim** | [`vsock_shim`](../src/mcp/vsock_shim.rs) | `mcp-<server>-<tool>` symlinks dialing a host-side bridge; AF_VSOCK in production, AF_UNIX for dev/CI |

The demo backend (`--url demo.invalid/mcp`) is a file-backed client used for offline onboarding and tests; it is not a real MCP transport.

See [`docs/features/transports.md`](features/transports.md) and [`docs/features/daemon-mode.md`](features/daemon-mode.md) for operator docs.

---

## Known gaps

- **Pagination cursors.** Spec-defined `nextCursor` on `*/list` responses is not yet consumed — mcp2cli issues a single `list` request per primitive and treats the first page as the full inventory. Will matter for servers with very large tool/resource catalogs.
- **Authorization (OAuth 2.1) flows.** `auth login` supports bearer-token capture and the stored-token lifecycle; end-to-end OAuth authorization-code with PKCE is partial — see [`docs/features/authentication.md`](features/authentication.md) for the current matrix.
- **Multi-root `notifications/roots/list_changed` debouncing.** Clients may spam the server if root config is hot-reloaded in a tight loop; there is no built-in debounce window.

Found a gap not listed here? File an issue — the intent is to track spec coverage accurately.
