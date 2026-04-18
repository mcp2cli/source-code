# Output Formats

Every mcp2cli command supports three output formats — human-readable text, JSON, and NDJSON — for both interactive use and programmatic consumption.

---

## Formats

### Human (default)

Optimized for terminal readability:

```bash
work ls
```

```text
echo          tool      Echoes back the input string
add           tool      Adds two numbers
readme        resource  demo://resource/readme.md
simple-prompt prompt    A simple prompt with no arguments
```

### JSON

Structured envelope for programmatic parsing:

```bash
work --json ls
```

```json
{
  "app_id": "work",
  "command": "discover",
  "summary": "discovered 14 capabilities",
  "lines": [
    "echo    tool    Echoes back the input string",
    "add     tool    Adds two numbers"
  ],
  "data": {
    "category": "capabilities",
    "items": [
      { "id": "echo", "kind": "tool", "summary": "Echoes back the input string" },
      { "id": "add", "kind": "tool", "summary": "Adds two numbers" }
    ]
  }
}
```

### NDJSON

Newline-delimited JSON — one JSON object per line:

```bash
work --output ndjson ls
```

```json
{"app_id":"work","command":"discover","summary":"discovered 14 capabilities","lines":[...],"data":{...}}
```

Best for streaming pipelines and log ingestion.

---

## Selecting a Format

### CLI Flags

```bash
work --json ls                    # JSON
work --output json ls             # Same as --json
work --output ndjson ls           # NDJSON
work --output human ls            # Explicit human
work ls                           # Human (default)
```

### Config Default

```yaml
defaults:
  output: json                    # Default for all commands
```

### Precedence

`--json` flag → `--output` flag → config `defaults.output` → `human`

---

## JSON Envelope Structure

Every JSON response follows a consistent envelope:

```json
{
  "app_id": "work",           // Config name or alias
  "command": "invoke",         // Operation type
  "summary": "called echo",   // Human-readable summary
  "lines": ["..."],           // Human-format lines (for compatibility)
  "data": { ... }             // Command-specific structured data
}
```

### Extracting Data with jq

```bash
# List tool names
work --json ls --tools | jq -r '.data.items[].id'

# Get echo result content
work --json echo --message hello | jq '.data.content'

# Check server health
work --json doctor | jq '.data.server'

# Get auth state
work --json auth status | jq '.data.auth_session.state'

# Count capabilities
work --json ls | jq '.data.items | length'

# Filter tools by name
work --json ls | jq '[.data.items[] | select(.id | test("email"))]'
```

---

## Piping and Scripting

JSON output makes mcp2cli a first-class citizen in shell pipelines:

```bash
# Store result for later use
RESULT=$(work --json echo --message hello)
echo "$RESULT" | jq '.data.content[0].text'

# Chain operations
work --json ls --tools | jq -r '.data.items[].id' | while read tool; do
  echo "Testing: $tool"
  work --json "$tool" --help 2>/dev/null || true
done

# Compare before/after
BEFORE=$(work --json ls | jq '.data.items | length')
# ... do something ...
AFTER=$(work --json ls | jq '.data.items | length')
echo "Tools changed: $BEFORE → $AFTER"
```

---

## Host Command Output

Host-level commands (`config`, `link`, `use`, `daemon`) also respect `--json`:

```bash
mcp2cli --json config list | jq '.[].name'
mcp2cli --json daemon status | jq '.data.daemons[] | select(.status == "running")'
mcp2cli --json use --show | jq '.data.config_name'
```

---

## See Also

- [Shell Scripting with MCP](../articles/shell-scripting-mcp.md) — full scripting patterns
- [AI Agents + MCP via CLI](../articles/ai-agents-mcp-cli.md) — JSON output for agent integration
- [Configuration Reference](../reference/config-reference.md) — `defaults.output` config
