# mcp2cli

**Turn any MCP server into a native command-line application.**

Server tools become verbs. Resources become nouns. Prompts become workflows. No MCP protocol knowledge required at the terminal.

```bash
# One binary. Any server. Real CLI commands.
work echo --message "hello world"
work email send --to user@example.com --body "Meeting at 3"
work get file:///project/README.md
staging deploy --version 2.1.0 --background
prod --json doctor | jq '.data.server'
```

---

## Why mcp2cli?

MCP (Model Context Protocol) defines a powerful standard for servers to expose tools, resources, and prompts. But interacting with an MCP server requires JSON-RPC plumbing, session negotiation, and protocol-level knowledge — none of which belongs in a CLI.

mcp2cli bridges that gap:

| Problem | mcp2cli |
|---------|---------|
| MCP requires JSON-RPC plumbing | Auto-discovers capabilities → typed `--flags` from JSON Schema |
| Testing MCP servers needs custom client code | Point, shoot: `mcp2cli --url http://localhost:3001/mcp ls` |
| Each server needs its own CLI wrapper | One binary + config files = unlimited server bindings |
| AI agents can't easily call MCP tools from shell | `--json` output → structured envelopes for programmatic parsing |
| CI/CD can't orchestrate MCP operations | Exit codes, JSON output, pipes, `--timeout`, `--background` |
| Server tools have cryptic protocol names | Profile overlays rename, group, and hide commands |

---

## Quick Start

### Install

```bash
cargo install --path .
```

### Option A: Ad-hoc — zero config

```bash
# HTTP server — just point and go
mcp2cli --url http://127.0.0.1:3001/mcp ls
mcp2cli --url http://127.0.0.1:3001/mcp echo --message hello

# Stdio server — just run
mcp2cli --stdio "npx @modelcontextprotocol/server-everything" ls
```

### Option B: Named config + alias

```bash
# Create a config
mcp2cli config init --name work --app bridge \
  --transport streamable_http --endpoint http://127.0.0.1:3001/mcp

# Create a symlink alias
mcp2cli link create --name work

# Use it like a standalone app
work ls                           # Discover capabilities
work echo --message hello         # Call a tool
work get demo://resource/readme   # Read a resource
work doctor                       # Health check
```

### Option C: Demo mode — no server needed

```bash
mcp2cli config init --name demo --app bridge \
  --transport streamable_http --endpoint https://demo.invalid/mcp
mcp2cli use demo
mcp2cli ls
```

> **[Full getting started guide →](docs/getting-started.md)**

---

## How It Works

```
mcp2cli discovers → builds manifest → generates CLI → parses input → executes MCP call
```

1. **Discovers** the server's tools, resources, resource templates, and prompts
2. **Builds a command manifest** — each capability becomes a typed command with flags derived from JSON Schema
3. **Generates a clap CLI tree** — dotted names become nested subcommands, required fields become required flags
4. **Parses your input** against the generated tree with full type validation
5. **Executes** the MCP operation and renders the result

### Schema-to-flag mapping

| JSON Schema | CLI flag | Example |
|-------------|----------|---------|
| `string` | `--name <TEXT>` | `--message hello` |
| `integer` | `--count <INT>` | `--steps 5` |
| `number` | `--rate <NUM>` | `--temperature 0.7` |
| `boolean` | `--flag` | `--include-image` |
| `enum` | `--kind <A\|B\|C>` | `--level error` |
| `array` | `--tags <VAL,...>` | `--labels bug,urgent` |
| Complex | `--config <JSON>` | `--config '{"a":1}'` |

### Namespace grouping

Dotted tool names automatically become nested subcommands:

```bash
# Server tools: email.send, email.reply, email.draft.create
work email send --to user@example.com --body "Hello"
work email reply --thread-id 123
work email draft create --subject "New draft"
```

---

## Features at a Glance

### Core

- **[Discovery-driven CLI](docs/features/discovery-driven-cli.md)** — server capabilities auto-generate typed CLI commands with `--flags` from JSON Schema
- **[Named configs & aliases](docs/features/named-configs-and-aliases.md)** — `mcp2cli use <name>`, symlink aliases (`work`, `prod`, `staging`), dispatch routing
- **[Profile overlays](docs/features/profile-overlays.md)** — rename, hide, group, alias commands; rename flags; change resource verbs
- **[Ad-hoc connections](docs/features/ad-hoc-connections.md)** — `--url` and `--stdio` for config-free, zero-setup usage
- **[Fuzzy matching](docs/features/fuzzy-matching.md)** — "Did you mean?" suggestions for mistyped commands

### Transports

