# Protocol Coverage

**mcp2cli** implements two revisions of the [Model Context Protocol](https://modelcontextprotocol.io/) end-to-end:

- **[MCP 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25)** — the *legacy* (session-oriented) revision: `initialize` handshake, `Mcp-Session-Id` on HTTP, server-initiated JSON-RPC requests.
- **[MCP 2026-07-28](https://modelcontextprotocol.io/specification/draft)** — the *modern* (stateless) revision introduced by the 2026-07-28 release candidate: no handshake, per-request `_meta` metadata, `server/discover`, Multi Round-Trip Requests (MRTR), `subscriptions/listen`, and tasks as an official extension.

This page is the canonical reference for **what is supported in each revision, how it surfaces on the CLI, and where the implementation lives in source**.

- [Protocol versions & negotiation](#protocol-versions--negotiation)
- [Lifecycle](#lifecycle)
- [Discovery](#discovery)
- [Tool invocation](#tool-invocation)
- [Resources](#resources)
- [Prompts](#prompts)
- [Completions](#completions)
- [Server-initiated interactions](#server-initiated-interactions)
  - [Multi Round-Trip Requests (2026-07-28)](#multi-round-trip-requests-2026-07-28)
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

## Protocol versions & negotiation

MCP 2026-07-28 removed the negotiation handshake: every request carries its
protocol version, client identity, and client capabilities in `_meta`
(`io.modelcontextprotocol/protocolVersion`, `…/clientInfo`,
`…/clientCapabilities`), and every modern server implements the
`server/discover` RPC ([SEP-2575](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2575)).

mcp2cli follows the spec's backward-compatibility algorithm out of the box
(`server.protocol_version: auto`, the default):

1. Before the first real operation, probe with `server/discover`
   (advertising `2026-07-28`).
2. A `DiscoverResult` → the server is **modern**: pick a mutually supported
   version, cache the advertised capabilities/identity, and speak
   statelessly from then on.
3. A recognised modern JSON-RPC error (`UnsupportedProtocolVersionError`
   `-32022`, `HeaderMismatch` `-32020`, `MissingRequiredClientCapability`
   `-32021`) → the server is modern but rejected the version: retry with a
   version from its advertised `supported` list, or report the mismatch.
4. Anything else — a plain `-32601`, an implementation-defined error, an
   HTTP `4xx` without a modern error body, or silence within the probe
   timeout (stdio) → the server is **legacy**: fall back to the
   `2025-11-25` `initialize` handshake.

Pin the behavior per config when you don't want auto-detection:

```yaml
server:
  protocol_version: "2026-07-28"   # modern only — never falls back
  # protocol_version: "2025-11-25" # legacy only — skips the probe
  # protocol_version: auto         # default
```

```bash
mcp2cli config init --name email --transport streamable-http \
  --endpoint https://mcp.example.com/email --protocol-version 2026-07-28
```

`doctor` and `inspect` report the negotiated revision and era:

```text
protocol: 2026-07-28 (stateless, per-request _meta)
protocol: 2025-11-25 (initialize handshake)
```

**Source.** [`src/mcp/protocol.rs`](../src/mcp/protocol.rs) — `VersionPolicy`, `ProtocolEra`, `probe_request`, `classify_probe_response`, `select_modern_version`, `inject_modern_meta`. Per-transport bootstrap: [`src/mcp/client.rs`](../src/mcp/client.rs) — `ensure_ready` on both `StdioMcpClient` and `StreamableHttpMcpClient`.

---

## Lifecycle

| Method / notification | Direction | 2025-11-25 | 2026-07-28 |
|---|---|---|---|
| `initialize` | client → server | **Supported** — sent on first request of every session | *Removed by spec* — replaced by per-request `_meta` |
| `notifications/initialized` | client → server | **Supported** | *Removed by spec* |
| `server/discover` | client → server | — | **Supported** — era probe, capability discovery, liveness |
| `ping` | bidirectional | **Supported** — `mcp2cli ping` | *Removed by spec* — `ping` transparently maps to `server/discover` |

**What mcp2cli does.** Against legacy servers, every transport runs the `initialize` handshake through [`ProtocolEngine`](../src/mcp/protocol.rs) and advertises client capabilities (elicitation, sampling, roots). Against modern servers, the `server/discover` probe replaces the handshake at the same round-trip cost, and each request carries the required `_meta` fields — including `extensions: { "io.modelcontextprotocol/tasks": {} }` so servers may return task handles. Negotiated capabilities are cached in the state store and inspected by `doctor` / `inspect`.

**CLI surface.**

```bash
email ping                # legacy: MCP ping; modern: server/discover round-trip
email doctor              # health + negotiated protocol revision and era
email inspect             # full capability dump for the active config
```

**Source.** [`src/mcp/protocol.rs`](../src/mcp/protocol.rs) — `initialize_request`, `complete_initialize`, `probe_request`, `complete_discover`.

---

## Discovery

| Method / notification | Direction | 2025-11-25 | 2026-07-28 |
|---|---|---|---|
| `tools/list` | client → server | **Supported** | **Supported** |
| `resources/list` | client → server | **Supported** | **Supported** |
| `resources/templates/list` | client → server | **Supported** | **Supported** |
| `prompts/list` | client → server | **Supported** | **Supported** |
| `notifications/*/list_changed` | server → client | **Supported** — invalidates cache | Delivered via [`subscriptions/listen`](#resource-updates) only |
| `ttlMs` / `cacheScope` on list results | server → client | — | **Passed through** — surfaced in `--json` output |

**What mcp2cli does.** `ls` issues the four `*/list` requests and persists the merged inventory to the state store as a [`DiscoveryInventoryView`]. Subsequent commands read from the cache rather than re-querying on every invocation. On legacy servers, inbound `list_changed` notifications write a stale marker file; the next `ls` refreshes the cache. Modern servers attach `ttlMs`/`cacheScope` freshness hints (SEP-2549), which mcp2cli passes through in structured output.

On MCP 2026-07-28 over Streamable HTTP, tools whose `x-mcp-header` annotations are invalid (SEP-2243) are **excluded from discovery** with a logged warning, as the spec requires.

The cache is what powers the **dynamic CLI** — [`apps::dynamic::build_dynamic_cli`](../src/apps/dynamic.rs) reads a [`CommandManifest`](../src/apps/manifest.rs) built from the inventory and materialises a `clap` tree where every tool / resource template / prompt becomes a subcommand with flags.

**CLI surface.**

```bash
email ls                         # populate / refresh the cache
email ls --tools                 # filter by primitive
```

**Source.** [`src/mcp/protocol.rs`](../src/mcp/protocol.rs) — `discover_method_name`. Cache: [`src/runtime/state.rs`](../src/runtime/state.rs). Header-annotation validation: [`src/mcp/client.rs`](../src/mcp/client.rs) — `reject_invalid_header_tools`.

---

## Tool invocation

| Method | Direction | 2025-11-25 | 2026-07-28 |
|---|---|---|---|
| `tools/call` | client → server | **Supported** — progress token + optional `_meta.task` | **Supported** — progress token; MRTR retries; task results via the tasks extension |
| `x-mcp-header` param mirroring (HTTP) | client → server | — | **Supported** — annotated params become `Mcp-Param-*` headers |

**What mcp2cli does.** Each discovered tool becomes a clap subcommand with flags derived from its JSON Schema input. Type coercion handles strings, numbers, booleans, enums, arrays, and nested objects; complex shapes fall back to a `--config <JSON>` escape hatch. `--background` elevates the call to a [task](#tasks-long-running-operations): on legacy servers via `_meta.task`, on modern servers by advertising the tasks extension and accepting a `resultType: "task"` handle.

Modern results carry a required `resultType` field; mcp2cli treats results without it as `"complete"` (as the spec requires of clients) and resolves `"input_required"` results through [MRTR](#multi-round-trip-requests-2026-07-28) automatically.

**CLI surface.**

```bash
# Dynamic (typed flags per tool):
email send --to user@example.com --body "Meeting at 3"
email search --query "metrics" --limit 10

# Static/protocol-shaped (any server, opaque args):
mcp2cli invoke send --arg to=user@example.com --arg body=@body.txt
mcp2cli invoke slow-job --background          # returns job id; see `jobs`
```

**Source.** Request mapping: [`src/mcp/protocol.rs`](../src/mcp/protocol.rs) — `map_operation_to_request` / `map_operation_to_modern_request`. Header mirroring: [`src/mcp/client.rs`](../src/mcp/client.rs) — `extract_param_headers`, `encode_header_value`.

---

## Resources

| Method / notification | Direction | 2025-11-25 | 2026-07-28 |
|---|---|---|---|
| `resources/read` | client → server | **Supported** | **Supported** (MRTR-capable) |
| `resources/subscribe` | client → server | **Supported** | *Removed by spec* — replaced by `subscriptions/listen` |
| `resources/unsubscribe` | client → server | **Supported** | *Removed by spec* — closing the listen stream unsubscribes |
| `subscriptions/listen` | client → server | — | **Supported** — see below |
| `notifications/resources/updated` | server → client | **Supported** | **Supported** — on the listen stream, tagged with `subscriptionId` |

**What mcp2cli does.** Concrete resources read with `get <URI>`; parameterised resource templates (e.g. `file:///{path}`) surface as typed commands whose flags fill the template parameters.

Subscriptions differ per revision:

- **2025-11-25** — `subscribe <URI>` sends `resources/subscribe`; updates flow through the event broker to stderr, webhooks, Unix sockets, or SSE endpoints, depending on [event sink configuration](features/event-system.md).
- **2026-07-28** — subscriptions last only while a `subscriptions/listen` stream stays open. `subscribe <URI>` opens the stream with `notifications.resourceSubscriptions: [<URI>]`, waits for `notifications/subscriptions/acknowledged`, reports it, and releases the stream (which is the protocol's cancellation signal). `unsubscribe` resolves locally — there is nothing to send.

**CLI surface.**

```bash
email get mail://inbox                          # read a concrete URI
email get "file:///{path}" --path docs/index.md # parameterised template
email subscribe mail://inbox                    # legacy: resources/subscribe; modern: listen + ack
email unsubscribe mail://inbox                  # modern: local no-op with explanation
```

**Source.** [`src/mcp/protocol.rs`](../src/mcp/protocol.rs) — subscribe mappings. Listen streams: [`src/mcp/client.rs`](../src/mcp/client.rs) — `modern_subscribe` on both transports. Update handling: [`src/mcp/handler.rs`](../src/mcp/handler.rs).

---

## Prompts

| Method | Direction | 2025-11-25 | 2026-07-28 |
|---|---|---|---|
| `prompts/get` | client → server | **Supported** | **Supported** (MRTR-capable) |

**What mcp2cli does.** Each prompt becomes a subcommand. Typed flags are derived from the prompt's declared arguments; dotted argument names (e.g. `context.thread_id`) nest under `--context-thread-id` by default, with overlay support for renaming.

**CLI surface.**

```bash
email prompt review-diff --diff-file hunk.patch
email prompt summarise --context-thread-id 123
```

**Source.** [`src/mcp/protocol.rs`](../src/mcp/protocol.rs) — `flatten_prompt_arguments`.

---

## Completions

| Method | Direction | 2025-11-25 | 2026-07-28 |
|---|---|---|---|
| `completion/complete` | client → server | **Supported** — with `ref.context` | **Supported** |

**What mcp2cli does.** `complete` asks the server for suggested values for a reference point (a resource URI template variable or a prompt argument). The `ref.context` object lets the server answer context-sensitively (for example, completing only files inside a previously chosen directory).

**CLI surface.**

```bash
mcp2cli complete \
  --ref-type prompt \
  --ref-name summarise \
  --arg-name context.thread_id \
  --value 12
```

**Source.** [`src/mcp/model.rs`](../src/mcp/model.rs) — `McpOperation::Complete`. [`src/mcp/protocol.rs`](../src/mcp/protocol.rs) — `completion_params`.

---

## Server-initiated interactions

MCP is bidirectional, but the mechanics changed between revisions:

- **2025-11-25** — servers send their own JSON-RPC requests
  (`elicitation/create`, `sampling/createMessage`, `roots/list`) on the
  active stream; the client answers inline.
- **2026-07-28** — servers **must not** send requests. They return an
  `InputRequiredResult` (`resultType: "input_required"`) whose
  `inputRequests` map embeds the same request shapes; the client gathers
  the answers and **retries the original request** with `inputResponses`.
  This is the Multi Round-Trip Requests pattern
  ([SEP-2322](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2322)).

Either way, the same terminal-first handlers answer the requests, so the
user experience is identical across revisions.

### Multi Round-Trip Requests (2026-07-28)

| Piece | mcp2cli |
|---|---|
| `resultType: "input_required"` on `tools/call` / `resources/read` / `prompts/get` | **Supported** — resolved automatically |
| `inputRequests` (elicitation / sampling / roots entries) | **Supported** — dispatched through the standard handlers |
| `requestState` opaque echo | **Supported** — echoed verbatim, never inspected; omitted when absent |
| Fresh JSON-RPC id per retry | **Supported** |
| Round-trip guard | Capped at 8 rounds per logical operation |

**Source.** [`src/mcp/protocol.rs`](../src/mcp/protocol.rs) — `parse_input_required`, `attach_input_responses`. [`src/mcp/handler.rs`](../src/mcp/handler.rs) — `fulfill_input_requests`. Driver: [`src/mcp/client.rs`](../src/mcp/client.rs) — `drive_modern_operation`.

### Elicitation

| Method | Direction | 2025-11-25 | 2026-07-28 |
|---|---|---|---|
| `elicitation/create` | server → client | **Supported** — form + URL modes | **Supported** — embedded in MRTR `inputRequests` |
| `notifications/elicitation/complete` | server → client | **Supported** | *Removed by spec* (the retry carries the outcome) |

Servers request structured input from the user mid-operation — e.g. a destructive action that wants confirmation or a missing parameter. mcp2cli renders:

- **Form mode** — a terminal prompt per field from the JSON Schema, with type validation.
- **URL mode** — prints a URL and waits for the user to complete the flow out-of-band, then continues.

See [`docs/features/elicitation-and-sampling.md`](features/elicitation-and-sampling.md).

**Source.** [`src/mcp/handler.rs`](../src/mcp/handler.rs) — `handle_elicitation_request`.

### Sampling

| Method | Direction | 2025-11-25 | 2026-07-28 |
|---|---|---|---|
| `sampling/createMessage` | server → client | **Supported** | **Supported** — embedded in MRTR `inputRequests`; feature deprecated by SEP-2577 but fully functional |

Servers ask the client to run an LLM completion on their behalf. mcp2cli always keeps the human in the loop: the inbound request is displayed with the tool context and pending message, and the user approves, edits, or rejects before the reply is forwarded.

> MCP 2026-07-28 deprecates the Sampling feature (12-month removal window,
> [SEP-2577](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2577)).
> mcp2cli keeps answering sampling requests for as long as servers send them.

See [`docs/features/elicitation-and-sampling.md`](features/elicitation-and-sampling.md).

**Source.** [`src/mcp/handler.rs`](../src/mcp/handler.rs) — `handle_sampling_request`.

### Roots

| Method / notification | Direction | 2025-11-25 | 2026-07-28 |
|---|---|---|---|
| `roots/list` | server → client | **Supported** | **Supported** — embedded in MRTR `inputRequests`; feature deprecated by SEP-2577 |
| `notifications/roots/list_changed` | client → server | **Supported** | *Removed by spec* |

The client advertises a list of filesystem or URI roots that scope where a server may read/write. Configure roots in the app config ([`roots` section](reference/config-reference.md)); they are returned on demand.

**Source.** [`src/mcp/handler.rs`](../src/mcp/handler.rs) — `handle_roots_list`.

---

## Notifications

### Progress

| Notification | Direction | 2025-11-25 | 2026-07-28 |
|---|---|---|---|
| `notifications/progress` | server → client | **Supported** | **Supported** — on the originating request's response stream |

Every long-running request (`tools/call`, `resources/read`, `prompts/get`, task operations) is stamped with a unique `_meta.progressToken`. Incoming progress notifications are correlated back to the operation and emitted as `RuntimeEvent::Progress` — rendered on stderr for human users, streamed as NDJSON to configured event sinks for programmatic consumers.

**Source.** Token injection: [`src/mcp/protocol.rs`](../src/mcp/protocol.rs) — `inject_progress_token`. Notification routing: [`src/mcp/handler.rs`](../src/mcp/handler.rs) — `handle_progress`.

### Logging

| Method / notification | Direction | 2025-11-25 | 2026-07-28 |
|---|---|---|---|
| `logging/setLevel` | client → server | **Supported** | *Removed by spec* — level travels per-request in `_meta` |
| `_meta["io.modelcontextprotocol/logLevel"]` | client → server | — | **Supported** — injected on every request once set |
| `notifications/message` | server → client | **Supported** | **Supported** — only for requests that asked for logs |

The `log <LEVEL>` command works in both revisions: on legacy servers it sends `logging/setLevel`; on modern servers it persists the level in the state store, and every subsequent request carries it in `_meta["io.modelcontextprotocol/logLevel"]`. Server logs are surfaced as `RuntimeEvent::ServerLog` on the configured sinks either way.

> MCP 2026-07-28 deprecates the Logging feature (SEP-2577); log to stderr
> (stdio) or OpenTelemetry per the suggested migration.

**CLI surface.**

```bash
email log debug     # ask the server for debug-level logs
email log warning   # back off
```

**Source.** [`src/mcp/protocol.rs`](../src/mcp/protocol.rs) — `modern_offline_result`, `META_LOG_LEVEL`. Persistence: [`src/runtime/state.rs`](../src/runtime/state.rs) — `set_server_log_level`.

### Cancellation

| Mechanism | Direction | 2025-11-25 | 2026-07-28 |
|---|---|---|---|
| `notifications/cancelled` | bidirectional | **Supported** | **Supported** on stdio (client → server only) |
| Closing the response stream (HTTP) | client → server | — | **Supported** — the transport-level disconnect is the cancel signal |

Pressing Ctrl+C during a pending request sends `notifications/cancelled` (stdio) or closes the response stream (modern HTTP). Incoming cancels are accepted and surfaced as a runtime event; no state is corrupted if a cancel arrives after the response.

**Source.** [`src/mcp/handler.rs`](../src/mcp/handler.rs), [`src/mcp/client.rs`](../src/mcp/client.rs) — `cancel_request`.

### List-changed

| Notification | Direction | 2025-11-25 | 2026-07-28 |
|---|---|---|---|
| `notifications/tools/list_changed` | server → client | **Supported** — writes stale marker | Opt-in via `subscriptions/listen` (`toolsListChanged`) |
| `notifications/resources/list_changed` | server → client | **Supported** — writes stale marker | Opt-in via `subscriptions/listen` (`resourcesListChanged`) |
| `notifications/prompts/list_changed` | server → client | **Supported** — writes stale marker | Opt-in via `subscriptions/listen` (`promptsListChanged`) |

On legacy servers these arrive on any active stream and invalidate the discovery cache. On modern servers they only flow on an open `subscriptions/listen` stream; one-shot CLI invocations rely on the `ttlMs` freshness hints and explicit `ls` refreshes instead.

**Source.** [`src/mcp/handler.rs`](../src/mcp/handler.rs) — `handle_list_changed`.

### Resource updates

| Notification | Direction | 2025-11-25 | 2026-07-28 |
|---|---|---|---|
| `notifications/resources/updated` | server → client | **Supported** | **Supported** — on the listen stream, tagged with `io.modelcontextprotocol/subscriptionId` |

Emitted as a `RuntimeEvent` for every configured sink. On stdio, modern notifications are demultiplexed by the `subscriptionId` `_meta` field, as the spec requires.

**Source.** [`src/mcp/handler.rs`](../src/mcp/handler.rs) — `handle_resource_updated`.

---

## Tasks (long-running operations)

| Method / field | Direction | 2025-11-25 (experimental core) | 2026-07-28 (`io.modelcontextprotocol/tasks` extension) |
|---|---|---|---|
| Opt-in | client → server | `_meta.task` per request (via `--background`) | Extension advertised in `clientCapabilities.extensions`; server decides per request |
| Task creation | server → client | `_meta.task` on the result | `CreateTaskResult` (`resultType: "task"`) — may arrive unsolicited |
| `tasks/get` | client → server | **Supported** | **Supported** — polled with the server's `pollIntervalMs`; terminal states carry `result`/`error` |
| `tasks/result` | client → server | **Supported** (blocking) | *Removed by spec* — mapped to `tasks/get` |
| `tasks/update` | client → server | — | **Supported** — answers `input_required` task states |
| `tasks/cancel` | client → server | **Supported** | **Supported** (cooperative) |
| `tasks/list` | client → server | not used | *Removed by spec* |

**What mcp2cli does.** Passing `--background` on an invocation returns immediately with a job id; mcp2cli persists a [`JobRecord`](../src/runtime/state.rs) and later invocations use `jobs` to poll, wait, cancel, or stream updates — even across separate process invocations because the record lives on disk.

On modern servers the extension is server-directed: if a server returns a task handle for a call that was **not** `--background`, mcp2cli polls it to completion transparently and returns the final result as if the call had been synchronous — including answering `input_required` states (elicitations embedded in the task) via `tasks/update`.

**CLI surface.**

```bash
email send --to team@example.com --background
# returns: { "job_id": "...", "remote_task_id": "...", "status": "pending" }

email jobs show <job-id>
email jobs wait <job-id> --timeout 300
email jobs watch <job-id>            # stream progress events
email jobs cancel <job-id>
```

See [`docs/features/background-jobs.md`](features/background-jobs.md).

**Source.** [`src/mcp/protocol.rs`](../src/mcp/protocol.rs) — `parse_modern_task`, `TASKS_EXTENSION_ID`. Polling driver: [`src/mcp/client.rs`](../src/mcp/client.rs) — `poll_modern_task`, `finish_modern_task`. Job persistence: [`src/runtime/state.rs`](../src/runtime/state.rs).

---

## Transports

mcp2cli speaks four transports, all behind the single [`McpClient`](../src/mcp/client.rs) trait. Transport selection comes from the active config (`server.transport`) or CLI flags (`--url`, `--stdio`).

| Transport | Source | When to use |
|---|---|---|
| **Stdio** | [`StdioMcpClient`](../src/mcp/client.rs) | Local MCP server packaged as a command (`npx @modelcontextprotocol/server-everything`, Python packages, custom binaries) |
| **Streamable HTTP** | [`StreamableHttpMcpClient`](../src/mcp/client.rs) | Remote or local-networked servers. JSON-RPC POST + Server-Sent Events on the response body |
| **Daemon IPC** | [`DaemonMcpClient`](../src/mcp/client.rs) | Automatic when `mcp2cli daemon` is running for the active config — reuses a warm connection instead of paying init cost every call |
| **VSOCK / Unix shim** | [`vsock_shim`](../src/mcp/vsock_shim.rs) | `mcp-<server>-<tool>` symlinks dialing a host-side bridge; AF_VSOCK in production, AF_UNIX for dev/CI |

Streamable HTTP behavior per revision:

| Aspect | 2025-11-25 | 2026-07-28 |
|---|---|---|
| Sessions | `Mcp-Session-Id` header honoured | *Removed* — never sent |
| Request metadata headers | `MCP-Protocol-Version` after init | `MCP-Protocol-Version`, `Mcp-Method`, `Mcp-Name` on every POST (base64 sentinel encoding for unsafe values) |
| Custom param headers | — | `x-mcp-header`-annotated tool params mirrored as `Mcp-Param-*`; invalid annotations exclude the tool from discovery |
| Server-initiated messages | JSON-RPC requests on SSE streams | *Removed* — MRTR interim results instead |
| Long-lived notifications | HTTP GET stream (not used by mcp2cli) | `subscriptions/listen` POST response stream |
| SSE resumability (`Last-Event-ID`) | Spec-optional (not used by mcp2cli) | *Removed* — broken streams are re-issued as new requests |
| Protocol errors | HTTP status only | HTTP `4xx` **with** JSON-RPC error bodies (`-32020`…`-32022`) surfaced as JSON-RPC errors |

The demo backend (`--url demo.invalid/mcp`) is a file-backed client used for offline onboarding and tests; it is not a real MCP transport.

See [`docs/features/transports.md`](features/transports.md) and [`docs/features/daemon-mode.md`](features/daemon-mode.md) for operator docs.

---

## Known gaps

- **Pagination cursors.** Spec-defined `nextCursor` on `*/list` responses is not yet consumed — mcp2cli issues a single `list` request per primitive and treats the first page as the full inventory. Will matter for servers with very large tool/resource catalogs.
- **Authorization (OAuth 2.1) flows.** `auth login` supports bearer-token capture plus authorization-code + PKCE with dynamic client registration for streamable-HTTP servers, including RFC 9207 `iss` validation on the callback. Remaining gaps include Client ID Metadata Documents, pre-registered client config, refresh-token rotation, and runtime step-up authorization — see [`docs/features/authentication.md`](features/authentication.md) for the current matrix.
- **Multi-root `notifications/roots/list_changed` debouncing.** Clients may spam the server if root config is hot-reloaded in a tight loop; there is no built-in debounce window.
- **Long-lived `subscriptions/listen` watching.** The one-shot CLI verifies subscriptions (open → ack → release); holding a listen stream open for continuous updates is a natural fit for `mcp2cli daemon` and is not implemented yet.
- **`ttlMs`-driven cache expiry.** Modern freshness hints are passed through in output but do not yet expire the discovery cache automatically.
- **Trace context propagation.** OpenTelemetry `traceparent`/`tracestate` `_meta` conventions (SEP-414) are not emitted.

Found a gap not listed here? File an issue — the intent is to track spec coverage accurately.
