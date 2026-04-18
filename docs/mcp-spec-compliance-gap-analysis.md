# MCP Specification Compliance Gap Analysis

> **Date:** 2026-03-28
> **Current mcp2cli protocol version:** `2025-03-26`
> **Latest MCP specification:** `2025-11-25` (via `2025-06-18` intermediate)
> **Method:** Full audit of mcp2cli codebase vs. normative MCP spec, all SEPs merged
> into `2025-06-18` and `2025-11-25`, and GitHub changelogs.

---

## Executive Summary

mcp2cli implements the `2025-03-26` spec with strong coverage of the core
features (tools, resources, prompts, completion, logging, progress, sampling,
elicitation). However, **two full spec revisions have shipped since** — introducing
structured output, resource links, tool icons, URL-mode elicitation, the task
system, tool-level execution metadata, and significant authorization changes.

This document catalogs every gap, ranked by impact, with normative references.

---

## What We Already Have (Compliant)

| Feature | Spec Section | Status |
|---------|-------------|--------|
| `tools/list`, `tools/call` | Server/Tools | ✅ Full |
| `resources/list`, `resources/read` | Server/Resources | ✅ Full |
| `resources/templates/list` | Server/Resources | ✅ Full |
| `prompts/list`, `prompts/get` | Server/Prompts | ✅ Full |
| `completion/complete` | Server/Utilities/Completion | ✅ Full |
| `logging/setLevel` | Server/Utilities/Logging | ✅ Full |
| `ping` | Base/Utilities/Ping | ✅ Full |
| `initialize` / `notifications/initialized` | Base/Lifecycle | ✅ Full |
| Progress notifications (receive) | Base/Utilities/Progress | ✅ Full |
| Server log notifications | Server/Utilities/Logging | ✅ Full |
| List changed notifications (tools/resources/prompts) | All | ✅ Full |
| Resource updated notification | Server/Resources | ✅ Receive |
| Elicitation (form mode, server→client) | Client/Elicitation | ✅ Full |
| Sampling (human-in-the-loop, server→client) | Client/Sampling | ✅ Full |
| capability negotiation | Base/Lifecycle | ✅ Core |
| Stdio transport | Base/Transports | ✅ Full |
| Streamable HTTP transport | Base/Transports | ✅ Full |

---

## Gap Inventory

### Tier 1: Protocol Version & Capability Gaps (Must Fix)

#### G-01: Protocol Version Outdated — `2025-03-26` → `2025-11-25`

**Impact:** HIGH — Servers may refuse handshake or withhold features when they
see an old protocol version. Version negotiation allows fallback, but we should
advertise the latest version we intend to support.

**Spec:** Lifecycle § Version Negotiation — "The client MUST send a protocol
version it supports. This SHOULD be the latest version supported by the client."

**Change:** Update `DEFAULT_MCP_PROTOCOL_VERSION` from `"2025-03-26"` to
`"2025-11-25"` in `protocol.rs:7`. This is a single constant change but it
commits us to handling all features that version implies.

**Required before bumping:**
- G-02 (structured content), G-03 (resource_link), G-04 (elicitation modes),
  G-06 (title field), G-07 (icons), G-12 (MCP-Protocol-Version header).

---

#### G-02: Structured Content / `outputSchema` Not Parsed

**Impact:** HIGH — Tools with `outputSchema` return data in `structuredContent`.
We detect its existence in `tool_call_summary()` but don't extract or render it.
For a CLI that needs to pipe and format server output, this is critical.

**Spec (2025-06-18):** SEP via PR #371. Tools § Structured Content — "Structured
content is returned as a JSON object in the `structuredContent` field."
Tools § Output Schema — "Clients SHOULD validate structured results against
this schema."

**What to build:**
1. Parse `outputSchema` from `tools/list` response and store it in discovery items.
2. When rendering tool call results, prefer `structuredContent` over text `content`
   when present — output as formatted JSON or table.
3. Validate `structuredContent` against `outputSchema` when available (SHOULD).
4. `--json` output mode should emit `structuredContent` verbatim.
5. Display `outputSchema` in `ls --detail` or `help <tool>`.

---

#### G-03: `resource_link` Content Type in Tool Results

**Impact:** HIGH — New content type `resource_link` appears in tool call results
alongside `text`, `image`, `audio`, `resource`. We don't recognize or render it.

**Spec (2025-06-18):** PR #603. Tools § Resource Links — "A tool MAY return links
to Resources... `type: resource_link`".

