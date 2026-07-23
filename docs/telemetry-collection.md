# Telemetry: Collection & Backend Setup

*How mcp2cli telemetry works, what's collected, and how to set up a backend to aggregate usage data — without vendor lock-in.*

---

## Overview

mcp2cli includes opt-out anonymous usage telemetry. It helps the team understand:

- Which features are actually used (and which are dead weight)
- What transports and configurations are popular
- Where errors occur most often
- How long operations take in real usage

The system is designed with three principles:

1. **Privacy first** — no sensitive data, no identity, no tracking
2. **Vendor agnostic** — local NDJSON files + HTTP POST to any
   collector (defaults to a first-party endpoint, can be redirected
   or disabled in config)
3. **User control** — multiple opt-out mechanisms, full data transparency

## Default collector

Out of the box, events are converted to **OTLP/HTTP JSON spans** — the
standard OpenTelemetry wire format — and POSTed to

```text
https://telemetry.mcp2cli.dev/v1/traces
```

the tsok observability stack's dedicated ingest endpoint (Grafana +
Tempo/Loki/Prometheus). Any OTEL Collector can ingest the same payload
natively. No third-party trackers.

Every batch carries a `service.namespace = "mcp2cli"` **resource**
attribute — that's what actually files the data under the mcp2cli
project on a backend shared across multiple projects; the endpoint URL
itself carries no project identity. If you ever redirect `telemetry.endpoint`
at your own collector (see [Option B](#option-b-built-in-http-shipping)
below), that attribute travels with it regardless of destination.

Override the URL with `telemetry.endpoint` in your config, set it
to `null` to keep events purely local, or opt out entirely via any
of the mechanisms below.

### Verifying the endpoint is reachable

```bash
curl -s -o /dev/null -w '%{http_code}\n' -X POST \
  https://telemetry.mcp2cli.dev/v1/traces \
  -H 'Content-Type: application/json' --data '{"resourceSpans":[]}'
# expect 200
```

The endpoint is OTLP/HTTP only (JSON or protobuf, no gRPC), requires no
auth (identity is purely the resource attributes in the payload), and is
rate-limited to 100 requests/second (200 burst) per source IP — mcp2cli's
shipper already backs off gracefully on a `429` by simply leaving the
batch on local disk for the next invocation to retry, so this never
surfaces as a user-visible error.

## CLI telemetry is independent of the website

mcp2cli's CLI telemetry does **not** share any identifier with
website analytics, the installer, or any other surface. The CLI's
`installation_id` is a random per-machine UUID (`telemetry_id`
file) created on first run and never transmitted to the install
script or the website. `install.sh` sends no telemetry at all.
Browser analytics on mcp2cli.dev, when present, use their own
session-scoped identifiers that never cross into the CLI. There is
deliberately no web → install → first-run attribution chain — we
don't think users should be tracked across surfaces just because
they both come from the same project.

---

## What's Collected

Each CLI invocation produces one event, held locally in this schema:

```json
{
  "schema": 1,
  "installation_id": "550e8400-e29b-41d4-a716-446655440000",
  "timestamp": "2026-03-30T14:22:00Z",
  "cli_version": "0.1.7",
  "os": "linux",
  "arch": "x86_64",
  "event": {
    "type": "command_run",
    "command_category": "tool_invoke",
    "transport": "streamable_http",
    "json_output": false,
    "background": false,
    "timeout_override": false,
    "profile_active": true,
    "daemon_active": false,
    "ad_hoc": false,
    "protocol_era": "modern",
    "outcome": "success",
    "duration_ms": 342
  }
}
```

### Fields Explained

| Field | Purpose | Example Values |
|-------|---------|----------------|
| `schema` | Event schema version for forward compatibility | `1` |
| `installation_id` | Random UUID per installation — not user-identifying | UUID v4 |
| `timestamp` | When the command ran (UTC) | ISO-8601 |
| `cli_version` | mcp2cli version | `"0.1.7"` |
| `os` | Operating system family | `"linux"`, `"macos"`, `"windows"` |
| `arch` | CPU architecture | `"x86_64"`, `"aarch64"` |
| `command_category` | What type of command (NOT the actual name) | See table below |
| `transport` | Connection type used | `"streamable_http"`, `"stdio"`, `"configured"`, `"ad_hoc"`, `"none"` |
| `json_output` | Whether `--json` was passed | `true`/`false` |
| `background` | Whether `--background` was used | `true`/`false` |
| `timeout_override` | Whether `--timeout` was explicitly set | `true`/`false` |
| `profile_active` | Whether a profile overlay was in use | `true`/`false` |
| `daemon_active` | Whether the invocation was routed through a running `mcp2cli daemon` | `true`/`false` |
| `ad_hoc` | Whether `--url`/`--stdio` ad-hoc mode was used | `true`/`false` |
| `protocol_era` | Negotiated MCP protocol revision, when a session was negotiated | `"legacy"` (2025-11-25), `"modern"` (2026-07-28), absent |
| `outcome` | Result of the command — never the error message itself | `"success"`, `"error"` |
| `duration_ms` | Wall-clock time in milliseconds | `342` |

### What actually goes over the wire

The schema above is the **local** NDJSON format. When shipping to an HTTP
endpoint, events are converted into a real OTLP/HTTP JSON `resourceSpans`
batch — one span per event, one shared resource block per batch:

```json
{
  "resourceSpans": [{
    "resource": {
      "attributes": [
        { "key": "service.name", "value": { "stringValue": "mcp2cli-cli" } },
        { "key": "service.namespace", "value": { "stringValue": "mcp2cli" } },
        { "key": "service.version", "value": { "stringValue": "0.1.7" } },
        { "key": "mcp2cli.os", "value": { "stringValue": "linux" } },
        { "key": "mcp2cli.arch", "value": { "stringValue": "x86_64" } }
      ]
    },
    "scopeSpans": [{
      "scope": { "name": "mcp2cli.telemetry", "version": "1" },
      "spans": [{
        "traceId": "…", "spanId": "…",
        "name": "command_run",
        "startTimeUnixNano": "…", "endTimeUnixNano": "…",
        "attributes": [
          { "key": "mcp2cli.installation_id", "value": { "stringValue": "…" } },
          { "key": "mcp2cli.command.category", "value": { "stringValue": "tool_invoke" } },
          { "key": "mcp2cli.transport", "value": { "stringValue": "streamable_http" } },
          { "key": "mcp2cli.outcome", "value": { "stringValue": "success" } },
          { "key": "mcp2cli.protocol_era", "value": { "stringValue": "modern" } },
          { "key": "mcp2cli.duration_ms", "value": { "intValue": "342" } }
        ],
        "status": { "code": 1 }
      }]
    }]
  }]
}
```

`service.namespace` and `service.name` are **resource** attributes
(describe the sending application), not span attributes — this is
deliberate: a dashboard that groups by project reads the resource block,
not per-event fields. Every other collected field is a span attribute,
namespaced under `mcp2cli.*`.

### Command Categories

| Category | Maps to |
|----------|---------|
| `tool_invoke` | Any tool call (the tool name is NOT recorded) |
| `resource_read` | `get <URI>` |
| `prompt_run` | Any prompt execution |
| `discover` | `ls` |
| `ping` | `ping` |
| `doctor` | `doctor` |
| `inspect` | `inspect` |
| `auth` | `auth login/logout/status` |
| `jobs` | `jobs list/show/wait/cancel/watch` |
| `log` | `log <level>` |
| `complete` | `complete` |
| `subscribe` | `subscribe`/`unsubscribe` |
| `config` | `config init/list/show` |
| `link` | `link create` |
| `use` | `use <name>` |
| `daemon` | `daemon start/stop/status` |
| `command` | Other server-derived commands |

### Special Events

| Event Type | When | Sent |
|------------|------|------|
| `first_run` | First time mcp2cli runs on an installation | Once per installation |
| `command_run` | Every CLI invocation | Per invocation |

---

## What's NOT Collected

This is explicit and permanent — these items will never be added:

- **No server endpoints or URLs** — we don't know where your MCP server lives
- **No tool/prompt/resource names** — we don't know what your server offers
- **No argument values** — we don't see your data, messages, or payloads
- **No file paths** — we don't know your directory structure
- **No config content** — we don't read your YAML beyond telemetry settings
- **No environment variables** — we don't see your credentials or secrets
- **No error messages or stack traces** — `outcome` is a coarse `success`/`error`, nothing more
- **No hostname, username, or process ID** — never sent as OTel resource attributes, even though those are common conventions for other kinds of telemetry
- **No IP addresses recorded by us** — the local NDJSON mode has no network component; the shipped HTTP request necessarily has a source IP at the transport layer like any request, but mcp2cli never reads or records it
- **No user identifiers** — the installation_id is a random UUID

---

## Opt-Out Mechanisms

Any one of these disables telemetry:

### 1. Config File

```yaml
telemetry:
  enabled: false
```

### 2. Environment Variable

```bash
export MCP2CLI_TELEMETRY=off   # or: false, 0, no, disabled
```

### 3. CLI Flag

```bash
mcp2cli --no-telemetry ls
```

### 4. DO_NOT_TRACK Standard

```bash
export DO_NOT_TRACK=1
```

Following the [Console Do Not Track](https://consoledonottrack.com/) standard used by Homebrew, Gatsby, and other CLI tools.

### Precedence

```text
--no-telemetry flag > MCP2CLI_TELEMETRY env > DO_NOT_TRACK env > config enabled field
```

If any of these signals "off", telemetry is fully disabled for that invocation.

---

## Local Data Storage

Events are written to:

```text
~/.local/share/mcp2cli/telemetry.ndjson
```

This is a standard newline-delimited JSON file. You can:

```bash
# View events
cat ~/.local/share/mcp2cli/telemetry.ndjson | jq '.'

# Count events
wc -l ~/.local/share/mcp2cli/telemetry.ndjson

# See command distribution
cat ~/.local/share/mcp2cli/telemetry.ndjson | \
  jq -r '.event.command_category // .event.type' | sort | uniq -c | sort -rn

# See error rate
cat ~/.local/share/mcp2cli/telemetry.ndjson | \
  jq -r 'select(.event.outcome == "error") | .event.command_category'

# Delete all data
rm ~/.local/share/mcp2cli/telemetry.ndjson

# See your installation ID
cat ~/.local/share/mcp2cli/telemetry_id
```

### Other Files

| File | Purpose |
|------|---------|
| `telemetry_id` | Random UUID identifying this installation |
| `telemetry_first_run` | Marker file — first-run event already sent |
| `telemetry.ndjson` | Event log (append-only) |
| `telemetry.pending.json` | Pending batch for HTTP shipping (temporary) |

---

## Setting Up a Collection Backend

The mcp2cli telemetry system is vendor-agnostic. Locally, events are
NDJSON — any system that accepts JSON can be a backend. Two different
integration points below, don't confuse them:

- **Option A and C–F** read `telemetry.ndjson` directly, on their own
  schedule (a cron job, a manual relay script, …) — they never touch
  mcp2cli's own HTTP shipping and can use whatever wire format you want,
  since you own both ends.
- **Option B** reconfigures mcp2cli's *built-in* shipper (the one that
  talks to the default collector by default) to POST to your URL instead
  — that one always sends real OTLP/HTTP JSON, never a bespoke format.

### Architecture Options

```mermaid
graph LR
    subgraph "User Machines"
        CLI1["mcp2cli"] --> LOCAL1["telemetry.ndjson"]
        CLI2["mcp2cli"] --> LOCAL2["telemetry.ndjson"]
    end

    subgraph "Collection (pick one)"
        A["Option A: Cron script<br/>ships to API"]
        B["Option B: HTTP endpoint<br/>in config"]
        C["Option C: Central log<br/>aggregation"]
    end

    subgraph "Backend (pick one)"
        PG["PostgreSQL"]
        CH["ClickHouse"]
        PH["PostHog"]
        PL["Plausible"]
        S3["S3 + Athena"]
    end

    LOCAL1 --> A
    LOCAL2 --> A
    A --> PG
    B --> PH
    C --> CH
```

---

### Option A: Simple Cron-Based Collection (Recommended Starting Point)

The simplest approach — a cron job ships events from the local file to your backend.

#### 1. Collector Script

```bash
#!/bin/bash
# /usr/local/bin/mcp2cli-telemetry-ship.sh
# Ships telemetry events to an HTTP endpoint and rotates the local file.

TELEMETRY_FILE="$HOME/.local/share/mcp2cli/telemetry.ndjson"
ENDPOINT="${MCP2CLI_TELEMETRY_ENDPOINT:-https://your-collector.example.com/v1/events}"

if [ ! -f "$TELEMETRY_FILE" ] || [ ! -s "$TELEMETRY_FILE" ]; then
  exit 0
fi

# Atomically move the file to avoid race with mcp2cli
BATCH="/tmp/mcp2cli-telemetry-batch-$$.ndjson"
mv "$TELEMETRY_FILE" "$BATCH"

# Ship as JSON array
PAYLOAD=$(jq -s '.' "$BATCH")
HTTP_CODE=$(curl -sf -o /dev/null -w "%{http_code}" \
  -X POST "$ENDPOINT" \
  -H "Content-Type: application/json" \
  -d "$PAYLOAD")

if [ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "202" ]; then
  rm "$BATCH"
else
  # Put events back on failure
  cat "$BATCH" >> "$TELEMETRY_FILE"
  rm "$BATCH"
fi
```

#### 2. Cron Entry

```bash
# Ship telemetry every hour
0 * * * * /usr/local/bin/mcp2cli-telemetry-ship.sh
```

#### 3. Minimal HTTP Collector (Node.js)

```javascript
// collector.js — receives telemetry batches and appends to a file
const http = require('http');
const fs = require('fs');

const LOG_FILE = '/var/log/mcp2cli-telemetry.ndjson';

http.createServer((req, res) => {
  if (req.method !== 'POST') {
    res.writeHead(405).end();
    return;
  }
  let body = '';
  req.on('data', chunk => body += chunk);
  req.on('end', () => {
    try {
      const events = JSON.parse(body);
      const lines = events.map(e => JSON.stringify(e)).join('\n') + '\n';
      fs.appendFileSync(LOG_FILE, lines);
      res.writeHead(202).end();
    } catch {
      res.writeHead(400).end();
    }
  });
}).listen(9090, () => console.log('Telemetry collector on :9090'));
```

---

### Option B: Built-in HTTP Shipping

Redirect mcp2cli's own shipper — the same one that talks to the default
collector — at your endpoint instead:

```yaml
telemetry:
  enabled: true
  endpoint: "https://your-collector.example.com/v1/traces"
  batch_size: 25
```

Unlike Options A and C–F below (which read the local NDJSON file
independently, on your own schedule), this is what mcp2cli's *built-in*
shipper sends, to whatever URL you configure. It always sends a real
**OTLP/HTTP JSON `resourceSpans` batch** — see [What actually goes over
the wire](#what-actually-goes-over-the-wire) above for the exact shape —
not a plain JSON array of the local event schema. `batch_size` caps how
many pending local events go into one POST; a batch is attempted on every
invocation that has something pending, not only once `batch_size` is
reached. Your collector needs to speak OTLP/HTTP (any OpenTelemetry
Collector receiver does this natively) or translate it — see the
[OTLP/HTTP spec](https://opentelemetry.io/docs/specs/otlp/#otlphttp) if
you're writing a custom one.

A shipping attempt only removes events from the local file after a
confirmed `2xx` response; any other outcome (network error, timeout,
non-2xx status) leaves them in place for the next invocation to retry, so
a temporarily unreachable collector never loses data.

---

### Option C: PostHog (Self-Hosted or Cloud)

[PostHog](https://posthog.com/) is open-source product analytics, self-hostable, with a generous free tier.

#### Setup

1. Deploy PostHog (Docker or cloud)
2. Create a project and get your API key
3. Set up a collector that translates events to PostHog format:

```python
# posthog-relay.py — translates mcp2cli events to PostHog capture API
import json, sys, requests

POSTHOG_HOST = "https://your-posthog.example.com"
API_KEY = "phc_your_project_api_key"

for line in sys.stdin:
    event = json.loads(line)
    kind = event["event"]
    props = {
        "cli_version": event["cli_version"],
        "os": event["os"],
        "arch": event["arch"],
    }
    if kind.get("type") == "command_run":
        props.update({
            "command_category": kind["command_category"],
            "transport": kind["transport"],
            "outcome": kind["outcome"],
            "duration_ms": kind["duration_ms"],
            "json_output": kind["json_output"],
            "background": kind["background"],
            "ad_hoc": kind["ad_hoc"],
        })
    requests.post(f"{POSTHOG_HOST}/capture/", json={
        "api_key": API_KEY,
        "event": kind.get("type", "unknown"),
        "distinct_id": event["installation_id"],
        "properties": props,
        "timestamp": event["timestamp"],
    })
```

Usage:

```bash
cat /var/log/mcp2cli-telemetry.ndjson | python posthog-relay.py
```

---

### Option D: PostgreSQL

Direct storage in PostgreSQL for teams who want SQL analytics.

#### Schema

```sql
CREATE TABLE mcp2cli_telemetry (
    id BIGSERIAL PRIMARY KEY,
    received_at TIMESTAMPTZ DEFAULT now(),
    schema_version INT NOT NULL,
    installation_id UUID NOT NULL,
    event_timestamp TIMESTAMPTZ NOT NULL,
    cli_version TEXT NOT NULL,
    os TEXT NOT NULL,
    arch TEXT NOT NULL,
    event_type TEXT NOT NULL,
    command_category TEXT,
    transport TEXT,
    json_output BOOLEAN,
    background BOOLEAN,
    timeout_override BOOLEAN,
    profile_active BOOLEAN,
    daemon_active BOOLEAN,
    ad_hoc BOOLEAN,
    outcome TEXT,
    duration_ms BIGINT
);

CREATE INDEX idx_telemetry_ts ON mcp2cli_telemetry (event_timestamp);
CREATE INDEX idx_telemetry_category ON mcp2cli_telemetry (command_category);
CREATE INDEX idx_telemetry_installation ON mcp2cli_telemetry (installation_id);
```

#### Ingest Script

```bash
#!/bin/bash
# ingest-to-postgres.sh
DB_URL="${TELEMETRY_DB_URL:-postgres://localhost/mcp2cli}"

cat /var/log/mcp2cli-telemetry.ndjson | jq -r '
  [
    .schema,
    .installation_id,
    .timestamp,
    .cli_version,
    .os,
    .arch,
    (.event.type // "unknown"),
    (.event.command_category // null),
    (.event.transport // null),
    (.event.json_output // null),
    (.event.background // null),
    (.event.timeout_override // null),
    (.event.profile_active // null),
    (.event.daemon_active // null),
    (.event.ad_hoc // null),
    (.event.outcome // null),
    (.event.duration_ms // null)
  ] | @csv
' | psql "$DB_URL" -c "
COPY mcp2cli_telemetry (
  schema_version, installation_id, event_timestamp, cli_version,
  os, arch, event_type, command_category, transport,
  json_output, background, timeout_override, profile_active,
  daemon_active, ad_hoc, outcome, duration_ms
) FROM STDIN WITH CSV;
"
```

#### Example Queries

```sql
-- Top 10 most-used commands
SELECT command_category, COUNT(*) as uses
FROM mcp2cli_telemetry
WHERE event_type = 'command_run'
GROUP BY command_category
ORDER BY uses DESC
LIMIT 10;

-- Error rate by command category
SELECT
  command_category,
  COUNT(*) FILTER (WHERE outcome = 'error') AS errors,
  COUNT(*) AS total,
  ROUND(100.0 * COUNT(*) FILTER (WHERE outcome = 'error') / COUNT(*), 1) AS error_pct
FROM mcp2cli_telemetry
WHERE event_type = 'command_run'
GROUP BY command_category
ORDER BY error_pct DESC;

-- Feature adoption over time (weekly)
SELECT
  date_trunc('week', event_timestamp) AS week,
  COUNT(*) FILTER (WHERE json_output) AS json_users,
  COUNT(*) FILTER (WHERE background) AS background_users,
  COUNT(*) FILTER (WHERE daemon_active) AS daemon_users,
  COUNT(*) FILTER (WHERE ad_hoc) AS adhoc_users,
  COUNT(*) FILTER (WHERE profile_active) AS profile_users
FROM mcp2cli_telemetry
WHERE event_type = 'command_run'
GROUP BY week
ORDER BY week;

-- P50/P95/P99 latency by command
SELECT
  command_category,
  PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY duration_ms) AS p50_ms,
  PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY duration_ms) AS p95_ms,
  PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY duration_ms) AS p99_ms
FROM mcp2cli_telemetry
WHERE event_type = 'command_run' AND outcome = 'success'
GROUP BY command_category;

-- Unique installations per week
SELECT
  date_trunc('week', event_timestamp) AS week,
  COUNT(DISTINCT installation_id) AS active_installs
FROM mcp2cli_telemetry
GROUP BY week
ORDER BY week;

-- OS/Arch distribution
SELECT os, arch, COUNT(DISTINCT installation_id) AS installs
FROM mcp2cli_telemetry
GROUP BY os, arch
ORDER BY installs DESC;
```

---

### Option E: ClickHouse (High Volume)

For high-volume deployments, ClickHouse is ideal for analytics on append-only event data.

```sql
CREATE TABLE mcp2cli_telemetry (
    installation_id UUID,
    event_timestamp DateTime64(3),
    cli_version LowCardinality(String),
    os LowCardinality(String),
    arch LowCardinality(String),
    event_type LowCardinality(String),
    command_category LowCardinality(String),
    transport LowCardinality(String),
    outcome LowCardinality(String),
    duration_ms UInt64,
    json_output Bool,
    background Bool,
    ad_hoc Bool
) ENGINE = MergeTree()
ORDER BY (event_timestamp, installation_id);
```

---

### Option F: S3 + Athena (Serverless)

For AWS-native teams: ship NDJSON to S3, query with Athena.

```bash
# Ship to S3
aws s3 cp /var/log/mcp2cli-telemetry.ndjson \
  s3://your-bucket/telemetry/$(date +%Y/%m/%d)/batch-$(date +%s).ndjson
```

Athena external table:

```sql
CREATE EXTERNAL TABLE mcp2cli_telemetry (
    schema INT,
    installation_id STRING,
    timestamp STRING,
    cli_version STRING,
    os STRING,
    arch STRING,
    event STRUCT<
        type: STRING,
        command_category: STRING,
        transport: STRING,
        outcome: STRING,
        duration_ms: BIGINT,
        json_output: BOOLEAN,
        background: BOOLEAN,
        ad_hoc: BOOLEAN
    >
)
ROW FORMAT SERDE 'org.openx.data.jsonserde.JsonSerDe'
LOCATION 's3://your-bucket/telemetry/';
```

---

## Dashboard Queries (Backend-Agnostic)

These queries work against the NDJSON data regardless of backend. Use `jq` for local analysis or translate to SQL.

### Feature Adoption Report

```bash
# Which features are people actually using?
cat telemetry.ndjson | jq -r '
  select(.event.type == "command_run") |
  [
    (if .event.json_output then "json_output" else empty end),
    (if .event.background then "background" else empty end),
    (if .event.ad_hoc then "ad_hoc" else empty end),
    (if .event.profile_active then "profile" else empty end),
    (if .event.daemon_active then "daemon" else empty end),
    (if .event.timeout_override then "timeout" else empty end)
  ] | .[]
' | sort | uniq -c | sort -rn
```

### Version Adoption

```bash
cat telemetry.ndjson | jq -r '.cli_version' | sort | uniq -c | sort -rn
```

### Slowest Commands

```bash
cat telemetry.ndjson | jq -r '
  select(.event.type == "command_run" and .event.outcome == "success") |
  "\(.event.duration_ms)ms \(.event.command_category)"
' | sort -rn | head -20
```

---

## Privacy Audit Checklist

Use this checklist to verify the telemetry implementation meets privacy requirements:

- [ ] Events contain no server endpoints or URLs
- [ ] Events contain no tool/prompt/resource names
- [ ] Events contain no argument values or payloads
- [ ] Events contain no file paths
- [ ] Events contain no environment variables
- [ ] Events contain no error messages or stack traces (`outcome` only)
- [ ] Shipped OTLP resource attributes contain no hostname, username, or process ID
- [ ] Installation ID is a random UUID (not derived from user info)
- [ ] Opt-out via config works (`telemetry.enabled: false`)
- [ ] Opt-out via env var works (`MCP2CLI_TELEMETRY=off`)
- [ ] Opt-out via CLI flag works (`--no-telemetry`)
- [ ] `DO_NOT_TRACK=1` disables telemetry
- [ ] Local NDJSON file is human-readable and inspectable
- [ ] Users can delete telemetry data at any time
- [ ] No telemetry is sent during CI (if `DO_NOT_TRACK` is set)

---

## Industry References

The mcp2cli telemetry design follows established patterns from:

| Project | Model | Docs |
|---------|-------|------|
| [Homebrew](https://docs.brew.sh/Analytics) | Opt-out, anonymous, Google Analytics | `HOMEBREW_NO_ANALYTICS=1` |
| [Rust/Cargo](https://blog.rust-lang.org/2020/01/31/conf-2020-upgrade.html) | Survey-based (no runtime telemetry) | — |
| [VS Code](https://code.visualstudio.com/docs/getstarted/telemetry) | Opt-out, detailed levels, Application Insights | Settings UI |
| [Next.js](https://nextjs.org/telemetry) | Opt-out, anonymous, PostHog | `npx next telemetry disable` |
| [Gatsby](https://www.gatsbyjs.com/docs/telemetry/) | Opt-out, anonymous | `gatsby telemetry --disable` |
| [.NET CLI](https://learn.microsoft.com/en-us/dotnet/core/tools/telemetry) | Opt-out, anonymous, Application Insights | `DOTNET_CLI_TELEMETRY_OPTOUT=1` |

---

## See Also

- [Config Reference](reference/config-reference.md) — `telemetry` config section
- [CLI Reference](reference/cli-reference.md) — `--no-telemetry` flag
