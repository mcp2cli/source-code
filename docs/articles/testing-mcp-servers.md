# Testing MCP Servers with the CLI

*Validate protocol compliance, exercise every capability, and catch regressions — all from the command line.*

---

## Why CLI-First Testing?

| Approach | Pros | Cons |
|----------|------|------|
| Custom test client code | Full control | High effort, maintains its own MCP client |
| GUI-based MCP inspector | Visual | Manual, can't automate |
| **mcp2cli** | Zero code, automatable, JSON output | Requires mcp2cli installed |

mcp2cli gives you a ready-made MCP client that speaks the full protocol. Point it at your server and validate everything.

---

## Quick Smoke Test

The fastest way to validate a new MCP server:

```bash
# Test HTTP server
mcp2cli --url http://localhost:3001/mcp doctor

# Test stdio server
mcp2cli --stdio "./target/debug/my-server" doctor
```

`doctor` runs a comprehensive health check:
- ✅ Transport connection
- ✅ Server info (name, version)
- ✅ Auth state
- ✅ Cached capabilities

---

## Protocol Compliance Testing

### Step 1: Initialize & Inspect

```bash
# Full server capabilities dump
mcp2cli --url http://localhost:3001/mcp --json inspect
```

Verify:
- Protocol version matches (2025-11-25)
- Expected capabilities are advertised
- Server name and version are correct

```bash
# Extract specific capabilities
mcp2cli --url http://localhost:3001/mcp --json inspect | \
  jq '.data.capabilities'
```

### Step 2: Discovery

```bash
# List all tools
mcp2cli --url http://localhost:3001/mcp --json ls --tools | \
  jq '.data.items[].id'

# List all resources
mcp2cli --url http://localhost:3001/mcp --json ls --resources | \
  jq '.data.items[].id'

# List all prompts
mcp2cli --url http://localhost:3001/mcp --json ls --prompts | \
  jq '.data.items[].id'
```

### Step 3: Tool Invocation

```bash
# Call each tool and verify it returns without error
for tool in $(mcp2cli --url http://localhost:3001/mcp --json ls --tools | jq -r '.data.items[].id'); do
  echo "Testing: $tool"
  if mcp2cli --url http://localhost:3001/mcp --json --timeout 10 "$tool" 2>/dev/null; then
    echo "  ✅ $tool OK"
  else
    echo "  ❌ $tool FAILED (exit code: $?)"
  fi
done
```

### Step 4: Resource Reading

```bash
# Test every concrete resource
mcp2cli --url http://localhost:3001/mcp --json ls --resources | \
  jq -r '.data.items[] | select(.kind == "resource") | .id' | \
  while read uri; do
    echo "Reading: $uri"
    mcp2cli --url http://localhost:3001/mcp --json get "$uri" | jq '.summary'
  done
```

### Step 5: Ping & Latency

```bash
# Liveness check
mcp2cli --url http://localhost:3001/mcp --json ping | jq '.data'
```

---

## Automated Test Script

Save this as `test-mcp-server.sh`:

```bash
#!/bin/bash
set -euo pipefail

ENDPOINT="${1:?Usage: $0 <endpoint-or-stdio-command>}"
PASS=0
FAIL=0

# Determine connection mode
if [[ "$ENDPOINT" == http* ]]; then
  BASE="mcp2cli --url $ENDPOINT --json --timeout 15"
else
  BASE="mcp2cli --stdio \"$ENDPOINT\" --json --timeout 15"
fi

run_test() {
  local name="$1"
  shift
  if eval "$BASE $*" >/dev/null 2>&1; then
    echo "  ✅ $name"
    ((PASS++))
  else
    echo "  ❌ $name (exit $?)"
    ((FAIL++))
  fi
}

echo "=== MCP Server Test Suite ==="
echo "Target: $ENDPOINT"
echo ""

echo "--- Connectivity ---"
run_test "ping" "ping"
run_test "doctor" "doctor"
run_test "inspect" "inspect"

echo ""
echo "--- Discovery ---"
run_test "discover all" "ls"
run_test "discover tools" "ls --tools"
run_test "discover resources" "ls --resources"
run_test "discover prompts" "ls --prompts"

echo ""
echo "--- Tool Calls ---"
TOOLS=$(eval "$BASE ls --tools" 2>/dev/null | jq -r '.data.items[].id' 2>/dev/null || echo "")
for tool in $TOOLS; do
  run_test "tool: $tool" "$tool" 2>/dev/null || true
done

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
exit $FAIL
```

