# mcp2cli Use Cases

Real-world scenarios and workflow patterns — from single-server setups to
multi-environment CI/CD pipelines.

---

## Table of Contents

1. [Single-Server Setup](#1-single-server-setup)
2. [Multi-Environment Aliases](#2-multi-environment-aliases)
3. [Subprocess MCP Server](#3-subprocess-mcp-server)
4. [Namespace Grouping (Dotted Tools)](#4-namespace-grouping)
5. [Scripting and Automation](#5-scripting-and-automation)
6. [Long-Running Operations](#6-long-running-operations)
7. [Complex Argument Payloads](#7-complex-argument-payloads)
8. [Server-Initiated Elicitation](#8-server-initiated-elicitation)
9. [Event-Driven Monitoring](#9-event-driven-monitoring)
10. [Server-Initiated Sampling](#10-server-initiated-sampling)
11. [Offline and Disconnected Work](#11-offline-and-disconnected-work)
12. [CI/CD Integration](#12-cicd-integration)
13. [Profile Overlays (Curated CLI)](#13-profile-overlays)
14. [Email MCP Server](#14-email-mcp-server)
15. [GitHub MCP Server](#15-github-mcp-server)
16. [Database MCP Server](#16-database-mcp-server)
17. [Debugging and Diagnostics](#17-debugging-and-diagnostics)
18. [Resource Templates](#18-resource-templates)
19. [Tab Completion and Server Completions](#19-tab-completion-and-server-completions)
20. [Multi-User Shared Infrastructure](#20-multi-user-shared-infrastructure)
21. [Demo and Learning Mode](#21-demo-and-learning-mode)
22. [Resource Subscriptions (Live Watching)](#22-resource-subscriptions)
23. [Workspace Roots for Context-Aware Servers](#23-workspace-roots)
24. [Task Lifecycle Management](#24-task-lifecycle-management)
25. [Request Cancellation](#25-request-cancellation)
26. [Infrastructure Provisioning Server](#26-infrastructure-provisioning-server)
27. [Data Pipeline Orchestration](#27-data-pipeline-orchestration)

---

## 1. Single-Server Setup

**Scenario:** You have one MCP server and want to interact with it from the
terminal.

```bash
# Create a config
mcp2cli config init --name api \
  --app bridge \
  --transport streamable_http \
  --endpoint https://api.example.com/mcp

# Set as active
mcp2cli use api

# Discover what the server offers
mcp2cli ls

# Use tools — each is a first-class command with typed flags
mcp2cli get-user --id 123
mcp2cli create-order --product widget --quantity 5

# Read resources
mcp2cli get "orders://recent"

# Run prompts
mcp2cli summarize-order --order-id 456
```

---

## 2. Multi-Environment Aliases

**Scenario:** You work with dev, staging, and production servers and want each
to feel like a separate CLI application.

```bash
# Create configs for each environment
mcp2cli config init --name dev \
  --transport stdio --stdio-command ./dev-server

mcp2cli config init --name staging \
  --transport streamable_http \
  --endpoint https://staging.api.example.com/mcp

mcp2cli config init --name prod \
  --transport streamable_http \
  --endpoint https://prod.api.example.com/mcp

# Create symlink aliases
mcp2cli link create --name dev
mcp2cli link create --name staging
mcp2cli link create --name prod

# Each alias routes to its own server with its own commands
dev ls
staging deploy --version 1.2.3
prod health-check
dev echo --message "test"
```

Each alias reads `argv[0]` and loads the matching config automatically. They
share the same binary but feel like completely different applications.

---

## 3. Subprocess MCP Server

**Scenario:** You want to run a local MCP server as a subprocess — no HTTP
server needed.

```bash
# Node.js MCP server
mcp2cli config init --name local \
  --transport stdio \
  --stdio-command npx \
  --stdio-args '@modelcontextprotocol/server-everything'

# Python MCP server
mcp2cli config init --name pyserver \
  --transport stdio \
  --stdio-command python \
  --stdio-args my_mcp_server.py

# Rust MCP server
mcp2cli config init --name rustserver \
  --transport stdio \
  --stdio-command ./target/release/my-mcp-server

mcp2cli use local
mcp2cli ls
```

Environment variables and working directory can be set in the config:

```yaml
server:
  transport: stdio
  stdio:
    command: node
    args: [server.js]
    cwd: /path/to/project
    env:
      NODE_ENV: development
      DATABASE_URL: "postgres://localhost/dev"
```

---

## 4. Namespace Grouping

**Scenario:** The MCP server uses dotted tool names for organization. mcp2cli
auto-groups them into nested subcommands.

```bash
# Server exposes: email.send, email.reply, email.draft.create,
#                 email.draft.list, email.labels.list, email.labels.add

email --help
# COMMANDS:
#   send       Send an email
#   reply      Reply to an email
#   draft      Draft management
#     create     Create a draft
#     list       List drafts
#   labels     Label management
#     list       List labels
#     add        Add a label
#   get        Fetch a resource by URI
#   auth       Authentication
#   ...

email send --to user@example.com --subject "Hello" --body "Message"
email draft create --subject "WIP" --body "Work in progress"
email labels add --message-id msg_456 --label urgent
```

Grouping rules:
- ≥2 capabilities sharing a prefix form a subcommand group
- Dots, slashes, underscores, and hyphens are treated as separators

---

## 5. Scripting and Automation

**Scenario:** You want to use mcp2cli in shell scripts, cron jobs, or
automation pipelines.

### JSON output for parsing

```bash
# Get capabilities as JSON
work --json ls | jq '.data.items[].id'

# Call a tool and extract result
RESULT=$(work --json get-user --id 123 | jq -r '.data.result')
echo "User: $RESULT"

# Check auth status programmatically
AUTH_STATE=$(work --json auth status | jq -r '.data.auth_session.state')
if [ "$AUTH_STATE" != "authenticated" ]; then
  echo "Not authenticated, logging in..."
  work auth login
fi

# Parse doctor output
work --json doctor | jq '{server: .data.server, auth: .data.auth_session.state}'
```

### NDJSON for streaming

```bash
work --output ndjson long-running-task --steps 10 |
  while IFS= read -r line; do
    echo "$line" | jq -c '{type: .type, message: .message}'
  done
```

### Chaining commands

```bash
# Discover, validate, then execute
work doctor && work ls && work deploy --env staging

# Batch operations
for user_id in 1 2 3 4 5; do
  work --json get-user --id "$user_id" >> users.jsonl
done

# Conditional execution
if work --json ls | jq -e '.data.items[] | select(.id == "deploy")' > /dev/null; then
  work deploy --version latest
else
  echo "Server does not have deploy capability"
fi
```

### Non-blocking patterns

```bash
# Fire multiple background jobs
for dataset in sales marketing engineering; do
  work analyze --dataset "$dataset" --background
done

# Wait for all to complete
work jobs list | grep running | while read -r job_id rest; do
  work jobs wait "$job_id"
done
```

---

## 6. Long-Running Operations

**Scenario:** You need to run operations that take minutes or hours —
analysis, data processing, batch imports.

### Start a background job

```bash
work analyze-dataset --dataset q4-2025 --background
# → Job created: job-abc-123
```

### Monitor progress

```bash
work jobs watch --latest          # Watch live progress events
work jobs show --latest           # Poll status
work jobs show job-abc-123
work jobs wait --latest           # Block until done
```

### Manage jobs

```bash
work jobs list
work jobs cancel job-abc-123
```

### Cross-session persistence

Jobs persist on disk — you can exit the terminal and check results later:

```bash
# In one terminal:
work big-import --background

# Later, in another terminal:
work jobs show --latest
# status: completed
# result: "Imported 50,000 records"
```

---

## 7. Complex Argument Payloads

**Scenario:** A tool requires deeply nested arguments that are tedious to
type inline.

### JSON-typed flags

```bash
work configure \
  --labels '["production","us-west"]' \
  --limits '{"cpu":"2","memory":"4Gi"}'
```

### File-based payloads

```bash
cat > deploy-config.json << 'EOF'
{
  "environment": "production",
  "config": {
    "replicas": 5,
    "image": "myapp:2.1.0",
    "labels": ["production", "us-west"],
    "resources": { "cpu": "1", "memory": "2Gi" }
  }
}
EOF

work deploy --args-file deploy-config.json
```

### Layered overrides

Combine argument sources — they merge with later sources winning:

```bash
work deploy \
  --args-file deploy-config.json \
  --args-json '{"config":{"replicas":10}}' \
  --environment canary
```

---

## 8. Server-Initiated Elicitation

**Scenario:** A tool asks for additional input during execution — the server
needs confirmation, credentials, or details it can't determine upfront.

```bash
work trigger-deployment
```

The server sends an `elicitation/create` request. mcp2cli prompts
interactively:

```text
--- elicitation request ---
Deployment requires additional confirmation:
  Target region (AWS region) [required]: us-east-1
  Confirm (yes/no) [required]: yes
  Max instances [default: 3]:
  Tags [comma-separated]: prod,web,v2
--- end elicitation ---
```

The response is sent back to the server and execution continues.

### Type handling

| Schema type | Input | Result |
|---|---|---|
| `boolean` | `yes`, `true`, `y`, `1` | `true` |
| `integer` | `42` | `42` |
| `number` | `3.14` | `3.14` |
| `array` | `a, b, c` | `["a", "b", "c"]` |
| `enum` | Matched by title | Corresponding const value |
| `string` | Any text | Used as-is |

---

## 9. Event-Driven Monitoring

**Scenario:** You want to capture runtime events (progress, job updates, auth
prompts, server log messages) for monitoring or dashboards.

### Multiple sinks simultaneously

```yaml
events:
  enable_stdio_events: true                      # Stderr (human-readable)
  http_endpoint: "http://monitoring:9090/events"  # HTTP webhook (POST JSON)
  local_socket_path: "/tmp/mcp2cli-events.sock"  # Unix socket (NDJSON)
  sse_endpoint: "127.0.0.1:9091"                 # SSE server
  command: "logger -t mcp2cli '${MCP_EVENT_MESSAGE}'" # Shell command
```

All five sinks receive every event. Use stderr for development, HTTP for
production alerting, sockets for local IPC, SSE for web dashboards, and
command exec for custom integrations.

### Server notification events

During tool calls, the server may send real-time notifications:

```bash
$ work analyze-dataset --dataset q4-2025
[work] analyze-1 1/5 Loading dataset...
[work] analyze-1 2/5 Parsing records...
[work] server debug (db): Query executed in 42ms
[work] analyze-1 3/5 Computing aggregates...
[work] analyze-1 5/5 Complete
```

Progress notifications (`notifications/progress`), log messages
(`notifications/message`), and capability change signals
(`notifications/{tools,resources,prompts}/list_changed`) are all delivered
through the event broker.

### Command execution sink

Run arbitrary commands for each event with environment variable interpolation:

```yaml
# Desktop notification
events:
  command: "notify-send 'mcp2cli' '${MCP_EVENT_MESSAGE}'"

# Forward to webhook
events:
  command: "curl -s -X POST http://hooks/mcp -d \"${MCP_EVENT_JSON}\""

# Send to syslog
events:
  command: "logger -t mcp2cli '${MCP_EVENT_MESSAGE}'"

# Conditional: only alert on errors
events:
  command: "[ \"$MCP_EVENT_TYPE\" = 'server_log' ] && logger -p user.err '${MCP_EVENT_MESSAGE}'"
```

Available environment variables:
- `MCP_EVENT_TYPE` — event type (info, progress, server_log, job_update, auth_prompt, list_changed)
- `MCP_EVENT_JSON` — full JSON-serialized event
- `MCP_EVENT_APP_ID` — the app_id field
- `MCP_EVENT_MESSAGE` — human-readable one-line message

### Listening to events

```bash
# Unix socket
socat UNIX-LISTEN:/tmp/mcp2cli-events.sock,fork - | jq .

# SSE
curl -N http://127.0.0.1:9091
```

### Capability change detection

When the server signals that its tool/resource/prompt list has changed:

```bash
$ work long-running-task
[work] server tools have changed; run 'ls' to refresh
```

A stale marker file is written so the next `ls` command forces a live
re-discovery instead of using the cache.

---

## 10. Server-Initiated Sampling

**Scenario:** A tool needs a model/AI response during execution — the server
sends `sampling/createMessage` to the client.

```bash
$ work generate-code --spec api-spec.yaml
--- sampling request ---
The server requests a model response.
Model hint: claude-3-5-sonnet
System: You are an expert code generator
Max tokens: 2000

Messages:
  [user] Generate a REST controller for the given API spec

Your response (or 'decline' to reject): Here is the controller implementation...
--- end sampling ---

title: Code Generation
result: Generated src/controllers/api.ts
```

mcp2cli advertises `sampling` capability during initialization. The user
acts as a human-in-the-loop model — seeing exactly what the server asks
and deciding what to respond.

### Declining a sampling request

Type `decline` or press Enter with no input to reject:

```bash
Your response (or 'decline' to reject): decline
```

The server receives a JSON-RPC error (-32600) and can handle the rejection
gracefully.

---

## 11. Offline and Disconnected Work

**Scenario:** The server is temporarily unreachable but you need to see what
capabilities are available.

### Discovery cache fallback

```bash
# These work even when the server is down (from cache)
work ls
work ls --tools
work ls --resources
```

### What works offline

- Listing capabilities (`ls`)
- Viewing job records (`jobs list`, `jobs show`)
- Auth status check (`auth status`)
- Doctor diagnostics (`doctor`)

### What requires connectivity

- Calling tools
- Reading resources
- Running prompts
- Job sync (`jobs wait`, `jobs cancel`)
- Auth flows (`auth login`)

---

## 12. CI/CD Integration

**Scenario:** You want to call MCP server tools from CI/CD pipelines —
GitHub Actions, GitLab CI, Jenkins.

### Setup in CI

```bash
cargo install --path .

mcp2cli config init --name ci \
  --transport streamable_http \
  --endpoint "$MCP_SERVER_ENDPOINT"
mcp2cli use ci

echo "$MCP_TOKEN" | mcp2cli auth login
```

### CI pipeline steps

```bash
# Validate
mcp2cli doctor

# Deploy
DEPLOY_RESULT=$(mcp2cli --json deploy \
  --version "$CI_COMMIT_SHA" \
  --environment staging)
echo "$DEPLOY_RESULT" | jq -e '.data.success' || exit 1

# Long operations
mcp2cli run-tests --suite full --background
mcp2cli jobs wait --latest

JOB_STATUS=$(mcp2cli --json jobs show --latest | jq -r '.data.status')
if [ "$JOB_STATUS" != "completed" ]; then
  echo "Tests failed"
  exit 1
fi
```

### Event forwarding to CI logs

```yaml
events:
  enable_stdio_events: true    # Events appear in CI log output
```

---

## 13. Profile Overlays

**Scenario:** You want the CLI to feel polished — renaming awkward tool names,
hiding internal tools, grouping related commands.

### Add a profile

```yaml
# In configs/work.yaml
profile:
  display_name: "Work Toolkit"
  aliases:
    long-running-operation: lro
    get-tiny-image: image
    annotated-message: annotate
  hide:
    - print-env
    - debug-probe
  groups:
    analysis:
      - analyze-data
      - generate-report
      - export-csv
  flags:
    echo:
      message: msg
  resource_verb: fetch
```

### Result

```bash
work echo --msg hello          # Renamed flag
work lro --duration 5          # Shortened command name
work image                     # Friendly name
work analysis analyze-data ... # Custom grouping
work fetch demo://resource/... # Custom resource verb
```

---

## 14. Email MCP Server

**Scenario:** An email MCP server exposes tools for sending, reading, and
managing email.

### Server capabilities

- **Tools:** `send`, `reply`, `forward`, `archive`, `labels.add`, `labels.remove`, `draft.create`, `draft.send`
- **Resources:** `mail://inbox`, `mail://sent`, `mail://draft/123`
- **Resource templates:** `mail://search?q={query}`, `mail://message/{id}`
- **Prompts:** `summarize-thread`, `compose-reply`, `triage-inbox`

### Usage

```bash
email send --to user@example.com --subject "Hello" --body "Message body"
email reply --thread-id th_123 --body "Thanks for the update"
email labels add --message-id msg_456 --label urgent
email draft create --subject "Draft" --body "Work in progress"

email get mail://inbox
email search --query "invoice 2026"
email message msg_789

email summarize-thread --thread-id th_123
email compose-reply --thread-id th_123 --style professional
email triage-inbox

email send --to team@example.com --subject "Batch report" --background
email jobs watch --latest
```

Dotted tool names auto-group: `labels.add` → `email labels add`,
`draft.create` → `email draft create`.

---

## 15. GitHub MCP Server

**Scenario:** A GitHub MCP server exposes repository, issue, and PR tools.

### Server capabilities

- **Tools:** `repos.list`, `repos.create`, `issues.list`, `issues.create`, `issues.comment`, `pr.list`, `pr.create`, `pr.review`, `pr.merge`
- **Resource templates:** `gh://repo/{owner}/{name}`, `gh://issue/{owner}/{name}/{number}`
- **Prompts:** `review-pr`, `draft-issue`

### Usage

```bash
gh repos list --org my-org --limit 10
gh repos create --name new-project --private true
gh issues list --repo owner/repo --state open
gh issues create --repo owner/repo --title "Bug" --body "Details..."
gh issues comment --repo owner/repo --number 42 --body "Fixed in v2"
gh pr list --repo owner/repo --state open
gh pr create --repo owner/repo --title "Feature" --head feature-branch
gh pr review --repo owner/repo --number 15 --approve true
gh pr merge --repo owner/repo --number 15 --method squash

# Resource templates
gh repo owner/my-project
gh issue owner/my-project 42

# AI-powered prompts
gh review-pr --repo owner/repo --number 15 --focus security
gh draft-issue --title "Performance regression" --context "Load test results..."
```

---

## 16. Database MCP Server

**Scenario:** A database MCP server exposes query, schema inspection, and
migration tools.

```bash
db query --sql "SELECT * FROM users LIMIT 10"
db query --sql "SELECT count(*) FROM orders WHERE status='pending'"

# Schema inspection (resources)
db get "schema://tables"
db get "schema://tables/users"

# Migration tools
db migrate --direction up --steps 1
db migrate --direction down --steps 1 --background
db jobs watch --latest

# AI prompts
db explain-query --sql "SELECT u.name, COUNT(o.id) FROM users u JOIN orders o..."
db suggest-index --table orders --column status
```

---

## 17. Debugging and Diagnostics

**Scenario:** Something isn't working. You need to diagnose connectivity,
auth, or capability issues.

### Doctor

```bash
work doctor
```

Output:
```yaml
config: work
profile: bridge
transport: streamable_http
server: my-server 2.1.0
auth: authenticated
negotiated: protocol 2025-03-26 with 5 capability groups cached
inventory: 14 tools, 3 resources, 2 prompts cached
```

### Inspect

```bash
work inspect
```

Full server capability response: protocol version, capabilities, server info.

### Ping

```bash
work ping
```

Server liveness check with latency measurement.

### Verbose logging

```bash
MCP2CLI_LOGGING__LEVEL=debug work echo --message test 2>debug.log
MCP2CLI_LOGGING__LEVEL=trace work doctor 2>trace.log
```

### Auth debugging

```bash
work auth status
```

---

## 18. Resource Templates

**Scenario:** The server exposes parameterized resources (URI templates) that
become first-class commands.

### Single-parameter templates → positional argument

```bash
# Template: greeting/{name}
work greeting Alice
# → resources/read with URI: greeting/Alice
```

### Multi-parameter templates → flags

```bash
# Template: mail://search?q={query}&folder={folder}
work mail-search --query invoice --folder inbox
# → resources/read with URI: mail://search?q=invoice&folder=inbox
```

### Concrete resources via `get`

```bash
work get "demo://resource/readme.md"
work get "demo://resource/static/document/architecture.md"
```

---

## 19. Tab Completion and Server Completions

**Scenario:** You want server-assisted completions for argument values.

```bash
# Request completions for a tool argument
work complete ref/tool echo message "hel"

# Complete a prompt argument
work complete ref/prompt compose-reply style "prof"

# Set server log level
work log debug
work log info
```

---

## 20. Multi-User Shared Infrastructure

**Scenario:** Multiple team members use the same MCP server with shared configs
via version control.

```bash
# Store configs in a git repo
git init team-mcp-configs
cd team-mcp-configs

cat > staging.yaml << 'EOF'
schema_version: 1
app:
  profile: bridge
server:
  display_name: Staging API
  transport: streamable_http
  endpoint: https://staging.internal.example.com/mcp
defaults:
  output: human
logging:
  level: warn
  format: pretty
  outputs: [{kind: stderr}]
auth:
  browser_open_command: null
events:
  enable_stdio_events: true
EOF

git add . && git commit -m "Add staging config"

# Point mcp2cli to the shared dir
export MCP2CLI_CONFIG_DIR=/path/to/team-mcp-configs
mcp2cli link create --name staging
staging ls
```

Each user authenticates independently — tokens are stored in the per-user
data directory, not in the shared config.

---

## 21. Demo and Learning Mode

**Scenario:** You want to learn mcp2cli without setting up a real server.

### Using the reference server

```bash
# Start the reference MCP server
npx @modelcontextprotocol/server-everything streamableHttp
# → Running on http://127.0.0.1:3001/mcp

# In another terminal:
mcp2cli config init --name everything \
  --transport streamable_http \
  --endpoint http://127.0.0.1:3001/mcp
mcp2cli use everything

# Or as a stdio server (no separate process needed):
mcp2cli config init --name everything-stdio \
  --transport stdio \
  --stdio-command npx \
  --stdio-args '@modelcontextprotocol/server-everything'
mcp2cli use everything-stdio

# Discover and use
mcp2cli ls
mcp2cli echo --message hello
mcp2cli add --a 5 --b 3
mcp2cli get-tiny-image
mcp2cli simple-prompt
mcp2cli complex-prompt --temperature 0.7 --style concise

# Diagnostics
mcp2cli doctor
mcp2cli inspect
mcp2cli ping

# Auth flows
mcp2cli auth login
mcp2cli auth status
mcp2cli auth logout

# Background jobs
mcp2cli long-running-operation --duration 5 --steps 3 --background
mcp2cli jobs watch --latest
```

---

## 22. Resource Subscriptions

**Scenario:** You want to be notified when a server resource changes — a config
file, a database table, or a monitoring endpoint — without polling.

### Subscribe to a resource

```bash
work subscribe "file:///project/config.yaml"
work subscribe "db://tables/users"
```

The server will send `notifications/resources/updated` whenever the resource
changes. Events arrive through configured sinks (stderr, webhook, socket, SSE).

### Monitor in a second terminal

```bash
# With SSE event sink enabled:
curl -N http://127.0.0.1:9091
# → data: {"type":"info","message":"resource updated: file:///project/config.yaml"}
```

### Unsubscribe when done

```bash
work unsubscribe "file:///project/config.yaml"
```

### Automation: react to resource changes

```bash
# Watch for updates and re-read when they arrive
work subscribe "config://app/settings"

# In another terminal, watch the SSE stream:
curl -sN http://127.0.0.1:9091 | while IFS= read -r line; do
  if echo "$line" | grep -q "resource updated"; then
    work --json get "config://app/settings" >> config-history.jsonl
  fi
done
```

### Use case: config hot-reload

```yaml
# In events config:
events:
  command: |
    [ "$MCP_EVENT_TYPE" = "info" ] && echo "$MCP_EVENT_MESSAGE" | grep -q "resource updated" && \
    systemctl reload my-service
```

---

## 23. Workspace Roots

**Scenario:** A code-aware MCP server needs to know which directories it should
operate on. The server sends a `roots/list` request, and mcp2cli responds with
the configured roots.

### Configure roots

```yaml
# In configs/code.yaml
server:
  transport: stdio
  stdio:
    command: ./code-analysis-server
  roots:
    - uri: "file:///home/user/project/src"
      name: "Source"
    - uri: "file:///home/user/project/tests"
      name: "Tests"
```

### How it works

During tool execution, the server may request `roots/list` to understand the
client's workspace. mcp2cli automatically responds with the configured roots.
The server can then scope its analysis, file search, or code generation to
those directories.

```bash
# The server uses roots to scope its work
code analyze --depth full
# Server internally calls roots/list → gets [src/, tests/]
# → Analyzes only those directories
```

### Multiple projects

```bash
# Frontend project
mcp2cli config init --name frontend --transport stdio --stdio-command ./lsp-server
# Add roots: src/, components/, public/

# Backend project
mcp2cli config init --name backend --transport stdio --stdio-command ./lsp-server
# Add roots: cmd/, internal/, pkg/

# Each alias reports different roots
frontend analyze --scope all    # Server sees frontend roots
backend analyze --scope all     # Server sees backend roots
```

---

## 24. Task Lifecycle Management

**Scenario:** A server supports the MCP task system for long-running operations.
You start a tool as a background task, monitor its progress, and retrieve the
result later — even from a different terminal session.

### Start a background task

```bash
work analyze-dataset --dataset q4-2025 --background
# → Task accepted (task-abc-123)
# Job created: job-1
```

When `--background` is used, mcp2cli sends `_meta.task` in the tool call
request. If the server supports tasks, it returns a task ID immediately instead
of blocking.

### Check task status

```bash
work jobs show --latest
# Queries tasks/get on the server for live status:
#   job-1: task-abc-123
#   status: working
#   message: "Processing 50,000 records..."
```

### Wait for completion

```bash
work jobs wait --latest
# Polls tasks/get every 2 seconds
# When complete, calls tasks/result for the final output:
#   status: completed
#   result: { "records_processed": 50000, "errors": 0 }
```

### Watch live status

```bash
work jobs watch --latest
# Polls every 1 second, prints each status change:
#   [job-1] working — Loading dataset...
#   [job-1] working — Processing records (25,000/50,000)...
#   [job-1] working — Computing aggregates...
#   [job-1] completed
```

### Cancel a running task

```bash
work jobs cancel --latest
# Sends tasks/cancel to the server
# → task-abc-123 cancelled
```

### Cross-session persistence

Tasks persist on disk. Start in one terminal, check from another:

```bash
# Terminal 1:
work train-model --epochs 100 --background

# Terminal 2 (later, even after Terminal 1 closed):
work jobs show --latest
work jobs wait --latest
```

### CI/CD with tasks

```bash
# Start a long deployment as a task
work deploy --image myapp:2.0 --environment staging --background

# Poll until complete
work jobs wait --latest
STATUS=$(work --json jobs show --latest | jq -r '.data.status')
[ "$STATUS" = "completed" ] || exit 1
```

---

## 25. Request Cancellation

**Scenario:** You started a long-running tool call and need to abort it
gracefully — notifying the server to stop work rather than just killing the
connection.

### Cancel from the CLI

When the server supports long-running operations, you can cancel in-flight
requests:

```bash
# Start a long tool call
work process-batch --size 1000000

# Press Ctrl+C — mcp2cli sends notifications/cancelled to the server
# The server can then abort the operation gracefully
```

### Server-initiated cancellation

If the server cancels a pending request it sent to you (e.g. an elicitation
or sampling request), mcp2cli handles the `notifications/cancelled`
notification and logs the reason:

```json
[work] request cancelled by server: timeout exceeded
```

### Programmatic cancellation

```bash
# Start a background job
work big-export --background
JOB_ID=$(work --json jobs list | jq -r '.data.jobs[-1].id')

# Cancel it
work jobs cancel "$JOB_ID"
# Sends tasks/cancel → notifications/cancelled to the server
```

---

## 26. Infrastructure Provisioning Server

**Scenario:** An infrastructure-as-code MCP server exposes provisioning,
scaling, and monitoring tools. Operations can take minutes, making the task
system essential.

### Server capabilities

- **Tools:** `provision`, `scale`, `destroy`, `status`, `logs.tail`, `deploy.rollout`, `deploy.rollback`
- **Resources:** `infra://clusters`, `infra://cluster/{id}`, `infra://costs`
- **Prompts:** `incident-response`, `capacity-plan`

### Usage

```bash
# Provision a new cluster (long-running → background task)
infra provision --region us-east-1 --size medium --background
infra jobs watch --latest
# → [job-1] working — Creating VPC...
# → [job-1] working — Launching instances...
# → [job-1] working — Configuring load balancer...
# → [job-1] completed

# Check cluster status
infra get "infra://cluster/cls-789"

# Scale up (quick operation)
infra scale --cluster cls-789 --replicas 5

# Rolling deployment
infra deploy rollout --cluster cls-789 --image myapp:3.0 --background
infra jobs watch --latest

# Emergency rollback
infra deploy rollback --cluster cls-789

# Subscribe to cost alerts
infra subscribe "infra://costs"
# → Receive notifications when costs exceed thresholds

# AI-assisted incident response
infra incident-response --cluster cls-789 --symptoms "high latency, 5xx errors"
```

---

## 27. Data Pipeline Orchestration

**Scenario:** A data engineering MCP server orchestrates ETL pipelines,
transformations, and quality checks. Multiple long-running jobs run in parallel.

### Server capabilities

- **Tools:** `pipeline.run`, `pipeline.status`, `transform`, `validate`, `export`
- **Resources:** `data://datasets`, `data://schema/{dataset}`
- **Prompts:** `generate-transform`, `diagnose-quality`

### Usage

```bash
# Start multiple pipelines in parallel
data pipeline run --name ingest-sales --background
data pipeline run --name ingest-marketing --background
data pipeline run --name ingest-support --background

# Watch all jobs
data jobs list
# ID    Status    Name
# 1     working   ingest-sales
# 2     working   ingest-marketing
# 3     completed ingest-support

# Wait for a specific job
data jobs wait 1

# Check schema
data get "data://schema/sales"

# Validate data quality
data validate --dataset sales --rules strict --background
data jobs watch --latest

# Subscribe to dataset changes for a downstream trigger
data subscribe "data://datasets/sales"

# Use server-side roots to scope operations
# Config:
#   roots:
#     - uri: "file:///data/warehouse"
#       name: "Data Warehouse"
data transform --input sales --output sales_enriched
```
