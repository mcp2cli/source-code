# Shell Scripting with MCP

*Build automation pipelines, CI/CD workflows, and monitoring scripts that talk to MCP servers.*

---

## Foundations

Every mcp2cli command supports `--json` output with a consistent envelope. This makes it composable with standard Unix tools.

### The JSON Envelope

```json
{
  "app_id": "email",
  "command": "invoke",
  "summary": "called echo",
  "lines": ["..."],
  "data": { /* command-specific structured data */ }
}
```

### Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Runtime error (server error, timeout, etc.) |
| `2` | CLI usage error (bad flags, missing args) |

---

## Common Patterns

### Extract Data with jq

```bash
# Tool names
email --json ls --tools | jq -r '.data.items[].id'

# Tool result content
email --json echo --message hello | jq -r '.data.content[0].text'

# Server health
email --json doctor | jq '.data.server'

# Capability count
TOOLS=$(email --json ls --tools | jq '.data.items | length')
echo "Server has $TOOLS tools"
```

### Conditional Logic

```bash
# Check if server is up
if email --timeout 5 ping >/dev/null 2>&1; then
  echo "Server is up"
else
  echo "Server is down"
  exit 1
fi

# Check if a specific tool exists
if email --json ls --tools | jq -e '.data.items[] | select(.id == "send")' >/dev/null 2>&1; then
  echo "Send tool available"
fi
```

### Looping Over Tools

```bash
# Call every tool with a smoke test
email --json ls --tools | jq -r '.data.items[].id' | while read tool; do
  echo -n "Testing $tool... "
  if email --json --timeout 10 "$tool" >/dev/null 2>&1; then
    echo "OK"
  else
    echo "FAIL"
  fi
done
```

### Piping Between Commands

```bash
# Get resource list, then read each one
email --json ls --resources | jq -r '.data.items[].id' | while read uri; do
  email --json get "$uri" > "resources/$(echo $uri | tr '/:' '__').json"
done
```

---

## CI/CD Recipes

### Deployment Pipeline

```bash
#!/bin/bash
set -euo pipefail

VERSION="${1:?Usage: $0 <version>}"
CONFIG="prod"

echo "=== Deploying v$VERSION ==="

# Pre-flight checks
echo "Running health check..."
"$CONFIG" --timeout 10 doctor || { echo "Server unhealthy"; exit 1; }

# Deploy
echo "Submitting deployment..."
RESULT=$("$CONFIG" --json deploy --version "$VERSION" --background)
JOB_ID=$(echo "$RESULT" | jq -r '.data.job_id')
echo "Job: $JOB_ID"

# Wait with timeout
echo "Waiting for completion..."
if timeout 600 "$CONFIG" --json jobs wait "$JOB_ID"; then
  echo "✅ Deploy v$VERSION succeeded"
else
  echo "❌ Deploy timed out"
  "$CONFIG" jobs cancel "$JOB_ID"
  exit 1
fi

# Post-deploy verification
echo "Verifying..."
"$CONFIG" --timeout 5 ping
echo "✅ Server responding after deploy"
```

### Nightly Regression Test

```bash
#!/bin/bash
# Run nightly in cron: 0 2 * * * /scripts/nightly-mcp-test.sh

LOG="/var/log/mcp-nightly/$(date +%Y%m%d).log"
PASS=0
FAIL=0

test_tool() {
  local tool="$1"
  shift
  if email --json --timeout 30 "$tool" "$@" >> "$LOG" 2>&1; then
    ((PASS++))
  else
    echo "FAIL: $tool" >> "$LOG"
    ((FAIL++))
  fi
}

echo "=== Nightly Test $(date) ===" >> "$LOG"

# Discovery
test_tool ls
test_tool ls --tools
test_tool ls --resources

# Core tools
test_tool search --query "nightly-test-$(date +%s)"
test_tool draft.list
test_tool ping

# Report
echo "Results: $PASS passed, $FAIL failed" >> "$LOG"

if [ "$FAIL" -gt 0 ]; then
  # Send alert (Slack, email, etc.)
  curl -s -X POST "$SLACK_WEBHOOK" \
    -d "{\"text\": \"🔴 MCP Nightly: $FAIL tests failed. See $LOG\"}"
fi
```

