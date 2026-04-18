# From Zero to Production

*A complete end-to-end guide: install mcp2cli, connect to MCP servers, configure for production, and set up monitoring.*

---

## Phase 1: Install & Explore

### Install

```bash
# From source
git clone https://github.com/mcp2cli/source-code.git
cd source-code
cargo install --path .

# Verify
mcp2cli --version
```

### Try the Demo

No server needed — the built-in demo mode lets you explore:

```bash
mcp2cli config init --name demo --app bridge \
  --transport streamable_http --endpoint https://demo.invalid/mcp

mcp2cli use demo

# Discover capabilities
mcp2cli ls

# Call tools
mcp2cli echo --message "hello world"
mcp2cli add --a 5 --b 3

# Read resources
mcp2cli get demo://resource/readme.md

# Health check
mcp2cli doctor
```

---

## Phase 2: Connect to Your Server

### For HTTP Servers

```bash
mcp2cli config init --name myserver --app bridge \
  --transport streamable_http \
  --endpoint http://your-server:3001/mcp

mcp2cli use myserver
mcp2cli doctor           # Verify connection
mcp2cli ls               # Discover capabilities
```

### For Stdio Servers

```bash
mcp2cli config init --name myserver --app bridge \
  --transport stdio \
  --stdio-command your-server-binary \
  --stdio-args '--config=config.yaml'

mcp2cli use myserver
mcp2cli doctor
mcp2cli ls
```

### Quick Test Without Config

```bash
# Just point and go
mcp2cli --url http://your-server:3001/mcp ls
mcp2cli --stdio "your-server-binary" ls
```

---

## Phase 3: Configure for Your Team

### Create a Production Config

```yaml
# configs/prod.yaml
schema_version: 1

app:
  profile: bridge

server:
  display_name: Production API Server
  transport: streamable_http
  endpoint: https://mcp-api.company.com/mcp

defaults:
  output: human
  timeout_seconds: 120

logging:
  level: warn
  format: pretty
  outputs:
    - kind: stderr

auth:
  browser_open_command: xdg-open

events:
  enable_stdio_events: true
  http_endpoint: "http://monitoring.internal:9090/mcp-events"

profile:
  display_name: "Production CLI"
  aliases:
    long-running-operation: lro
  hide:
    - debug-internal
    - test-tool
  groups:
    data:
      - query
      - export
      - import
    admin:
      - user-create
      - user-delete
      - config-update
  flags:
    query:
      database: db
      collection: col
  resource_verb: fetch
```

### Set Up Aliases

```bash
mcp2cli link create --name prod
mcp2cli link create --name prod --dir /usr/local/bin    # System-wide

# Now use it
prod ls
prod doctor
prod query --db users --col profiles
```

### Multi-Environment Setup

```bash
# Development
mcp2cli config init --name dev --transport stdio \
  --stdio-command ./target/debug/my-server

# Staging
mcp2cli config init --name staging --transport streamable_http \
  --endpoint https://staging-mcp.company.com/mcp

# Production
# (use the YAML above)

# Create aliases for each
mcp2cli link create --name dev
mcp2cli link create --name staging
mcp2cli link create --name prod
```

---

## Phase 4: Authentication

```bash
# Login (stores token locally)
prod auth login
# Enter token: <paste-your-token>

# Verify
prod auth status
# → Auth state: active

# All subsequent commands use the stored token
prod ls
prod query --db users
```

---

## Phase 5: Optimize Performance

### Enable Daemon Mode

For stdio servers or frequent usage:

```bash
# Start daemon (initializes connection once)
mcp2cli daemon start prod

# All commands now route through the daemon (~50ms instead of ~2s)
prod ls
prod echo --message test
prod query --db users

# Check daemon status
mcp2cli daemon status prod

# Stop when done
mcp2cli daemon stop prod
```

### Tune Timeouts

```yaml
# configs/prod.yaml
defaults:
  timeout_seconds: 120     # 2 min for normal operations

# For slow operations, override per-command:
# prod --timeout 600 export --dataset full
# prod --timeout 0 long-running-import
```

---

## Phase 6: CI/CD Integration

### GitHub Actions