- **[Streamable HTTP](docs/features/transports.md)** — JSON-RPC over HTTP with SSE streaming and session negotiation
- **[Stdio](docs/features/transports.md)** — spawn local subprocess servers, communicate via stdin/stdout
- **[Demo mode](docs/features/transports.md)** — `demo.invalid` file-backed backend for offline learning and testing

### Protocol Coverage

Full [MCP 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25) implementation. See the **[Protocol Coverage reference](docs/protocol-coverage.md)** for per-feature detail, source pointers, and CLI examples.

| Category | Methods / notifications | mcp2cli surface |
|---|---|---|
| **Lifecycle** | `initialize`, `notifications/initialized`, `ping` | One handshake per session; `ping` exposed as a CLI command |
| **Discovery** | `tools/list`, `resources/list`, `resources/templates/list`, `prompts/list` + `notifications/*/list_changed` | `ls` populates a persistent cache; list-change notifications invalidate it automatically |
| **Tool invocation** | `tools/call` | Typed `clap` flags from JSON Schema (required/optional, enums, defaults, nested objects); progress token attached automatically |
| **Resources** | `resources/read`, `resources/subscribe`, `resources/unsubscribe`, `notifications/resources/updated` | `get <URI>` for concrete reads; parameterised templates surface as commands; `subscribe`/`unsubscribe` stream change events |
| **Prompts** | `prompts/get` | Each prompt becomes a command with typed argument flags derived from the prompt definition |
| **Completions** | `completion/complete` | `complete` command with `ref` context (resource or prompt) per MCP 2025-11-25 |
| **Logging** | `logging/setLevel`, `notifications/message` | `log level <LEVEL>`; server-emitted logs are surfaced as runtime events and written to the configured sinks |
| **Progress** | `notifications/progress` + `_meta.progressToken` | Progress tokens auto-attached to long-running ops; ticks rendered to stderr or event sinks |
| **Cancellation** | `notifications/cancelled` (bidirectional) | Ctrl+C sends a cancel for the in-flight request; inbound cancels are acknowledged |
| **Elicitation** *(server→client)* | [`elicitation/create`](docs/features/elicitation-and-sampling.md) | Interactive terminal prompt — form mode (typed fields from JSON Schema) + URL mode (open-in-browser) |
| **Sampling** *(server→client)* | [`sampling/createMessage`](docs/features/elicitation-and-sampling.md) | Human-in-the-loop review with tool display before forwarding to the LLM |
| **Roots** *(server→client)* | `roots/list` | Client advertises configurable root URIs; servers query on demand |
| **Tasks** *(MCP 2025-11-25)* | `tasks/get`, `tasks/result`, `tasks/cancel`, `_meta.task` on `tools/call` | [`--background`](docs/features/background-jobs.md) creates a task; `jobs show/wait/cancel/watch` tracks it across invocations |

### Operations

- **[Authentication](docs/features/authentication.md)** — `auth login/logout/status` with file-backed token persistence
- **[Event system](docs/features/event-system.md)** — 5 sink types: stderr, HTTP webhook, Unix socket, SSE server, command exec
- **[Output formats](docs/features/output-formats.md)** — `--json`, `--output json|ndjson|human` — every command supports structured output
- **[Request timeouts](docs/features/request-timeouts.md)** — global default + per-command `--timeout` override
- **[Daemon mode](docs/features/daemon-mode.md)** — keep MCP connections warm between invocations (Unix socket IPC)
- **Doctor & Inspect** — runtime health diagnostics, full capability dump

---

## Multi-Server Workflow

Each alias routes to a different MCP server. Each feels like its own standalone application.

```bash
# Set up multiple servers
mcp2cli config init --name dev --transport stdio --stdio-command ./dev-server
mcp2cli config init --name staging --transport streamable_http --endpoint https://staging.api/mcp
mcp2cli config init --name prod --transport streamable_http --endpoint https://prod.api/mcp

# Create aliases
mcp2cli link create --name dev
mcp2cli link create --name staging
mcp2cli link create --name prod

# Each alias is its own CLI
dev ls                          # Local dev server
staging deploy --version 1.2.3  # Staging HTTP server
prod doctor                     # Production health check
```

---

## JSON Output for Scripts & Agents

Every command supports structured JSON output:

```bash
# JSON envelope
work --json ls | jq '.data.items[].id'

# Tool result
work --json echo --message hello | jq '.data.content[0].text'

# Health check data
work --json doctor | jq '.data.server'

# Machine-readable discovery
work --json ls --tools | jq '[.data.items[] | {id, kind, summary}]'
```

Consistent envelope format:

```json
{
  "app_id": "work",
  "command": "invoke",
  "summary": "called echo",
  "lines": ["..."],
  "data": { "content": [...] }
}
```

---

## Profile Overlays

Customize the CLI surface per-server — no server changes needed:

