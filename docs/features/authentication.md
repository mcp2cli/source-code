# Authentication

Manage server authentication with token persistence, interactive login, and automatic header injection.

---

## Commands

```bash
# Login — reads the bearer token from stdin, then falls back to a prompt
echo "$TOKEN" | email auth login

# Login non-interactively (fails fast instead of prompting if no token is piped)
echo "$TOKEN" | email auth login --non-interactive

# Login with a structured payload
email auth login --input-json '{"bearer_token": "sk-abc123", "account": "me@example.com"}'

# Check current state
email auth status

# Clear stored credentials
email auth logout
```

`auth login` obtains the bearer token from one of three sources, in order:

1. **Piped stdin** — `echo "$TOKEN" | email auth login`.
2. **`--input-json`** — a JSON object with the schema below.
3. **Interactive prompt** — used only when neither of the above provides a token.

Pass `--non-interactive` to skip the prompt and fail fast (useful in CI and scripts) when no token is supplied via stdin or `--input-json`.

### `--input-json` schema

```json
{
  "bearer_token": "<token>",
  "account": "<optional>"
}
```

| Field | Required | Meaning |
|-------|----------|---------|
| `bearer_token` | ✅ | The token sent as `Authorization: Bearer <token>`. |
| `account` | — | Optional account label stored alongside the token. |

---

## How It Works

```mermaid
sequenceDiagram
    participant User
    participant CLI as mcp2cli
    participant Store as Token Store
    participant Server as MCP Server

    User->>CLI: email auth login
    CLI->>User: Enter token:
    User->>CLI: sk-abc123
    CLI->>Store: Store token for "email"
    CLI-->>User: Authenticated ✓

    Note over User: Later...

    User->>CLI: email search --query "from:boss"
    CLI->>Store: Load token for "email"
    Store-->>CLI: sk-abc123
    CLI->>Server: POST /mcp<br/>Authorization: Bearer sk-abc123
    Server-->>CLI: Result
    CLI-->>User: Output
```

---

## Token Storage

Tokens are persisted per-config at:

```text
~/.local/share/mcp2cli/instances/<name>/tokens.json
```

The file is written with `0600` permissions (owner read/write only) and contains:

```json
{
  "bearer_token": "sk-abc123"
}
```

### Custom Token Path

Override the default location:

```yaml
auth:
  token_store_file: /secure/path/tokens.json
```

---

## Auth States

| State | Meaning |
|-------|---------|
| `unauthenticated` | No token stored |
| `active` | Token stored and being sent with requests |

Check the current state:

```bash
email auth status
# → Auth state: active

email --json auth status | jq '.data.auth_session.state'
# → "active"
```

---

## Transport Behavior

| Transport | Auth Support | How |
|-----------|-------------|-----|
| Streamable HTTP | ✅ | `Authorization: Bearer <token>` injected on every request from the stored token |
| Stdio | ❌ | Subprocess inherits environment; set env vars in config |
| Demo | ❌ | No auth needed |

The streamable-HTTP transport loads the stored token and attaches an
`Authorization: Bearer <token>` header to every request automatically — no
per-call flags needed. Both `http` and `https` endpoints are supported; HTTPS
connections use TLS via `rustls` with the `webpki` root certificates, so a
production endpoint like `https://mcp.example.com/email` works out of the box.

For stdio servers that need authentication, pass credentials via environment:

```yaml
server:
  transport: stdio
  stdio:
    command: my-server
    env:
      API_KEY: sk-abc123
```

---

## Browser-Based OAuth

For servers that support OAuth browser flows:

```yaml
auth:
  browser_open_command: "xdg-open"    # Linux
  # browser_open_command: "open"      # macOS
```

When the server sends an `elicitation/create` with a URL during auth, mcp2cli opens the browser automatically.

---

## See Also

- [Configuration Reference](../reference/config-reference.md) — auth config fields
- [Elicitation & Sampling](elicitation-and-sampling.md) — interactive auth flows
