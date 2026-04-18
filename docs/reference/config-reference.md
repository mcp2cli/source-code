# Configuration Reference

Complete reference for the mcp2cli YAML configuration schema.

---

## Config File Location

```text
~/.local/share/mcp2cli/configs/<name>.yaml
```

Override with: `MCP2CLI_CONFIG_DIR=/custom/path`

---

## Full Schema

```yaml
schema_version: 1                            # Required. Always 1.

app:
  profile: bridge                            # Application profile (only "bridge" currently)

server:
  display_name: "My MCP Server"              # Human-readable name for --help banners
  transport: streamable_http                 # streamable_http | stdio
  endpoint: "http://localhost:3001/mcp"      # Required for streamable_http transport
  stdio:
    command: "npx"                           # Subprocess command (required for stdio)
    args:                                    # Subprocess arguments
      - "@modelcontextprotocol/server-everything"
    cwd: "/path/to/working/dir"             # Optional working directory
    env:                                     # Optional environment variables
      API_KEY: "sk-abc123"
      NODE_ENV: "production"
  roots:                                     # Optional: exposed via roots/list
    - uri: "file:///home/user/project"
      name: "Project Root"

defaults:
  output: human                              # human | json | ndjson
  timeout_seconds: 120                       # Operation timeout (0 = no timeout)

logging:
  level: warn                                # trace | debug | info | warn | error
  format: pretty                             # pretty | json
  outputs:
    - kind: stderr                           # stderr | stdout | file
    # - kind: file
    #   path: /var/log/mcp2cli.log

auth:
  browser_open_command: "xdg-open"           # For OAuth browser flows (null = disabled)
  token_store_file: null                     # Custom token path (auto-derived if null)

events:
  enable_stdio_events: true                  # Human-readable events on stderr
  http_endpoint: null                        # HTTP webhook URL (POST JSON per event)
  local_socket_path: null                    # Unix socket path (NDJSON)
  sse_endpoint: null                         # SSE server bind address (host:port)
  command: null                              # Shell command per event

profile:                                     # Optional: customize CLI surface
  display_name: "My Custom CLI"
  aliases: {}                                # Rename commands
  hide: []                                   # Hide commands from help/ls
  groups: {}                                 # Custom command grouping
  flags: {}                                  # Rename flags per command
  resource_verb: get                         # Verb for resource reads
```

---

## Field Reference

### `schema_version`

| Type | Default | Required |
|------|---------|----------|
| `integer` | `1` | Yes |

Must be `1`. Used for future config migrations.

---

### `app`

#### `app.profile`

| Type | Default | Required |
|------|---------|----------|
| `string` | `"bridge"` | Yes |

The application profile to use. Currently only `bridge` is supported.

---

### `server`

#### `server.display_name`

| Type | Default | Required |
|------|---------|----------|
| `string` | `"MCP Bridge Server"` | No |

Human-readable name shown in `--help` banners and JSON output.

#### `server.transport`

| Type | Default | Required |
|------|---------|----------|
| `string` | `streamable_http` | Yes |

Transport protocol. Values:

| Value | Description |
|-------|-------------|
| `streamable_http` | HTTP JSON-RPC with SSE streaming |
| `stdio` | Stdin/stdout with subprocess |

#### `server.endpoint`

| Type | Default | Required |
|------|---------|----------|
| `string \| null` | `null` | For HTTP |

Full URL for the MCP server endpoint. Required when `transport: streamable_http`.

Special value: `https://demo.invalid/mcp` activates the built-in demo backend.

#### `server.stdio.command`

| Type | Default | Required |
|------|---------|----------|
| `string \| null` | `null` | For stdio |

Executable to spawn as the MCP server subprocess.

#### `server.stdio.args`

| Type | Default |
|------|---------|
| `string[]` | `[]` |

Arguments passed to the subprocess.

#### `server.stdio.cwd`

| Type | Default |
|------|---------|
| `string \| null` | `null` |

Working directory for the subprocess. If null, inherits the current directory.

#### `server.stdio.env`

| Type | Default |
|------|---------|
| `map<string, string>` | `{}` |

Environment variables merged into the subprocess environment.

#### `server.roots`

| Type | Default |
|------|---------|
| `array` | `[]` |

Roots exposed to the server via `roots/list`. Each entry has:

| Field | Type | Description |
|-------|------|-------------|
| `uri` | `string` | Root URI (typically `file://` paths) |
| `name` | `string` | Human-readable label |

---

### `defaults`

#### `defaults.output`

| Type | Default | Values |
|------|---------|--------|
| `string` | `human` | `human`, `json`, `ndjson` |

Default output format. Overridden by `--json` or `--output` flags.

#### `defaults.timeout_seconds`

| Type | Default | Range |
|------|---------|-------|
| `integer` | `120` | `0`–∞ |

Default timeout for all MCP operations, in seconds. `0` disables timeouts.

Overridden by `--timeout` flag per-command.

---

### `logging`

#### `logging.level`

| Type | Default | Values |
|------|---------|--------|
| `string` | `warn` | `trace`, `debug`, `info`, `warn`, `error` |

Minimum log level for tracing output.

#### `logging.format`

| Type | Default | Values |
|------|---------|--------|
| `string` | `pretty` | `pretty`, `json` |

Log output format. `json` is useful for log aggregation.

#### `logging.outputs`

| Type | Default |
|------|---------|
| `array` | `[{kind: stderr}]` |

Log output destinations. Each entry:

| Field | Values | Description |
|-------|--------|-------------|
| `kind` | `stderr`, `stdout`, `file` | Output target |
| `path` | (file only) | File path for `kind: file` |

---

### `auth`

#### `auth.browser_open_command`

| Type | Default |
|------|---------|
| `string \| null` | `null` |

