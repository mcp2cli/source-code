# Elicitation & Sampling

Handle interactive server-initiated requests — when the MCP server needs human input during tool execution.

---

## Elicitation

### What Is It?

During a tool call, the server may send an `elicitation/create` request asking the user for additional information — a form with fields, or a URL to visit.

### How It Looks

```
--- elicitation request ---
Please provide additional information:
  Name (Your full name) [required]: John Doe
  Age [required] [default: 25]: 30
  Role [options: admin, user, guest]: admin
--- end elicitation ---
```

### Type Coercion

mcp2cli automatically coerces typed input:

| Schema Type | Input | Coerced To |
|-------------|-------|------------|
| `boolean` | `yes`, `true`, `y`, `1` | `true` |
| `integer` | `42` | `42` |
| `number` | `3.14` | `3.14` |
| `array` | `a,b,c` | `["a","b","c"]` |
| `enum` | Title matching | Enum value |

### Defaults

When a field has a default value from the schema, pressing Enter without input uses the default.

### URL Mode

If the elicitation contains a URL (e.g., for OAuth), mcp2cli opens it in the browser:

```yaml
auth:
  browser_open_command: "xdg-open"    # Or "open" on macOS
```

---

## Sampling

### What Is It?

The server may send a `sampling/createMessage` request during tool execution, asking the client for a model response. In mcp2cli, this becomes a **human-in-the-loop** prompt — you are the "model."

### How It Looks

```
--- sampling request ---
The server requests a model response.
Model hint: claude-3-5-sonnet
System: You are a helpful assistant
Max tokens: 1000

Messages:
  [user] Summarize this document: ...

Available tools:
  search - Search the knowledge base
  calculate - Perform calculations

Tool choice: auto

Your response (or 'decline' to reject): 
--- end sampling ---
```

### Declining

Type `decline` or press Enter with no input to reject the sampling request. The server receives an error response.

### Response

Your text is sent back to the server with `model: "human-in-the-loop"`.

### Tool Information

When the server includes `tools` and `toolChoice` in the sampling request, mcp2cli displays the available tools and their descriptions so you can make an informed response.

---

## Capability Advertisement

mcp2cli advertises these capabilities during MCP initialization:

```json
{
  "capabilities": {
    "sampling": {},
    "elicitation": {},
    "roots": { "listChanged": true }
  }
}
```

This tells the server it can send elicitation and sampling requests.

---

## Non-Interactive Mode

In scripts and CI/CD, elicitation and sampling prompts block forever waiting for input. Solutions:

1. **Pipe input:** `echo "value" | work tool-that-elicits`
2. **Use `--timeout`:** `work --timeout 30 tool-that-elicits` — fails after 30s if blocked
3. **Server-side:** Configure the server to skip elicitation for automated clients

---

## See Also

- [Authentication](authentication.md) — OAuth flows may use elicitation
- [Request Timeouts](request-timeouts.md) — prevent blocking in non-interactive mode