```yaml
name: Deploy via MCP
on:
  push:
    branches: [main]

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Install mcp2cli
        run: cargo install --path .
      
      - name: Configure
        run: |
          mcp2cli config init --name prod --transport streamable_http \
            --endpoint "${{ secrets.MCP_ENDPOINT }}"
          mcp2cli use prod
          
      - name: Authenticate
        run: |
          echo "${{ secrets.MCP_TOKEN }}" | mcp2cli auth login
          
      - name: Pre-deploy health check
        run: prod --timeout 10 doctor
        
      - name: Deploy
        run: |
          prod --json deploy \
            --version "${{ github.sha }}" \
            --environment production \
            --timeout 300
            
      - name: Post-deploy verification
        run: |
          prod --timeout 10 ping
          HEALTH=$(prod --json doctor | jq -r '.summary')
          echo "Post-deploy health: $HEALTH"
```

### Shell Script Pipeline

```bash
#!/bin/bash
set -euo pipefail

# Pre-flight
prod --timeout 10 doctor || { echo "Server unhealthy"; exit 1; }

# Deploy with background job
RESULT=$(prod --json deploy --version "$VERSION" --background)
JOB_ID=$(echo "$RESULT" | jq -r '.data.job_id')

# Wait
timeout 600 prod jobs wait "$JOB_ID" || {
  prod jobs cancel "$JOB_ID"
  exit 1
}

# Verify
prod --timeout 5 ping
echo "Deploy complete"
```

---

## Phase 7: Monitoring & Observability

### Event Sinks

```yaml
events:
  enable_stdio_events: true
  
  # Send events to monitoring system
  http_endpoint: "http://monitoring.internal:9090/mcp-events"
  
  # Stream to local dashboard  
  sse_endpoint: "127.0.0.1:9091"
  
  # Log to file via command
  command: |
    echo "$(date -Iseconds) [${MCP_EVENT_TYPE}] ${MCP_EVENT_MESSAGE}" \
      >> /var/log/mcp2cli/events.log
```

### Health Check Probe

```bash
#!/bin/bash
# health-probe.sh — run every 30s from cron/systemd

if prod --timeout 5 --json ping >/dev/null 2>&1; then
  echo "mcp_server_up 1" | curl -s --data-binary @- http://pushgateway:9091/metrics/job/mcp
else
  echo "mcp_server_up 0" | curl -s --data-binary @- http://pushgateway:9091/metrics/job/mcp
  # Alert
  curl -s -X POST "$SLACK_WEBHOOK" -d '{"text": "🔴 MCP server down"}'
fi
```

### Structured Logging

```yaml
logging:
  level: info                          # info for production visibility
  format: json                         # JSON for log aggregation
  outputs:
    - kind: stderr
    - kind: file
      path: /var/log/mcp2cli/mcp2cli.log
```

---

## Phase 8: Team Onboarding

### Shared Config Repository

```bash
# Check configs into your repo
mkdir -p mcp-configs
cp ~/.local/share/mcp2cli/configs/*.yaml mcp-configs/

# Team members use:
export MCP2CLI_CONFIG_DIR=./mcp-configs
mcp2cli config list
```

### Profile Overlays for Usability

Create user-friendly command surfaces:

```yaml
profile:
  display_name: "Acme Platform CLI"
  aliases:
    execute-sql-query: query           # Short aliases
    create-deployment: deploy
    get-application-status: status
  groups:
    data:                              # Logical grouping
      - query
      - export
      - import
    deploy:
      - deploy
      - rollback
      - status
```

### Documentation for Your Team

```bash
# Show your team what's available
prod ls
prod --help
prod data --help
prod deploy --help
```

---

## Production Checklist

| Item | Command | Status |
|------|---------|--------|
| Config created | `mcp2cli config show --name prod` | ☐ |
| Connection verified | `prod doctor` | ☐ |
| Auth configured | `prod auth status` | ☐ |
| Aliases created | `prod ls` | ☐ |
| Daemon running | `mcp2cli daemon status prod` | ☐ |
| Timeouts configured | Check `defaults.timeout_seconds` | ☐ |
| Events configured | Check `events` section | ☐ |
| Health probe active | Cron/systemd probe running | ☐ |
| CI/CD integration | Pipeline deploys with `prod` | ☐ |
| Team access | Shared config repo | ☐ |
| Profile customized | Commands are intuitive | ☐ |

---

## See Also

- [Getting Started](../getting-started.md) — quick start for new users
- [Configuration Reference](../reference/config-reference.md) — every config option
- [Daemon Mode](../features/daemon-mode.md) — connection keep-alive
- [Multi-Server Workflows](multi-server-workflows.md) — orchestrating multiple servers
- [Shell Scripting with MCP](shell-scripting-mcp.md) — automation patterns