Command to open URLs in the browser (for OAuth flows). Examples: `xdg-open`, `open`, `wsl-open`.

#### `auth.token_store_file`

| Type | Default |
|------|---------|
| `string \| null` | `null` (auto-derived) |

Custom path for token storage. If null, defaults to `instances/<name>/tokens.json`.

---

### `events`

#### `events.enable_stdio_events`

| Type | Default |
|------|---------|
| `boolean` | `true` |

Write human-readable event lines to stderr.

#### `events.http_endpoint`

| Type | Default |
|------|---------|
| `string \| null` | `null` |

HTTP URL for webhook event delivery. Each event is POSTed as JSON.

#### `events.local_socket_path`

| Type | Default |
|------|---------|
| `string \| null` | `null` |

Path to a Unix domain socket for NDJSON event delivery.

#### `events.sse_endpoint`

| Type | Default |
|------|---------|
| `string \| null` | `null` |

Bind address for an SSE (Server-Sent Events) HTTP server. Format: `host:port`.

#### `events.command`

| Type | Default |
|------|---------|
| `string \| null` | `null` |

Shell command executed per event. Environment variables available:

| Variable | Content |
|----------|---------|
| `MCP_EVENT_TYPE` | Event type name |
| `MCP_EVENT_JSON` | Full JSON event |
| `MCP_EVENT_APP_ID` | Config name |
| `MCP_EVENT_MESSAGE` | One-line message |

---

### `profile`

Optional profile overlay for CLI customization.

#### `profile.display_name`

| Type | Default |
|------|---------|
| `string \| null` | `null` |

Custom name for `--help` banner. If null, uses `server.display_name`.

#### `profile.aliases`

| Type | Default |
|------|---------|
| `map<string, string>` | `{}` |

Command renames. Keys are original names (dot-notation for grouped), values are new names.

```yaml
aliases:
  long-running-operation: lro         # Top-level rename
  create.payload: object              # Grouped subcommand rename
```

#### `profile.hide`

| Type | Default |
|------|---------|
| `string[]` | `[]` |

Commands to hide from `--help` and `ls`.

#### `profile.groups`

| Type | Default |
|------|---------|
| `map<string, string[]>` | `{}` |

Custom grouping. Keys are group names, values are lists of command names.

```yaml
groups:
  mail:
    - send
    - reply
    - draft-create
```

#### `profile.flags`

| Type | Default |
|------|---------|
| `map<string, map<string, string>>` | `{}` |

Per-command flag renames. Outer key is command name, inner map is original→new.

```yaml
flags:
  echo:
    message: msg
```

#### `profile.resource_verb`

| Type | Default |
|------|---------|
| `string` | `"get"` |

Verb for the resource read command.

---

## Environment Variable Overrides

Any config field can be overridden via environment variable:

```text
MCP2CLI_<PATH__TO__FIELD>=<value>
```

| Config Path | Environment Variable |
|-------------|---------------------|
| `logging.level` | `MCP2CLI_LOGGING__LEVEL` |
| `server.endpoint` | `MCP2CLI_SERVER__ENDPOINT` |
| `defaults.output` | `MCP2CLI_DEFAULTS__OUTPUT` |
| `defaults.timeout_seconds` | `MCP2CLI_DEFAULTS__TIMEOUT_SECONDS` |

Additionally:

| Variable | Description |
|----------|-------------|
| `MCP2CLI_CONFIG_DIR` | Override config directory path |
| `MCP2CLI_DATA_DIR` | Override data directory path |

---

## Data Directories

| Path | Content |
|------|---------|
| `configs/` | Named config YAML files |
| `instances/<name>/discovery.json` | Cached discovery inventory |
| `instances/<name>/tokens.json` | Stored auth credentials |
| `instances/<name>/session.json` | Negotiated capabilities cache |
| `instances/<name>/daemon.json` | Daemon PID file |
| `instances/<name>/daemon.sock` | Daemon Unix socket |
| `instances/<name>/jobs/` | Background job records |
| `active.json` | Currently active config pointer |

Default base: `~/.local/share/mcp2cli/`

---

## Minimal Configs

### HTTP server

```yaml
schema_version: 1
server:
  transport: streamable_http
  endpoint: http://localhost:3001/mcp
```

### Stdio server

```yaml
schema_version: 1
server:
  transport: stdio
  stdio:
    command: npx
    args: ['@modelcontextprotocol/server-everything']
```

### Demo mode

```yaml
schema_version: 1
server:
  transport: streamable_http
  endpoint: https://demo.invalid/mcp
```

---

## `telemetry`

Anonymous usage telemetry configuration. See [Telemetry Collection Guide](../telemetry-collection.md) for full details.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | `bool` | `true` | Master switch. Set to `false` to disable telemetry |
| `endpoint` | `string?` | `null` | Optional HTTP endpoint for shipping NDJSON event batches |
| `batch_size` | `int` | `25` | Number of events to batch before shipping to HTTP endpoint |

```yaml
telemetry:
  enabled: true
  # endpoint: "https://your-collector.example.com/v1/events"
  # batch_size: 25
```

### Disabling telemetry

Any one of these disables telemetry completely:

```yaml
# In config
telemetry:
  enabled: false
```

```bash
# Via environment
export MCP2CLI_TELEMETRY=off

# Via CLI flag
mcp2cli --no-telemetry ls

# Via DO_NOT_TRACK standard
export DO_NOT_TRACK=1
```

---

## See Also

- [CLI Reference](cli-reference.md) — every command and flag
- [Getting Started](../getting-started.md) — config setup walkthrough
- [Profile Overlays](../features/profile-overlays.md) — profile customization guide
- [Telemetry Collection](../telemetry-collection.md) — telemetry backend setup