**What to build:**
1. In result rendering, detect `type: "resource_link"` content items.
2. Display link URI, name, description, mimeType.
3. Optionally auto-fetch the linked resource via `resources/read` with `--follow-links`.

---

#### G-04: URL Mode Elicitation

**Impact:** HIGH — The `2025-11-25` spec adds URL mode elicitation (`mode: "url"`)
for OAuth flows, sensitive data, payment processing. Our handler assumes form
mode only.

**Spec (2025-11-25):** SEP-1036. Elicitation § URL Mode — "URL mode elicitation
enables servers to direct users to external URLs... Clients MUST NOT
automatically pre-fetch the URL... MUST show the full URL to the user."

**Also new:** `elicitation` capability now has sub-fields `form` and `url`:
```json
{ "elicitation": { "form": {}, "url": {} } }
```

**What to build:**
1. Update `ClientCapabilities` to advertise `elicitation: { form: {}, url: {} }`.
2. In `handle_elicitation_request`, check `mode` field:
   - `"form"` (or absent): existing form handler.
   - `"url"`: display `message`, show `url` to user, offer to open in browser
     (or print for manual navigation), return `action: "accept"` when user
     confirms, `"decline"` or `"cancel"` otherwise.
3. Handle `notifications/elicitation/complete` notification from server.
4. Handle `URLElicitationRequiredError` (code `-32042`) on tool call responses —
   extract elicitations from error data, present URLs, retry after completion.

---

#### G-05: Elicitation Response Schema Changes (SEP-1330)

**Impact:** MEDIUM — The `2025-11-25` spec updated `ElicitResult` and enum schemas
to support titled and untitled single-select and multi-select enums. Our current
coercion handles `oneOf` with `const`+`title` and `anyOf` in items, which aligns
well — but the schema format was standardized more precisely.

**What to verify:**
- Confirm `coerce_elicitation_value` handles `oneOf` (single-select titled),
  `enum` (single-select untitled), `anyOf` in items (multi-select titled),
  `enum` in items (multi-select untitled). **Current code covers all four.**
- Add `default` value handling for all types (string, number, enum) per SEP-1034.
  **Current code handles `default_value` already.** ✅

**Status:** Likely compliant already — verify with test cases.

---

#### G-06: `title` Field on Tools, Resources, Prompts, Resource Templates