### Docker Health Check

```dockerfile
HEALTHCHECK --interval=30s --timeout=10s --retries=3 \
  CMD mcp2cli --url http://localhost:3001/mcp --timeout 5 ping
```

---

## Environment Variable Patterns

### Per-Environment Configs

```bash
# Development
export MCP2CLI_CONFIG_DIR=./configs/dev
email send --to dev@example.com --subject "Build 1.0"

# Staging  
export MCP2CLI_CONFIG_DIR=./configs/staging
email send --to staging@example.com --subject "Build 1.0"

# Production
export MCP2CLI_CONFIG_DIR=./configs/prod
email send --to prod@example.com --subject "Build 1.0"
```

### Dynamic Endpoint Override

```bash
# Point config at a different server without changing config files
MCP2CLI_SERVER__ENDPOINT=https://canary.api/mcp email --json doctor
```

### Secret Injection

```bash
# Inject secrets from a vault
export API_KEY=$(vault kv get -field=api_key secret/mcp-server)
mcp2cli --stdio "my-server" --env "API_KEY=$API_KEY" echo --message test
```

---

## Monitoring Integration

### Event-Driven Monitoring

```yaml
# Config for monitored server
events:
  http_endpoint: "http://prometheus-pushgateway:9091/metrics/job/mcp"
  command: |
    echo "mcp_event_total{type=\"${MCP_EVENT_TYPE}\",app=\"${MCP_EVENT_APP_ID}\"} 1" | \
      curl -s --data-binary @- http://prometheus-pushgateway:9091/metrics/job/mcp
```

### Probe Script for Uptime Monitoring

```bash
#!/bin/bash
# Prometheus-compatible probe
START=$(date +%s%N)
if mcp2cli --url "$MCP_ENDPOINT" --timeout 10 --json ping >/dev/null 2>&1; then
  END=$(date +%s%N)
  LATENCY=$(( (END - START) / 1000000 ))
  echo "mcp_probe_success 1"
  echo "mcp_probe_latency_ms $LATENCY"
else
  echo "mcp_probe_success 0"
fi
```

---

## NDJSON Streaming

For real-time pipelines, use NDJSON output:

```bash
# Stream events to a log aggregator
email --output ndjson ls 2>/dev/null | \
  while IFS= read -r line; do
    echo "$line" | curl -s -X POST http://log-aggregator/ingest -d @-
  done
```

---

## Error Handling Patterns

```bash
# Retry with backoff
retry() {
  local max_attempts=3
  local delay=2
  local attempt=1
  
  while [ $attempt -le $max_attempts ]; do
    if "$@"; then
      return 0
    fi
    echo "Attempt $attempt failed, retrying in ${delay}s..."
    sleep $delay
    delay=$((delay * 2))
    attempt=$((attempt + 1))
  done
  
  echo "All $max_attempts attempts failed"
  return 1
}

# Usage
retry email --timeout 30 send --to user@example.com --subject "Build 2.0"
```

### Capture and Parse Errors

```bash
OUTPUT=$(email --json echo --message hello 2>&1)
EXIT_CODE=$?

if [ $EXIT_CODE -ne 0 ]; then
  echo "Command failed with exit code $EXIT_CODE"
  echo "Error: $OUTPUT"
else
  echo "$OUTPUT" | jq '.data.content'
fi
```

---

## Parallel Execution

```bash
# Run multiple commands in parallel
email --json --timeout 30 tool-a --arg x &
email --json --timeout 30 tool-b --arg y &
email --json --timeout 30 tool-c --arg z &
wait
echo "All commands completed"
```

For many parallel commands, use GNU `parallel`:

```bash
email --json ls --tools | jq -r '.data.items[].id' | \
  parallel -j4 "email --json --timeout 30 {} 2>/dev/null > /tmp/results/{}.json"
```

---

## See Also

- [Output Formats](../features/output-formats.md) — JSON envelope details
- [Request Timeouts](../features/request-timeouts.md) — timeout control
- [Event System](../features/event-system.md) — event sinks for monitoring
- [Testing MCP Servers](testing-mcp-servers.md) — testing-specific patterns
- [AI Agents + MCP via CLI](ai-agents-mcp-cli.md) — agent integration
