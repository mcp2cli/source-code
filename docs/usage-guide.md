# mcp2cli Usage Guide

Complete reference for using mcp2cli with MCP servers — covering every
command, feature, and best practice.

---

## Table of Contents

1. [Core Concepts](#core-concepts)
2. [Setup and Configuration](#setup-and-configuration)
3. [Discovery and Listing](#discovery-and-listing)
4. [Tool Commands](#tool-commands)
5. [Resource Commands](#resource-commands)
6. [Prompt Commands](#prompt-commands)
7. [Arguments and Payloads](#arguments-and-payloads)
8. [Authentication](#authentication)
9. [Elicitation (Server → Client)](#elicitation)
10. [Background Jobs](#background-jobs)
11. [Ping, Logging, and Completions](#ping-logging-and-completions)
12. [Health and Diagnostics](#health-and-diagnostics)
13. [Event Delivery](#event-delivery)
14. [Alias Workflow](#alias-workflow)
15. [Profile Overlays](#profile-overlays)
16. [Output Formats](#output-formats)
17. [Session and Capability Negotiation](#session-and-capability-negotiation)
18. [Transports](#transports)
19. [Best Practices](#best-practices)
20. [Complete Command Reference](#complete-command-reference)

---

## Core Concepts

mcp2cli is a **bridge CLI runtime** that connects to MCP servers and exposes
their capabilities as familiar terminal commands. Server tools become verbs,
resources become nouns, prompts become workflows — no MCP protocol jargon in
the public UX.

**Key ideas:**

- **Discovery-driven commands** — the server's capabilities ARE the command
  tree. Tool `inputSchema` and prompt `arguments` generate real typed
  `--flags`, not generic key=value pairs.
- **Named configs** bind to MCP servers. Each config is a YAML file that
  specifies a transport (stdio or streamable HTTP), an endpoint, and runtime
  preferences.
- **Invocation-name dispatch** lets you create symlink aliases (`email`,
  `github`, `staging`) that automatically select the matching config.
- **Active config** (`mcp2cli use <name>`) sets a default so you can run
  commands directly without a name prefix.
- **Profile overlays** let you rename, hide, group, and alias commands to
  make the CLI feel hand-crafted for a specific server.
- **Cached discovery** remembers server capabilities between invocations for
  fast startup, offline listing, and typed command generation.
- **Event delivery** routes runtime events (progress, job updates, auth
  prompts) to configurable sinks — stderr, HTTP webhooks, Unix sockets, or
  SSE streams.

---

## Setup and Configuration

### Creating a config for a stdio server

```bash
mcp2cli config init \
  --name local \
  --app bridge \
  --transport stdio \
  --stdio-command npx \
  --stdio-arg '@modelcontextprotocol/server-everything'
```

This creates `configs/local.yaml` in the platform config directory with a
stdio transport that spawns `npx @modelcontextprotocol/server-everything` as a
subprocess.

### Creating a config for a streamable HTTP server

```bash
mcp2cli config init \
  --name email \
  --app bridge \
  --transport streamable_http \
  --endpoint https://mcp.example.com/email
```

### Listing configs

```bash
mcp2cli config list
```

### Showing a specific config

```bash
mcp2cli config show --name email
```

### Setting the active config

```bash
mcp2cli use email          # Set active config
mcp2cli use --show         # Show current active config
mcp2cli use --clear        # Clear active config
```

When an active config is set, commands can be invoked directly:

```bash
mcp2cli send --to user@example.com --subject "Hi" --body hello
```

### Config YAML structure

```yaml
schema_version: 1

app:
  profile: bridge

server:
  display_name: My MCP Server
  transport: stdio                 # or streamable_http
  endpoint: null                   # Required for streamable_http
  stdio:
    command: npx
    args:
      - '@modelcontextprotocol/server-everything'
    cwd: null                      # Optional working directory
    env: {}                        # Optional environment variables

defaults:
  output: human                    # human | json | ndjson

logging:
  level: warn                      # trace | debug | info | warn | error
  format: pretty                   # pretty | json
  outputs:
    - kind: stderr

auth:
  browser_open_command: null       # Optional command for OAuth flows

events:
  enable_stdio_events: true
  # http_endpoint: "http://127.0.0.1:9090/events"
  # local_socket_path: "/tmp/mcp2cli-events.sock"
  # sse_endpoint: "127.0.0.1:9091"

# Optional: customize the CLI surface
# profile:
#   display_name: "My Custom Tool"
#   aliases: {}
#   hide: []
#   groups: {}
#   flags: {}
#   resource_verb: get
```

### Environment overrides

Any config field can be overridden with a `MCP2CLI_` environment variable:

```bash
MCP2CLI_LOGGING__LEVEL=debug email search --query test
```

The config and data directories themselves can be overridden:

```bash
MCP2CLI_CONFIG_DIR=/custom/config MCP2CLI_DATA_DIR=/custom/data mcp2cli ...
```

---

## Discovery and Listing

Discovery introspects the MCP server's capabilities. The `ls` command is the
primary listing interface:

### List all capabilities

```bash
email ls
```

### Filter by kind

```bash
email ls --tools            # Tools only
email ls --resources        # Resources only
email ls --prompts          # Prompts only
```

### Filter by name/description

```bash
email ls --filter send
email ls --filter draft --tools
```

### Pagination

```bash
email ls --limit 5
email ls --cursor "next-page-token"
email ls --all                      # Show everything
```

### JSON output

```bash
email --json ls
```

Returns structured JSON with the full discovery payload:

```json
{
  "app_id": "email",
  "command": "discover",
  "summary": "discovered 14 capabilities",
  "lines": ["..."],
  "data": {
    "category": "capabilities",
    "items": [
      { "id": "send", "kind": "tool", "title": "Send Email", "description": "..." }
    ]
  }
}
```

### Discovery cache and fallback

Discovery results are cached per config. If a live discovery call fails (e.g.
the server is down), mcp2cli falls back to the cached inventory with an
explicit stale-data warning:

```text
[email] live discovery failed; returned cached inventory instead
```

This means you can still list capabilities using the last known server state,
even when disconnected.

### How discovery maps to MCP protocol

| Action | MCP Method |
|--------|-----------|
| List tools | `tools/list` |
| List resources | `resources/list` |
| List prompts | `prompts/list` |

The runtime sends `initialize` + `notifications/initialized` before the first
discovery call in a session, negotiating protocol version and capabilities.

---

## Tool Commands

Tools are the primary action surface. Each server tool becomes a top-level CLI
command with typed flags derived from its `inputSchema`.

### Basic invocation

```bash
email send --to user@example.com --subject "Hi" --body "Hello"
```

Output:

```yaml
title: Send Email
description: Sends an email message
sent: user@example.com
```

### Multiple arguments

```bash
email reply --thread-id 123 --body "Thanks"
```

### Enum arguments

```bash
email send --to user@example.com --subject "Status" --priority high
```

The `--priority` flag only accepts values from the schema's enum:
`high`, `normal`, `low`.

### Viewing tool help

```bash
email send --help
```

Output:

```yaml
Sends an email message

USAGE: email send --to <TO> --subject <SUBJECT> --body <BODY>

OPTIONS:
  --to <TO>              Recipient address [required]
  --subject <SUBJECT>    Message subject [required]
  --body <BODY>          Message body [required]
  --json                 Output in JSON format
  --background           Run as background job
  -h, --help             Print help
```

### Namespace grouping (dotted names)

Dotted tool names become nested subcommands:

```bash
# Server exposes: draft.create, draft.list, labels.add, labels.list
email draft create --subject "Draft"
email draft list
email labels add --thread-id 123 --label important
email labels list
```

### Background invocation

```bash
email send --to list@example.com --subject "Newsletter" --body "..." --background
```

This creates a local job record that tracks the remote task. See
[Background Jobs](#background-jobs).

### JSON output

```bash
email --json send --to user@example.com --subject "Hi" --body "Hello"
```

### How tool invocation maps to MCP protocol

The command sends a `tools/call` JSON-RPC request with the tool name and
arguments (constructed from the typed flags). Content blocks in the response
are rendered as human-readable lines. If the server returns `isError: true`,
the output indicates the error.

---

## Resource Commands

Resources are named content items exposed by the server — files, documents,
data blobs.

### Read a concrete resource

```bash
email get "mail://inbox"
```

Output:

```yaml
uri: mail://inbox
mime_type: text/markdown
content:
  # Inbox – 12 unread messages
  ...
```

The `get` verb is the default resource command. It can be customized to `fetch`
or `read` via a profile overlay.

### Resource templates as commands

Parameterized resources (URI templates) become their own commands with typed
flags:

```bash
# Single-parameter template: mail://thread/{id} → positional argument
email get "mail://thread/123"

# Multi-parameter template: mail://search?q={query}&folder={folder} → flags
email mail-search --query invoice --folder inbox
```

### JSON output

```bash
email --json get "mail://thread/123"
```

### How resource reading maps to MCP protocol

| Action | MCP Method |
|--------|----------|
| Read a resource | `resources/read` |

The response includes `contents[]` with `uri`, `mimeType`, and either `text`
or `blob` data.

---

## Prompt Commands

Prompts are guided templates defined by the server that produce structured
messages. Each prompt becomes a top-level command with typed flags from its
`arguments` metadata.

### Run a simple prompt

```bash
email simple-prompt
```

Output:

```yaml
prompt: simple-prompt
output:
  This is a simple prompt without arguments.
```

### Run a prompt with arguments

```bash
email complex-prompt --temperature 0.7 --style concise
```

### JSON output

```bash
email --json simple-prompt
```

### How prompts map to MCP protocol

| Action | MCP Method |
|--------|-----------|
| Run a prompt | `prompts/get` |

The response `messages[]` are rendered as text. Both array-shaped and
object-shaped content blocks are supported.

---

## Arguments and Payloads

Commands accept typed flags generated from server schemas. For complex
payloads, additional argument mechanisms are available:

### Typed flags (primary)

Flags are generated from tool `inputSchema` and prompt `arguments`:

```bash
email send --to user@example.com --subject "Hi" --body "Hello"
```

Required flags are enforced. Optional flags use defaults from the schema.

### JSON-typed flags

For values that must be a specific JSON type (arrays, objects):

```bash
email send --to user@example.com --cc '["a@example.com","b@example.com"]' --headers '{"X-Priority":"1"}'
```

### Bulk JSON payloads

```bash
# From a JSON string
email send --args-json '{"to":"user@example.com","subject":"Hi","body":"Hello"}'

# From a file
email send --args-file ./payload.json
```

### Merge precedence

When multiple argument sources are used, they merge in this order (later wins
at the leaf level):

1. `--args-file` (base layer)
2. `--args-json` (overlays file)
3. Typed flags (final override)

```bash
email send \
  --args-file ./base.json \
  --args-json '{"headers":{"X-Priority":"1"}}' \
  --subject "Override subject"
```

---

## Authentication

mcp2cli provides a unified auth command surface for managing server
credentials.

### Login

```bash
email auth login
```

For **real servers** (non-demo), this prompts for a bearer token via stdin:

```text
enter bearer token for email: <paste token>
```

The token is persisted in a file-backed token store at
`instances/<config>/tokens.json`.

For **demo servers** (demo.invalid endpoint), a simulated browser-based OAuth
flow is used.

### Check auth status

```bash
email auth status
```

Shows the current authentication state:

```yaml
auth: authenticated
account: bearer-token (stored)
```

### Logout

```bash
email auth logout
```

Clears the stored token and resets auth session state.

### JSON output

```bash
email --json auth status
```

### Token persistence

Tokens are stored per-config in the data directory:

```text
data/instances/<config-name>/tokens.json
```

Structure:

```json
{
  "bearer_token": "your-token-here",
  "account": "bearer-token",
  "updated_at": "2026-03-27T12:00:00Z"
}
```

---

## Elicitation

Elicitation is a **server-initiated** flow where the MCP server requests
structured input from the user during a command. This is an advanced MCP
protocol feature.

### How it works

1. You run a command that triggers an elicitation:

   ```bash
   email trigger-elicitation-request
   ```

2. The server sends an `elicitation/create` JSON-RPC request to the client
   with a message and a JSON Schema describing the requested fields.

3. mcp2cli prompts each field interactively on the terminal (stderr for
   prompts, stdin for input):

   ```
   --- elicitation request ---
   Please provide additional information:
     Name (Your full name) [required]: John Doe
     Age [required] [default: 25]: 30
     Role [options: admin, user, guest]: admin
   --- end elicitation ---
   ```

4. The response is sent back to the server as a JSON-RPC response with
   `action: "accept"` and the collected content.

### Type coercion

mcp2cli coerces input values based on the JSON Schema property type:

| Schema Type | Coercion |
|------------|----------|
| `boolean` | `true`/`yes`/`y`/`1` → `true`; anything else → `false` |
| `integer` | Parsed as `i64`; falls back to string |
| `number` | Parsed as `f64`; falls back to string |
| `array` | Comma-separated values, split and trimmed |
| `string` | Used as-is |

### Enum matching

For fields with `enum` or `anyOf`/`oneOf` options, mcp2cli:

- Shows the available options in the prompt
- Matches input by title (case-insensitive)
- Falls back to `const` values from the schema

### Default values

If a field has a `default` value in the schema and the user provides empty
input, the default is used automatically.

### Schema support

The elicitation schema follows JSON Schema Draft 7+ conventions:

- `properties` — field definitions
- `required` — mandatory fields
- `title` / `description` — shown in the prompt
- `default` — pre-filled on empty input
- `enum` — option list
- `oneOf` / `anyOf` — title/const matching for rich enums

### Client capability advertisement

mcp2cli advertises `elicitation` capability during the `initialize` handshake
so servers know the client supports interactive input:

```json
{
  "capabilities": {
    "elicitation": {}
  }
}
```

### Unknown server→client requests

If the server sends a request method that mcp2cli doesn't recognize, it
responds with a standard JSON-RPC `-32601 method not found` error to prevent
protocol hangs.

---

## Background Jobs

When an MCP server supports task-oriented execution, mcp2cli can track
long-running operations as background jobs.

### Start a background job

```bash
email send --to list@example.com --subject "Newsletter" --body "..." --background
```

If the server returns a task ID, a local `JobRecord` is created.

### List all jobs

```bash
email jobs list
```

Output:

```text
abc-123  running  invoke
def-456  completed  invoke
```

### Show job details

```bash
email jobs show abc-123
email jobs show --latest
email jobs show --latest --command invoke
```

### Wait for a job to complete

```bash
email jobs wait abc-123
email jobs wait --latest
```

Synchronously waits for the remote task to reach a terminal state.

### Cancel a job

```bash
email jobs cancel abc-123
email jobs cancel --latest
```

### Watch a job

```bash
email jobs watch abc-123
email jobs watch --latest --command invoke
```

Watch emits runtime events as the job progresses and completes.

### Job lifecycle

```text
--background → [queued] → [running] → [completed | canceled | failed]
```

| State | Meaning |
|-------|---------|
| `queued` | Accepted by the server, not yet started |
| `running` | Actively executing |
| `completed` | Finished successfully, result available |
| `canceled` | Canceled by the user or server |
| `failed` | Failed, failure reason available |

### Job persistence

Jobs are persisted in the data directory and survive across CLI invocations.
Results and failure reasons are stored when available.

---

## Ping, Logging, and Completions

These runtime commands provide protocol-level control over the MCP session.

### Ping

```bash
email ping
```

Sends an MCP `ping` request to check server liveness and measure round-trip
latency. Useful for connectivity checks in scripts:

```bash
email ping && echo "Server is alive"
```

### Server-side logging level

```bash
email log debug       # Set server log level to debug
email log info
email log warn
email log error
email log trace       # Maximum verbosity
```

Sends a `logging/setLevel` notification to the server. This controls what the
*server* logs, not mcp2cli's own log level (use `MCP2CLI_LOGGING__LEVEL` for
that).

### Completions

```bash
email complete ref/tool send to "use"
```

Sends a `completion/complete` request to the server, asking for suggested
values for the given argument.

Arguments:
1. `ref_kind` — reference type: `ref/tool` or `ref/prompt`
2. `name` — the tool or prompt name
3. `argument` — the argument name to complete
4. `value` (optional) — partial value to complete from

---

## Health and Diagnostics

### Doctor

```bash
email doctor
```

Shows a comprehensive health summary:

```yaml
config: email
profile: bridge
transport: streamable_http
server: mcp-servers/email 2.0.0
auth: unauthenticated
negotiated: protocol 2025-03-26 with 5 capability groups cached
inventory: 14 tools, 3 resources, 2 prompts cached
```

Reports:
- Config name and adapter profile
- Transport type
- Server name and version
- Auth state
- Cached negotiated capabilities
- Cached discovery inventory counts

### Inspect

```bash
email inspect
```

Shows the full server capability response: protocol version, capabilities
(tools, resources, prompts, logging, completions, experimental), server info
(name, version), and session metadata.

### JSON output

```bash
email --json doctor
```

---

## Event Delivery

mcp2cli emits structured events during command execution covering progress,
server log messages, job state changes, auth prompts, capability change
signals, and informational messages.

### Event types

| Event | Fields | When |
|-------|--------|------|
| `info` | `app_id`, `message` | General status messages |
| `progress` | `app_id`, `operation`, `current`, `total`, `message` | During long-running operations, server `notifications/progress` |
| `server_log` | `app_id`, `level`, `logger`, `message` | Server `notifications/message` log messages |
| `job_update` | `app_id`, `job_id`, `status`, `message` | When job state changes |
| `auth_prompt` | `app_id`, `message` | During auth login flows |
| `list_changed` | `app_id`, `kind`, `message` | Server `notifications/{tools,resources,prompts}/list_changed` |

### Delivery sinks

Events are delivered to configured sinks. Multiple sinks can be active
simultaneously.

#### Stderr (default)

```yaml
events:
  enable_stdio_events: true
```

Human-readable one-line events on stderr:

```json
[email] invoking capability send
[email] job abc-123 running watch started
[email] server info (db): Connection pool created
[email] analyze-1 3/5 Computing aggregates...
```

#### HTTP webhook

```yaml
events:
  http_endpoint: "http://127.0.0.1:9090/events"
```

POSTs each event as a JSON body to the endpoint. Fire-and-forget,
non-blocking.

#### Unix domain socket

```yaml
events:
  local_socket_path: "/tmp/mcp2cli-events.sock"
```

Writes newline-delimited JSON (NDJSON) to the socket. Connect-per-event.

#### SSE server

```yaml
events:
  sse_endpoint: "127.0.0.1:9091"
```

Starts a local HTTP server that serves `text/event-stream` to connected
clients. Uses a tokio broadcast channel so multiple clients can subscribe.

#### Command execution

```yaml
events:
  command: "notify-send 'mcp2cli' '${MCP_EVENT_MESSAGE}'"
```

Runs a shell command (via `sh -c`) for each event with environment variables:

| Variable | Content |
|----------|---------|
| `MCP_EVENT_TYPE` | Event type: `info`, `progress`, `server_log`, `job_update`, `auth_prompt`, `list_changed` |
| `MCP_EVENT_JSON` | Full JSON-serialized event |
| `MCP_EVENT_APP_ID` | The app_id field |
| `MCP_EVENT_MESSAGE` | Human-readable one-line message |

**Examples:**

```yaml
# Send to syslog
events:
  command: "logger -t mcp2cli '${MCP_EVENT_MESSAGE}'"

# POST to webhook with curl
events:
  command: "curl -s -X POST http://hooks.internal/mcp -d \"${MCP_EVENT_JSON}\""

# Desktop notification (Linux)
events:
  command: "notify-send 'mcp2cli' '${MCP_EVENT_MESSAGE}'"
```

### How MCP server notifications are handled

During MCP operations, the server may send JSON-RPC notifications that mcp2cli
routes through the event broker to all configured sinks:

| Notification | Event type | Behavior |
|---|---|---|
| `notifications/progress` | `progress` | Progress token, current/total, message |
| `notifications/message` | `server_log` | Level, logger name, log data |
| `notifications/tools/list_changed` | `list_changed` | Writes stale marker; next `ls` forces re-discovery |
| `notifications/resources/list_changed` | `list_changed` | Same as tools |
| `notifications/prompts/list_changed` | `list_changed` | Same as tools |
| `notifications/resources/updated` | `info` | Resource URI in message |

Both stdio and streamable HTTP transports handle interleaved notifications
within the response stream.

---

## Sampling

When a server sends `sampling/createMessage` during a tool call, mcp2cli
presents the request interactively and collects a human response:

```text
--- sampling request ---
The server requests a model response.
Model hint: claude-3-5-sonnet
System: You are a helpful assistant
Max tokens: 1000

Messages:
  [user] Summarize this document

Your response (or 'decline' to reject): <type your response>
--- end sampling ---
```

The response is sent back with `model: "human-in-the-loop"` and
`role: "assistant"`. Type `decline` or press Enter with no input to reject
the request (returns JSON-RPC error -32600).

mcp2cli advertises `sampling` and `elicitation` capabilities during MCP
initialization.

---

## Alias Workflow

The alias workflow lets you create dedicated CLI entrypoints that feel like
standalone applications.

### Create a symlink alias

```bash
mcp2cli link create --name email
```

This creates a symlink in the link directory (typically `~/.local/bin/email`)
that points to the mcp2cli binary. When invoked as `email`, the runtime
selects the `email` config automatically.

### Validation

`link create` validates that a named config exists before creating the
symlink. Use `--force` to bypass this check:

```bash
mcp2cli link create --name staging --force
```

### Use the alias

```bash
email ls
email send --to user@example.com --subject "Hi" --body "from alias"
email --json doctor
```

The alias behaves identically to `mcp2cli email <command>` — it dispatches
based on `argv[0]`.

### Custom link directory

```bash
mcp2cli link create --name email --dir /usr/local/bin
```

### Reserved names

The names `mcp2cli`, `config`, `link`, and `use` are
reserved and cannot be used as alias names.

---

## Profile Overlays

Optional YAML profiles customize the dynamic CLI surface — renaming commands,
hiding internal tools, grouping related capabilities, and renaming flags.

### Adding a profile

Add the `profile` section to your config YAML:

```yaml
profile:
  display_name: "Email Toolkit"
  aliases:
    search: find
    send: post
    draft.create: compose           # Rename grouped subcommand: "draft create" → "draft compose"
    labels.list: ls                 # "labels list" → "labels ls"
  hide:
    - labels.add
    - debug-probe
  groups:
    triage:
      - reply
      - draft.list
  flags:
    send:
      body: text
  resource_verb: fetch
```

### Profile fields

| Field | Type | Purpose |
|---|---|---|
| `display_name` | string | Shown in `--help` banner instead of server name |
| `aliases` | map | Rename commands: `original: short-name`. Supports dotted names to rename grouped subcommands: `group.child: new-child` |
| `hide` | list | Hide commands from help and `ls` |
| `groups` | map of lists | Custom subcommand grouping |
| `flags` | nested map | Rename flags per command: `cmd: {flag: alias}` |
| `resource_verb` | string | Verb for resource reads (`get`, `fetch`, `read`) |

### Result

```bash
email post --to user@example.com --text hello   # Renamed command + flag (was: send --body)
email find --query "from:boss"                   # Shortened command name (was: search)
email draft compose --subject "New"              # Renamed grouped subcommand (was: draft create)
email labels ls                                  # Renamed within group (was: labels list)
email triage reply --thread-id 123 --body "Ok"   # Custom group
email fetch "mail://inbox"                        # Custom resource verb
```

Profiles are always optional. Without one, the CLI uses the server's metadata.

---

## Output Formats

### Human (default)

Human-readable text lines:

```text
send  tool  Sends an email message
```

### JSON

```bash
email --json ls
# or
email --output json ls
```

Structured JSON object:

```json
{
  "app_id": "email",
  "command": "discover",
  "summary": "discovered 14 capabilities",
  "lines": ["send  tool  Sends an email message", "..."],
  "data": { "category": "capabilities", "items": [...] }
}
```

### NDJSON

```bash
email --output ndjson send --to user@example.com --subject "Hi" --body "Hello"
```

Newline-delimited JSON — each event and the final output are separate JSON
lines, suitable for streaming consumption.

### Format precedence

1. `--json` flag (highest priority)
2. `--output <format>` flag
3. Config `defaults.output` value
4. `human` (fallback)

---

## Session and Capability Negotiation

### Protocol handshake

On the first operation in a session, mcp2cli performs the MCP initialization
handshake:

1. Sends `initialize` with:
   - Protocol version (`2025-03-26`)
   - Client info (`mcp2cli` + version)
   - Client capabilities (`elicitation: {}`)
2. Receives `InitializeResult` with:
   - Server protocol version
   - Server capabilities (tools, resources, prompts, logging, completions)
   - Server info (name + version)
3. Sends `notifications/initialized`

### Capability caching

Negotiated capabilities are persisted per config so subsequent commands can
skip re-initialization and validate commands against known server capabilities.

### Capability enforcement

Before executing a command, mcp2cli checks cached capabilities to fast-fail if
the server doesn't support the requested operation. For example, if the
server's capabilities don't include `prompts`, running a prompt command will
fail immediately with a clear error.

### Session persistence

For streamable HTTP transport, session IDs (returned via `Mcp-Session` header)
are tracked and used to maintain session continuity across requests.

---

## Transports

### Stdio

The stdio transport spawns a subprocess MCP server and communicates via
line-delimited JSON-RPC on stdin/stdout.

```yaml
server:
  transport: stdio
  stdio:
    command: npx
    args: ['@modelcontextprotocol/server-everything']
    cwd: /path/to/working-dir     # Optional
    env:                           # Optional environment variables
      NODE_ENV: production
```

**Protocol details:**
- Each JSON-RPC message is a single line terminated by `\n`
- The runtime reads server responses line by line
- Server→client requests (e.g. `elicitation/create`) are handled inline
  during pending commands
- Notifications are dispatched to the event broker

### Streamable HTTP

The streamable HTTP transport sends JSON-RPC requests as HTTP POST to the
server endpoint.

```yaml
server:
  transport: streamable_http
  endpoint: http://127.0.0.1:3001/mcp
```

**Protocol details:**
- Requests include `Accept: text/event-stream, application/json` and
  `Content-Type: application/json`
- Responses may be SSE streams containing JSON-RPC response events
- Session ID from `Mcp-Session` header is tracked and sent on subsequent
  requests

### Demo mode

Configs with the endpoint `https://demo.invalid/mcp` use a built-in demo
backend with file-backed state. Useful for testing and learning without a
real server.

---

## Best Practices

### 1. Run `ls` after setting up a config

```bash
email ls
```

This populates the discovery cache, enabling:
- Typed commands with proper `--flags`
- Fast listing without hitting the server
- Capability validation before sending requests
- Offline fallback when the server is unreachable

### 2. Use `--json` for scripting and automation

```bash
email --json ls | jq '.data.items[].id'
email --json send --to user@example.com --subject "Hi" --body "test" | jq '.data'
email --json auth status | jq '.data.auth_session.state'
```

### 3. Use the alias workflow for multi-server setups

```bash
mcp2cli config init --name dev --transport stdio --stdio-command ...
mcp2cli config init --name staging --transport streamable_http --endpoint ...
mcp2cli config init --name prod --transport streamable_http --endpoint ...

mcp2cli link create --name dev
mcp2cli link create --name staging
mcp2cli link create --name prod

dev ls
staging deploy --version 1.2.3
prod doctor
```

### 4. Use `--args-file` for complex payloads

```bash
email send --args-file ./message.json
```

### 5. Use background jobs for long-running operations

```bash
email send --to list@example.com --subject "Q4 update" --body "..." --background
email jobs watch --latest
```

### 6. Use doctor to diagnose issues

```bash
email doctor
```

Check: Is auth set up? Is the server reachable? Are capabilities cached?
What protocol version is negotiated?

### 7. Set up event delivery for observability

```yaml
events:
  enable_stdio_events: false
  http_endpoint: "http://monitoring:9090/mcp-events"
```

### 8. Use `--config` for ad-hoc server connections

```bash
mcp2cli myserver --config /path/to/server.yaml ls
```

### 9. Keep logging quiet in normal use

```bash
MCP2CLI_LOGGING__LEVEL=debug email send --to user@example.com --subject "Hi" --body "test" 2>debug.log
```

### 10. Use profile overlays for polished CLIs

```yaml
profile:
  aliases:
    search: find
  hide:
    - internal-debug-tool
```

---

## Complete Command Reference

### Host commands (no config required)

| Command | Description |
|---------|------------|
| `mcp2cli config init` | Create a new named config |
| `mcp2cli config list` | List all named configs |
| `mcp2cli config show --name <NAME>` | Show a specific config |
| `mcp2cli link create --name <NAME>` | Create a symlink alias |
| `mcp2cli use <NAME>` | Set active config |
| `mcp2cli use --show` | Show active config |
| `mcp2cli use --clear` | Clear active config |

### Server-derived commands (from discovery)

| Pattern | Description |
|---------|------------|
| `<tool-name> [--flags]` | Call a tool (flags from JSON Schema) |
| `<prompt-name> [--flags]` | Run a prompt (flags from arguments) |
| `<template-name> [--flags\|positional]` | Read a resource template |
| `get <URI>` | Read a concrete resource |
| `<group> <subcommand>` | Dotted names become subcommand groups |

### Runtime commands (always available)

| Command | Description |
|---------|------------|
| `auth login` | Authenticate with the server |
| `auth logout` | Clear stored credentials |
| `auth status` | Show auth state |
| `jobs list` | List background jobs |
| `jobs show [ID \| --latest]` | Show job details |
| `jobs wait [ID \| --latest]` | Wait for job completion |
| `jobs cancel [ID \| --latest]` | Cancel a running job |
| `jobs watch [ID \| --latest]` | Watch job progress |
| `doctor` | Show runtime health |
| `inspect` | Show server capabilities and metadata |
| `ls [--tools\|--resources\|--prompts] [--filter]` | Unified capability listing |
| `ping` | Server liveness check |
| `log <LEVEL>` | Set server-side logging level |
| `complete <REF> <NAME> <ARG> [VALUE]` | Request tab-completions |

### Global flags

| Flag | Description |
|------|------------|
| `--json` | Output in JSON format |
| `--output <FORMAT>` | Output format: `human`, `json`, `ndjson` |
| `--config <PATH>` | Use a specific config file path |
| `--background` | Run as background job (tools only) |