Usage:

```bash
chmod +x test-mcp-server.sh

# Test HTTP server
./test-mcp-server.sh http://localhost:3001/mcp

# Test stdio server
./test-mcp-server.sh "npx @modelcontextprotocol/server-everything"
```

---

## Regression Testing in CI/CD

### GitHub Actions

```yaml
name: MCP Server Tests
on: [push, pull_request]

jobs:
  test-mcp:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Build server
        run: cargo build --release
      
      - name: Install mcp2cli
        run: cargo install --path .
      
      - name: Start server
        run: ./target/release/my-mcp-server &
        
      - name: Wait for server
        run: |
          for i in $(seq 1 30); do
            mcp2cli --url http://localhost:3001/mcp --timeout 5 ping && break
            sleep 1
          done
      
      - name: Run protocol tests
        run: |
          # Discovery
          TOOLS=$(mcp2cli --url http://localhost:3001/mcp --json ls --tools | jq '.data.items | length')
          echo "Server exposes $TOOLS tools"
          [ "$TOOLS" -gt 0 ] || exit 1
          
          # Smoke test each tool
          mcp2cli --url http://localhost:3001/mcp --json ls --tools | \
            jq -r '.data.items[].id' | while read tool; do
              echo "Testing $tool..."
              mcp2cli --url http://localhost:3001/mcp --json --timeout 10 "$tool" || \
                echo "WARNING: $tool failed"
            done
          
          # Health check
          mcp2cli --url http://localhost:3001/mcp doctor
```

---

## Testing Specific Scenarios

### Required vs. Optional Arguments

```bash
# Should fail — missing required arg
mcp2cli --url http://localhost:3001/mcp echo 2>&1 && echo "BAD: should have failed"

# Should succeed — with required arg
mcp2cli --url http://localhost:3001/mcp echo --message hello || echo "BAD: should have passed"
```

### Argument Type Validation

```bash
# Integer argument
mcp2cli --url http://localhost:3001/mcp add --a 5 --b 3

# Boolean flag
mcp2cli --url http://localhost:3001/mcp process --include-metadata

# JSON argument
mcp2cli --url http://localhost:3001/mcp deploy --config '{"replicas": 3}'

# Array argument
mcp2cli --url http://localhost:3001/mcp tag --labels bug,critical,p0
```

### Error Handling

```bash
# Call a non-existent tool (should fail gracefully)
mcp2cli --url http://localhost:3001/mcp nonexistent-tool 2>&1

# Pass invalid argument types (should fail with validation error)
mcp2cli --url http://localhost:3001/mcp add --a "not-a-number" --b 3 2>&1
```

### Timeout Behavior

```bash
# Server should respond within 5 seconds
mcp2cli --url http://localhost:3001/mcp --timeout 5 ping

# Test slow operations
mcp2cli --url http://localhost:3001/mcp --timeout 2 slow-operation 2>&1 | \
  grep -q "timed out" && echo "Timeout works correctly"
```

---

## Comparing Server Outputs

Capture baseline output and diff against future runs:

```bash
# Create baseline
mcp2cli --url http://localhost:3001/mcp --json ls > baseline.json

# After changes, compare
mcp2cli --url http://localhost:3001/mcp --json ls > current.json
diff <(jq -S '.data.items[].id' baseline.json) <(jq -S '.data.items[].id' current.json)
```

---

## Demo Mode for Fixtures

Use the built-in demo backend for deterministic testing when you need a stable baseline:

```bash
mcp2cli config init --name fixture --transport streamable_http --endpoint https://demo.invalid/mcp
mcp2cli use fixture
mcp2cli --json ls | jq '.data.items | length'  # Always returns same count
```

---

## See Also

- [Ad-Hoc Connections](../features/ad-hoc-connections.md) — `--url`/`--stdio` for stateless testing
- [Output Formats](../features/output-formats.md) — JSON output for assertions
- [Request Timeouts](../features/request-timeouts.md) — timeout enforcement
- [Shell Scripting with MCP](shell-scripting-mcp.md) — full scripting patterns
