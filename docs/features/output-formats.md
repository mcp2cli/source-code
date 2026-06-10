# Output Formats

Every mcp2cli command supports three output formats — human-readable text, JSON, and NDJSON — for both interactive use and programmatic consumption.

---

## Formats

### Human (default)

Optimized for terminal readability:

```bash
email ls
```

```text
send          tool      Send an email message
reply         tool      Reply to a thread
inbox         resource  mail://inbox
simple-prompt prompt    A simple prompt with no arguments
```

### JSON

Structured envelope for programmatic parsing:

```bash
email --json ls
```

```json
{
  "app_id": "email",
  "command": "discover",
  "summary": "discovered 14 capabilities",
  "lines": [
    "send    tool    Send an email message",
    "reply   tool    Reply to a thread"
  ],
  "data": {
    "category": "capabilities",
    "items": [
      { "id": "send", "kind": "tool", "summary": "Send an email message" },
      { "id": "reply", "kind": "tool", "summary": "Reply to a thread" }
    ]
  }
}
```

### NDJSON

Newline-delimited JSON — one JSON object per line:

```bash
email --output ndjson ls
```

```json
{"app_id":"email","command":"discover","summary":"discovered 14 capabilities","lines":[...],"data":{...}}
```

Best for streaming pipelines and log ingestion.

---

## Selecting a Format

### CLI Flags

```bash
email --json ls                   # JSON
email --output json ls            # Same as --json
email --output ndjson ls          # NDJSON
email --output human ls           # Explicit human
email ls                          # Human (default)
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
  "app_id": "email",          // Config name or alias
  "command": "invoke",         // Operation type
  "summary": "called send",   // Human-readable summary
  "lines": ["..."],           // Human-format lines (for compatibility)
  "data": { ... }             // Command-specific structured data
}
```

### Extracting Data with jq

```bash
# List tool names
email --json ls --tools | jq -r '.data.items[].id'

# Get send result content
email --json send --to user@example.com --subject "Hi" --body "..." | jq '.data.content'

# Check server health
email --json doctor | jq '.data.server'

# Get auth state
email --json auth status | jq '.data.auth_session.state'

# Count capabilities
email --json ls | jq '.data.items | length'

# Filter tools by name
email --json ls | jq '[.data.items[] | select(.id | test("draft"))]'
```

---

## Piping and Scripting

JSON output makes mcp2cli a first-class citizen in shell pipelines:

```bash
# Store result for later use
RESULT=$(email --json send --to user@example.com --subject "Hi" --body "hello")
echo "$RESULT" | jq '.data.content[0].text'

# Chain operations
email --json ls --tools | jq -r '.data.items[].id' | while read tool; do
  echo "Testing: $tool"
  email --json "$tool" --help 2>/dev/null || true
done

# Compare before/after
BEFORE=$(email --json ls | jq '.data.items | length')
# ... do something ...
AFTER=$(email --json ls | jq '.data.items | length')
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