```yaml
profile:
  display_name: "Email CLI"
  aliases:
    long-running-operation: lro     # Rename commands
    echo: ping
  hide:
    - debug-tool                    # Hide from help/ls
  groups:
    mail:                           # Custom grouping
      - send
      - reply
      - draft-create
  flags:
    echo:
      message: msg                  # Rename flags
  resource_verb: fetch              # "fetch" instead of "get"
```

---

## Config Model

Each named config is a YAML file. Minimal example:

```yaml
schema_version: 1
server:
  transport: streamable_http
  endpoint: http://localhost:3001/mcp
```

Full example with all options:

```yaml
schema_version: 1

app:
  profile: bridge

server:
  display_name: My MCP Server
  transport: stdio                       # or streamable_http
  endpoint: null                         # required for streamable_http
  stdio:
    command: npx
    args: ['@modelcontextprotocol/server-everything']
  roots:
    - uri: "file:///home/user/project"
      name: "Project Root"

defaults:
  output: human                          # human | json | ndjson
  timeout_seconds: 120                   # 0 = no timeout

logging:
  level: warn
  format: pretty
  outputs:
    - kind: stderr

auth:
  browser_open_command: null

events:
  enable_stdio_events: true
  # http_endpoint: "http://127.0.0.1:9090/events"
  # local_socket_path: "/tmp/mcp2cli-events.sock"
  # sse_endpoint: "127.0.0.1:9091"
  # command: "logger -t mcp2cli '${MCP_EVENT_MESSAGE}'"

profile:
  display_name: "My Tool"
  aliases: {}
  hide: []
  groups: {}
  flags: {}
  resource_verb: get
```

> **[Full config reference →](docs/reference/config-reference.md)**

---

## Runtime Commands

Always available alongside server-derived commands:

| Command | Description |
|---------|-------------|
| `ls [--tools\|--resources\|--prompts] [--filter]` | Discover capabilities |
| `ping` | Server liveness with latency |
| `doctor` | Runtime health diagnostics |
| `inspect` | Full capability dump |
| `auth login\|logout\|status` | Authentication management |
| `jobs list\|show\|wait\|cancel\|watch` | Background job management |
| `log <LEVEL>` | Set server-side log level |
| `subscribe <URI>` / `unsubscribe <URI>` | Resource change notifications |
| `complete <REF> <NAME> <ARG>` | Tab-completion from server |

## Host Commands

Manage configs and aliases — no server connection needed:

| Command | Description |
|---------|-------------|
| `mcp2cli config init [options]` | Create a named config |
| `mcp2cli config list` | List all configs |
| `mcp2cli config show --name <NAME>` | Show a config |
| `mcp2cli use <NAME>` | Set active config |
| `mcp2cli link create --name <NAME>` | Create symlink alias |
| `mcp2cli daemon start\|stop\|status` | Manage connection daemons |

> **[Full CLI reference →](docs/reference/cli-reference.md)**

---

## Use Cases

mcp2cli is useful across the entire MCP lifecycle:

| Use Case | How |
|----------|-----|
| **Test an MCP server** | `mcp2cli --url http://localhost:3001/mcp doctor` — instant health check |
| **E2E conformance testing** | Bash test suites that validate every MCP spec section with assertions |
| **Local server development** | `mcp2cli --stdio "./my-server" ls` — test as you build, zero client code |
| **AI agent tool-use** | `--json` output → parse in Python/Node → agents call any MCP tool |
| **CI/CD pipelines** | JSON output, exit codes, `--timeout`, `--background` — pipeline-native |
| **Infrastructure automation** | Per-service aliases → `k8s deploy`, `db backup`, `mon status` |
| **Shell scripting** | Pipe to `jq`, loop over tools, retry with backoff |
| **Multi-server orchestration** | Named configs + symlinks = cross-service workflows in bash |
| **Production monitoring** | Event sinks → webhooks, SSE, Unix sockets, custom commands |