**Impact:** MEDIUM — The `2025-06-18` spec (PR #663) adds `title` as
human-friendly display name, with `name` reserved as programmatic identifier.

**Spec:** "Add `title` field for human-friendly display names, so that `name`
can be used as a programmatic identifier."

**What to build:**
1. Parse `title` from tool/resource/prompt/template list responses.
2. In `ls` output: show `title` if present, `name` as identifier.
3. In `help <tool>`: show both title and name.
4. In `--json` mode: include both fields.

---

#### G-07: Tool / Resource / Prompt Icons (SEP-973)

**Impact:** LOW for CLI — Icons are primarily visual. But for completeness:

**Spec (2025-11-25):** "Servers can expose icons as additional metadata for tools,
resources, resource templates, and prompts."

**What to build:**
1. Parse `icons` array from discovery responses.
2. In `--json` output: include icons.
3. In human output: optionally note `[has icon]` or show URL.
4. De-prioritize — CLI doesn't render images.

---

### Tier 2: New Spec Features (Should Implement)

#### G-08: Task System (Experimental — SEP-1686) — ✅ IMPLEMENTED

**Status:** Implemented in Phase 2. `TaskGet`/`TaskResult`/`TaskCancel` operations
wired end-to-end. `--background` injects `_meta.task` for task augmentation.
`jobs show/wait/cancel/watch` poll `tasks/get`/`tasks/result`/`tasks/cancel`.
`TaskAccepted` result type handles deferred task responses.
`notifications/tasks/status` handler processes server-pushed status updates.

**Impact:** HIGH for long-running tools — The `2025-11-25` spec adds `tasks/*`
methods for durable/async tool execution.

---

#### G-09: `notifications/cancelled` — Cancellation Protocol — ✅ IMPLEMENTED

**Status:** Implemented in Phase 2. Bidirectional — handler processes
`notifications/cancelled` from servers; `cancel_request()` trait method
implemented for Stdio and StreamableHTTP transports.

**Impact:** MEDIUM — Enables graceful abort for long-running tool calls.

---

#### G-10: Resource Subscriptions (`resources/subscribe`, `resources/unsubscribe`) — ✅ IMPLEMENTED

**Status:** Implemented in Phase 2. `SubscribeResource`/`UnsubscribeResource`
operations end-to-end with `subscribe <URI>`/`unsubscribe <URI>` CLI commands.
Demo, Stdio, and StreamableHTTP clients all support the operations.

**Impact:** MEDIUM — Enables real-time resource change notifications.

---

#### G-11: Roots Capability (Client→Server) — ✅ IMPLEMENTED

**Status:** Implemented in Phase 2. Client declares `roots` capability.
`RootEntry` struct and `roots/list` server→client handler wired into
`OperationMessageHandler`. Roots are configurable per-server in YAML config.

**Impact:** MEDIUM — Enables servers to understand client workspace context.

---

#### G-12: `MCP-Protocol-Version` HTTP Header

**Impact:** MEDIUM for HTTP transport — The `2025-06-18` spec (PR #548) requires
this header on requests after initialization.

**Spec:** Transports § Protocol Version Header — "Require negotiated protocol
version to be specified via `MCP-Protocol-Version` header in subsequent
requests when using HTTP."

**What to build:**
1. After handshake, store negotiated version.
2. Include `MCP-Protocol-Version: <version>` header on all subsequent HTTP
   requests in the streamable HTTP transport.

---

#### G-13: Sampling with Tool Calling (SEP-1577) — ✅ IMPLEMENTED

**Status:** Implemented in Phase 2. Handler displays `tools` and `toolChoice`
from `sampling/createMessage` requests so the human-in-the-loop user has
full context.

**Impact:** MEDIUM — Complete sampling experience for tool-aware servers.

---

#### G-14: Progress Token Sending — ✅ IMPLEMENTED

**Status:** Implemented in Phase 2. Automatic `_meta.progressToken` injection
for `tools/call`, `prompts/get`, `resources/read`, `tasks/get`, `tasks/result`.
Tokens are generated per-request using `mcp2cli-<id>` format.

**Impact:** LOW — Enables servers to send targeted progress notifications.

---

#### G-15: `context` Field in Completion Requests — ✅ IMPLEMENTED

**Status:** Implemented in Phase 2. `McpOperation::Complete` carries optional
`context` for previously-resolved argument values, included in
`completion/complete` requests when non-empty.

**Impact:** LOW — Improves server-side completion accuracy.

---

### Tier 3: Authorization & Security (Environment-Dependent)

#### G-16: OAuth 2.1 / RFC 9728 Authorization Flow

**Impact:** HIGH for HTTP servers — The `2025-06-18` and `2025-11-25` specs
overhaul authorization with OAuth 2.1, Protected Resource Metadata (RFC 9728),
PKCE, resource indicators (RFC 8707), and client ID metadata documents.

**What to build (big scope):**
1. Protected Resource Metadata discovery (`.well-known/oauth-protected-resource`).
2. Authorization Server Metadata discovery (RFC 8414 + OIDC Discovery).
3. OAuth 2.1 Authorization Code flow with PKCE (S256).
4. Resource parameter (RFC 8707) in auth and token requests.
5. Client ID Metadata Documents support (SEP-991).
6. Dynamic Client Registration fallback (RFC 7591).
7. Token storage, refresh, and retry on 401/403.
8. Step-up scope challenge handling (SEP-835).
9. Secure token storage (not in plaintext).

**Note:** This is a significant standalone workstream. Our current OAuth support
(`auth` module) handles basic browser-based flows — it needs to be upgraded to
match the new MCP authorization spec.

---

#### G-17: `Mcp-Session-Id` Header Handling

**Impact:** MEDIUM for HTTP — Streamable HTTP transport sessions use this header.

**Spec:** Transports § Session Management — "If an `Mcp-Session-Id` is returned
by the server during initialization, clients MUST include it in all subsequent
HTTP requests."

**What to build:**
1. Capture `Mcp-Session-Id` from init response header.
2. Include on all subsequent requests.
3. Handle 404 response (session expired) — re-initialize.
4. Send HTTP DELETE on graceful shutdown.

---

### Tier 4: Display & Rendering Improvements

#### G-18: Rich Content Type Rendering

**Impact:** MEDIUM — Tool results can contain `image`, `audio`, and `resource`
content types. We extract text but don't render or save other types.

**What to build:**
1. `type: "image"` — decode base64, save to temp file, print path.
   With `--open`, launch viewer.
2. `type: "audio"` — decode base64, save to temp file, print path.
3. `type: "resource"` — show URI, mimeType, text or blob info.
4. `type: "resource_link"` — show URI, name, mimeType (see G-03).

---

#### G-19: Annotations Display for Audience/Priority

**Impact:** LOW — Tool results, resources, and prompts can have `annotations`
with `audience` (["user", "assistant"]), `priority` (0.0-1.0), `lastModified`.

**What to build:**
1. Parse annotations on content items.
2. In verbose mode, show audience and priority.
3. Filter content by `audience: ["user"]` when in user-facing mode.

---

#### G-20: Tool Annotation Semantics for CLI UX

**Impact:** LOW — Tool annotations (`readOnlyHint`, `destructiveHint`,
`idempotentHint`, `openWorldHint`) inform UI decisions.

**What to build:**
1. Show `[read-only]`, `[destructive]`, etc. tags in tool listing.
2. For `destructiveHint: true` tools, prompt confirmation before execution.
3. For `idempotentHint: true` tools, allow automatic retry on timeout.

---

### Tier 5: Transport & Wire Protocol

#### G-21: JSON-RPC Batching

**Impact:** LOW — The `2025-06-18` spec removed batching (PR #416) then
`2025-11-25` re-added it. Current spec says messages "MAY be batched."

**Status:** Not needed for basic compliance. Consider later for performance.

---

#### G-22: SSE Stream Resumability (`Last-Event-ID`)

**Impact:** LOW — HTTP transport may support `Last-Event-ID` for resume.

**Current:** Not implemented. Would improve robustness for long-lived HTTP
connections.

---

#### G-23: `_meta` Field on All Messages

**Impact:** LOW — The `2025-06-18` spec (PR #710) adds `_meta` to more message
types and defines formal usage.

**What to build:**
1. Include `_meta` with `progressToken` on requests (see G-14).
2. Preserve and pass through `_meta` on responses.
3. For task system (G-08), include `io.modelcontextprotocol/related-task` in
   `_meta`.

---

## Implementation Roadmap

### Phase 1: Core Spec Compliance (bump to 2025-11-25)
Priority | Gap | Effort
---------|-----|-------
P0 | G-02: Structured content + outputSchema | Medium
P0 | G-03: `resource_link` content type | Small
P0 | G-04: URL mode elicitation | Medium
P0 | G-06: `title` field parsing | Small
P0 | G-12: `MCP-Protocol-Version` header | Small
P0 | G-01: Bump version to `2025-11-25` | Trivial (after above)

### Phase 2: Feature Parity — ✅ COMPLETE
Priority | Gap | Effort | Status
---------|-----|--------|-------
P1 | G-08: Task system (experimental) | Large | ✅ Done
P1 | G-09: Cancellation notifications | Medium | ✅ Done
P1 | G-10: Resource subscriptions | Medium | ✅ Done
P1 | G-11: Roots capability | Small | ✅ Done
P1 | G-13: Sampling with tools | Small | ✅ Done
P1 | G-14: Progress token sending | Small | ✅ Done
P1 | G-15: Completion context | Small | ✅ Done

### Phase 3: Auth & Security
Priority | Gap | Effort
---------|-----|-------
P2 | G-16: OAuth 2.1 / RFC 9728 | Large
P2 | G-17: Session ID handling | Small

### Phase 4: Polish
Priority | Gap | Effort
---------|-----|-------
P3 | G-07: Icons metadata | Trivial
P3 | G-18: Rich content rendering | Medium
P3 | G-19: Content annotations | Small
P3 | G-20: Tool annotation UX | Small
P3 | G-21: JSON-RPC batching | Medium
P3 | G-22: SSE resumability | Medium
P3 | G-23: `_meta` field | Small

---

## SEP Reference Index

| SEP | Title | Status | Our Gap |
|-----|-------|--------|---------|
| SEP-973 | Icons for tools/resources/prompts | Merged → 2025-11-25 | G-07 |
| SEP-986 | Tool name guidance | Merged → 2025-11-25 | Compliant |
| SEP-991 | OAuth Client ID Metadata Documents | Merged → 2025-11-25 | G-16 |
| SEP-1034 | Default values in elicitation schemas | Merged → 2025-11-25 | Compliant |
| SEP-1036 | URL mode elicitation | Merged → 2025-11-25 | G-04 |
| SEP-1303 | Input validation → Tool Execution Error | Merged → 2025-11-25 | Compliant |
| SEP-1319 | Decouple request payload schemas | Merged → 2025-11-25 | N/A (schema only) |
| SEP-1330 | ElicitResult + enum schema updates | Merged → 2025-11-25 | G-05 (verify) |
| SEP-1577 | Tool calling in sampling | Merged → 2025-11-25 | ✅ G-13 |
| SEP-1613 | JSON Schema 2020-12 default dialect | Merged → 2025-11-25 | N/A |
| SEP-1686 | Tasks (experimental) | Merged → 2025-11-25 | ✅ G-08 |
| SEP-1699 | SSE polling streams | Merged → 2025-11-25 | G-22 |
| SEP-1730 | SDK tiering | Merged → 2025-11-25 | N/A (governance) |
| SEP-835 | Incremental scope consent | Merged → 2025-11-25 | G-16 |
| SEP-932 | Governance formalization | Merged → 2025-11-25 | N/A |
| SEP-985 | OAuth PRM discovery alignment | Merged → 2025-11-25 | G-16 |
| SEP-994 | Communication practices | Merged → 2025-11-25 | N/A |
| SEP-1302 | Working groups | Merged → 2025-11-25 | N/A |
| PR #371 | Structured tool output | Merged → 2025-06-18 | G-02 |
| PR #382 | Elicitation (form mode) | Merged → 2025-06-18 | Compliant |
| PR #416 | Remove JSON-RPC batching | Merged → 2025-06-18 | N/A |
| PR #548 | MCP-Protocol-Version header | Merged → 2025-06-18 | G-12 |
| PR #598 | Completion context field | Merged → 2025-06-18 | ✅ G-15 |
| PR #603 | resource_link content type | Merged → 2025-06-18 | G-03 |
| PR #663 | title field | Merged → 2025-06-18 | G-06 |
| PR #710 | _meta on all interfaces | Merged → 2025-06-18 | ✅ G-14/G-23 |

---

## Key Nuances Discovered

1. **Version negotiation is flexible but not optional.** If we advertise
   `2025-11-25` and the server only supports `2025-03-26`, the server will
   respond with `2025-03-26` and we MUST fall back. Our code should handle both
   paths cleanly — this means feature detection based on negotiated version, not
   just our advertised version.

2. **Tasks are experimental but already normative.** SEP-1686 is tagged
   experimental but is part of the canonical `2025-11-25` spec. Servers like
   long-running batch processors may REQUIRE task augmentation
   (`execution.taskSupport: "required"`), meaning we'd fail to call those tools
   without task support.

3. **URL mode elicitation is a security boundary.** The spec is explicit:
   "MUST NOT automatically pre-fetch the URL", "MUST NOT open the URL without
   explicit consent", "MUST show the full URL to the user." For a CLI, this
   means printing the URL and waiting for user confirmation, then optionally
   launching a browser.

4. **`structuredContent` is backwards-compatible by design.** Servers that return
   structured content SHOULD also include the serialized JSON in a TextContent
   block. This means our current code already works — but we lose the ability to
   do typed extraction, validation, and structured `--json` output.

5. **Resource links are NOT in `resources/list`.** "Resource links returned by
   tools are not guaranteed to appear in the results of a `resources/list`
   request." They're ephemeral references that might only exist in tool results.

6. **The authorization spec is enormous.** OAuth 2.1 + RFC 9728 + RFC 8707 +
   Client ID Metadata Documents + Dynamic Client Registration + PKCE + scope
   challenges. This is easily the largest single workstream. For stdio-only
   usage this is irrelevant; for HTTP servers it's mandatory.

7. **Progress tokens are bidirectional.** The client sends a `progressToken`, the
   server echoes it back in `notifications/progress`. Without sending one, the
   server MAY still send progress updates (and many do), but associating them
   with specific requests is not possible.

8. **The spec removed and re-added JSON-RPC batching.** `2025-06-18` removed it,
   `2025-11-25` allows it again. Batching is optional and not required for
   compliance.

9. **`Mcp-Session-Id` is only for Streamable HTTP.** Stdio sessions are
   inherently stateful (one process = one session). But HTTP sessions can expire
   server-side, requiring re-initialization.

10. **Tool annotations are untrusted.** "Clients MUST consider tool annotations
    to be untrusted unless they come from trusted servers." For a CLI, this means
    we should display them but not make security decisions based on them without
    user configuration marking a server as trusted.
