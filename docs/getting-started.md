# Getting Started with mcp2cli

Get from zero to running MCP commands in under 5 minutes.

---

## Prerequisites

- **Rust toolchain** (for building from source) or a pre-built binary
- An MCP server to connect to (or use the built-in demo mode)

---

## Install

```bash
# From source
cargo install --path .

# Verify
mcp2cli --version
```

---

## Your First Server Connection

### Option A: Demo Mode (no server needed)

The built-in demo mode uses a file-backed backend — perfect for learning without setting up a real server.

```bash
# Create a demo config
mcp2cli config init --name demo --app bridge \
  --transport streamable_http --endpoint https://demo.invalid/mcp

# Set it as active
mcp2cli use demo

# Discover capabilities
mcp2cli ls
```

### Option B: Local Stdio Server

Spawn a local MCP server as a subprocess:

```bash
# Create config pointing to a local server
mcp2cli config init --name local --app bridge --transport stdio \
  --stdio-command npx \
  --stdio-args '@modelcontextprotocol/server-everything'

mcp2cli use local
mcp2cli ls
```

### Option C: Remote HTTP Server

Connect to a running HTTP MCP server:

```bash
mcp2cli config init --name remote --app bridge \
  --transport streamable_http \
  --endpoint http://127.0.0.1:3001/mcp

mcp2cli use remote
mcp2cli ls
```

### Option D: Ad-Hoc (no config file)

Skip configuration entirely with `--url` or `--stdio`:

```bash
# HTTP server — just point and go
mcp2cli --url http://127.0.0.1:3001/mcp ls

# Stdio server — just run
mcp2cli --stdio "npx @modelcontextprotocol/server-everything" ls
```

---

## Explore What the Server Offers

```bash
# List everything
mcp2cli ls

# Filter by type
mcp2cli ls --tools         # Tools only
mcp2cli ls --resources     # Resources only
mcp2cli ls --prompts       # Prompts only

# Search
mcp2cli ls --filter echo
```

---

## Call a Tool

Server tools become typed CLI commands. Flags come directly from JSON Schema:

```bash
# Simple tool call
mcp2cli echo --message "hello world"

# Tool with multiple arguments
mcp2cli add --a 5 --b 3

# Tool with complex arguments
mcp2cli deploy --tags '["alpha","beta"]' --config '{"replicas":3}'
```

---

## Read a Resource

```bash
# By URI
mcp2cli get demo://resource/readme.md

# Resource template (parameterized)
mcp2cli user-profile --user-id 42
```

---

## Run a Prompt

```bash
mcp2cli simple-prompt
mcp2cli complex-prompt --temperature 0.7 --style concise
```

---

## Create a Symlink Alias

Make your server feel like a standalone application:

```bash
mcp2cli link create --name work

# Now use it directly
work ls
work echo --message "hello from alias"
work doctor
```

---

## JSON Output for Scripts

Every command supports structured JSON output:

```bash
# JSON envelope
work --json ls

# Pipe to jq
work --json echo --message hello | jq '.data'

# NDJSON for streaming
work --output ndjson ls
```

---

## What's Next?

| Goal | Read |
|------|------|
| Configure timeout, logging, events | [Configuration Reference](reference/config-reference.md) |
| Customize command names and grouping | [Profile Overlays](features/profile-overlays.md) |
| Run background jobs | [Background Jobs](features/background-jobs.md) |
| Set up CI/CD pipelines | [Shell Scripting with MCP](articles/shell-scripting-mcp.md) |
| Connect AI agents | [AI Agents + MCP via CLI](articles/ai-agents-mcp-cli.md) |
| Manage multiple servers | [Named Configs & Aliases](features/named-configs-and-aliases.md) |
| Keep connections warm | [Daemon Mode](features/daemon-mode.md) |