> **[Detailed articles for each use case →](docs/index.md#articles--guides)**

---

## Testing

```bash
cargo test --lib                # Unit tests
cargo test --test integration   # Integration tests
cargo test                      # All tests
```

### Local Validation Servers

```bash
# Streamable HTTP server
npx @modelcontextprotocol/server-everything streamableHttp
# → http://127.0.0.1:3001/mcp

# Stdio server
npx @modelcontextprotocol/server-everything
```

### E2E Testing Your Own Server

Use mcp2cli as a conformance test harness:

```bash
# Quick smoke test
mcp2cli --url http://localhost:3001/mcp doctor

# Structured conformance suite
./run-conformance.sh --url http://localhost:3001/mcp
```

> **[E2E conformance testing guide →](docs/articles/e2e-conformance-testing.md)**

---

## Documentation

### Getting Started

| Document | Description |
|----------|-------------|
| [Getting Started](docs/getting-started.md) | Install, configure, run your first command |
| [CLI Reference](docs/reference/cli-reference.md) | Every command, flag, and option |
| [Config Reference](docs/reference/config-reference.md) | Complete YAML schema |
| [config.example.yaml](config.example.yaml) | Annotated config template |

### Feature Guides

| Feature | Guide |
|---------|-------|
| Discovery-driven CLI | [docs/features/discovery-driven-cli.md](docs/features/discovery-driven-cli.md) |
| Profile overlays | [docs/features/profile-overlays.md](docs/features/profile-overlays.md) |
| Transports | [docs/features/transports.md](docs/features/transports.md) |
| Ad-hoc connections | [docs/features/ad-hoc-connections.md](docs/features/ad-hoc-connections.md) |
| Request timeouts | [docs/features/request-timeouts.md](docs/features/request-timeouts.md) |
| Fuzzy matching | [docs/features/fuzzy-matching.md](docs/features/fuzzy-matching.md) |
| Daemon mode | [docs/features/daemon-mode.md](docs/features/daemon-mode.md) |
| Background jobs | [docs/features/background-jobs.md](docs/features/background-jobs.md) |
| Event system | [docs/features/event-system.md](docs/features/event-system.md) |
| Authentication | [docs/features/authentication.md](docs/features/authentication.md) |
| Output formats | [docs/features/output-formats.md](docs/features/output-formats.md) |
| Elicitation & sampling | [docs/features/elicitation-and-sampling.md](docs/features/elicitation-and-sampling.md) |
| Named configs & aliases | [docs/features/named-configs-and-aliases.md](docs/features/named-configs-and-aliases.md) |

### Articles

| Article | Audience |
|---------|----------|
| [AI Agents + MCP via CLI](docs/articles/ai-agents-mcp-cli.md) | Agent developers |
| [E2E & Conformance Testing](docs/articles/e2e-conformance-testing.md) | MCP server authors |
| [Testing MCP Servers](docs/articles/testing-mcp-servers.md) | Server validation |
| [Local Development & Prototyping](docs/articles/local-dev-prototyping.md) | Server developers |
| [Shell Scripting with MCP](docs/articles/shell-scripting-mcp.md) | DevOps |
| [Multi-Server Workflows](docs/articles/multi-server-workflows.md) | Platform engineers |
| [Platform Engineering](docs/articles/platform-engineering.md) | Infrastructure teams |
| [From Zero to Production](docs/articles/from-zero-to-production.md) | Production deployment |

### Architecture

| Document | Description |
|----------|-------------|
| [Design Proposal](DESIGN-PROPOSAL.md) | Architecture of the discovery-driven dynamic CLI |
| [MCP Spec Compliance](docs/mcp-spec-compliance-gap-analysis.md) | Spec compliance audit and gap tracking |
| [Implementation Plan](docs/PLAN-2025-11-25.md) | MCP 2025-11-25 compliance roadmap |

---

## Telemetry

mcp2cli collects **anonymous, non-sensitive** usage telemetry to help us understand which features are used and where to focus improvements. We follow the same opt-out model used by Homebrew, Rust, and VS Code.

### What is collected

- Command category (e.g. "tool_invoke", "discover", "auth" — **never** the actual tool/prompt name)
- Transport type used (stdio, HTTP)
- Whether features like `--json`, `--background`, `--timeout`, daemon, profile overlay, or ad-hoc mode were used
- Outcome (success/error) and duration in milliseconds
- OS, architecture, and CLI version
- A random installation UUID (not tied to your identity)

### What is NOT collected

- No server endpoints, URIs, tool names, argument values, file paths, or configuration content
- No IP addresses or user identifiers
- No environment variables or credentials

### How to opt out

Any one of these disables telemetry completely:

```bash
# Config file
telemetry:
  enabled: false

# Environment variable
export MCP2CLI_TELEMETRY=off

# CLI flag (per-invocation)
mcp2cli --no-telemetry ls

# Respect DO_NOT_TRACK standard (https://consoledonottrack.com/)
export DO_NOT_TRACK=1
```

### Where data is stored

Events are written to `~/.local/share/mcp2cli/telemetry.ndjson` as newline-delimited JSON. You can inspect, delete, or rotate this file at any time.

> **[Telemetry collection & backend setup guide →](docs/telemetry-collection.md)**

---

## Stewardship

`mcp2cli` is operated and developed by **TSOK — The Source of Knowledge
AI Laboratory** ([tsok.org](https://tsok.org)), with engineering and
agent operations supported by **TSOK-Bot** ([tsok.bot](https://tsok.bot)),
TSOK's in-house AI agent platform. The project is released under the
Apache License 2.0; see [LICENSE](LICENSE).

## License

Apache License 2.0 — see [LICENSE](LICENSE).