#!/usr/bin/env sh
# mcp2cli installer — curl -fsSL https://mcp2cli.dev/install.sh | sh
#
# Installs mcp2cli from source using cargo. mcp2cli does not yet ship
# pre-built binaries; until it does, this script is the one-command
# way to get the latest main on your PATH.
#
# Optional arguments:
#   --ref=<uuid>   attribute this install to a browser session. The
#                  website generates this per-visitor and embeds it
#                  in the install command it displays.

set -eu

REPO="https://github.com/mcp2cli/source-code"
BRANCH="main"
BIN="mcp2cli"
OTLP_ENDPOINT="https://telemetry.mcp2cli.dev/v1/traces"

# -------- argv parsing --------
MCP2CLI_INSTALL_REF=""
for arg in "$@"; do
    case "$arg" in
        --ref=*) MCP2CLI_INSTALL_REF="${arg#--ref=}" ;;
    esac
done

# -------- colour helpers --------
if [ -t 2 ]; then
    BOLD="$(printf '\033[1m')"
    DIM="$(printf '\033[2m')"
    GREEN="$(printf '\033[32m')"
    RED="$(printf '\033[31m')"
    YELLOW="$(printf '\033[33m')"
    RESET="$(printf '\033[0m')"
else
    BOLD="" DIM="" GREEN="" RED="" YELLOW="" RESET=""
fi

info()  { printf '%s%s%s %s\n' "$BOLD" "mcp2cli" "$RESET" "$1" >&2; }
warn()  { printf '%s%swarn%s    %s\n' "$BOLD" "$YELLOW" "$RESET" "$1" >&2; }
err()   { printf '%s%serror%s   %s\n' "$BOLD" "$RED" "$RESET" "$1" >&2; }
ok()    { printf '%s%sok%s      %s\n' "$BOLD" "$GREEN" "$RESET" "$1" >&2; }

need_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        err "required command not found: $1"
        return 1
    fi
}

print_rustup_hint() {
    cat >&2 <<EOF

${BOLD}mcp2cli installs from source with cargo.${RESET}
Install the Rust toolchain first (one-liner, official installer):

    ${DIM}curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh${RESET}

Then re-run this script. See https://rustup.rs for details.
EOF
}

# -------- telemetry helpers --------

# True when telemetry should be sent — honours the same opt-outs the
# CLI respects: DO_NOT_TRACK, MCP2CLI_TELEMETRY={off,false,0,no,disabled}.
telemetry_enabled() {
    if [ -n "${DO_NOT_TRACK:-}" ]; then return 1; fi
    case "$(printf %s "${MCP2CLI_TELEMETRY:-}" | tr '[:upper:]' '[:lower:]')" in
        off|false|0|no|disabled) return 1 ;;
    esac
    return 0
}

# POSIX UUIDv4 via /dev/urandom with a uuidgen shortcut if available.
new_uuid() {
    if command -v uuidgen >/dev/null 2>&1; then
        uuidgen | tr '[:upper:]' '[:lower:]'
        return
    fi
    # Generate 16 random bytes, format as 8-4-4-4-12 hex.
    od -An -N16 -tx1 /dev/urandom | tr -d ' \n' | \
      sed 's/\(.\{8\}\)\(.\{4\}\)\(.\{4\}\)\(.\{4\}\)\(.\{12\}\)/\1-\2-\3-\4-\5/'
}

# Best-effort nanosecond-since-epoch. Linux/BusyBox give us %N; macOS's
# /bin/date doesn't, so we pad seconds to nanos as a fallback.
now_ns() {
    ns="$(date +%s%N 2>/dev/null || true)"
    case "$ns" in
        *N*|'' ) printf '%s000000000' "$(date +%s)" ;;
        * ) printf '%s' "$ns" ;;
    esac
}

# Strip anything that isn't a UUID-shaped character. The --ref value
# comes from an untrusted source (the browser session id), so we
# reject anything unusual before splicing it into the OTLP JSON body.
sanitise_uuid() {
    printf %s "$1" | tr -dc 'a-fA-F0-9-' | cut -c1-36
}

# Fire-and-forget OTLP span emit. Takes a span name; uses the globally-
# set MCP2CLI_INSTALL_ID / MCP2CLI_INSTALL_REF. Silent on failure;
# backgrounded so it never blocks the install.
emit_span() {
    telemetry_enabled || return 0
    span_name="$1"
    trace_id="$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')"
    span_id="$(od -An -N8 -tx1 /dev/urandom | tr -d ' \n')"
    ts="$(now_ns)"
    body=$(cat <<JSON
{"resourceSpans":[{"resource":{"attributes":[
  {"key":"service.name","value":{"stringValue":"mcp2cli-installer"}},
  {"key":"host.os","value":{"stringValue":"$(uname -s 2>/dev/null || echo unknown)"}},
  {"key":"host.arch","value":{"stringValue":"$(uname -m 2>/dev/null || echo unknown)"}}
]},"scopeSpans":[{"scope":{"name":"mcp2cli.telemetry","version":"1"},"spans":[{
  "traceId":"$trace_id","spanId":"$span_id","name":"$span_name","kind":1,
  "startTimeUnixNano":"$ts","endTimeUnixNano":"$ts",
  "attributes":[
    {"key":"mcp2cli.install_id","value":{"stringValue":"$MCP2CLI_INSTALL_ID"}},
    {"key":"mcp2cli.visitor_id","value":{"stringValue":"$MCP2CLI_INSTALL_REF"}}
  ],"status":{"code":1}
}]}]}]}
JSON
)
    (curl -fsSL --max-time 2 \
        -H 'Content-Type: application/json' \
        -d "$body" \
        "$OTLP_ENDPOINT" \
        >/dev/null 2>&1 &)
}

# -------- main --------
main() {
    # Normalise and generate IDs before any output so the install
    # banner and the written handoff file agree on the same values.
    MCP2CLI_INSTALL_REF="$(sanitise_uuid "$MCP2CLI_INSTALL_REF")"
    MCP2CLI_INSTALL_ID="$(new_uuid)"
    export MCP2CLI_INSTALL_REF MCP2CLI_INSTALL_ID

    info "installing from ${DIM}${REPO}${RESET} (branch ${BRANCH})"

    # Beacon the fetch span early so it lands even if cargo install
    # fails — tells us about abandoned installs too.
    emit_span install_fetch

    if ! need_cmd cargo; then
        print_rustup_hint
        exit 1
    fi

    CARGO_BIN="${CARGO_HOME:-$HOME/.cargo}/bin"
    case ":${PATH:-}:" in
        *":${CARGO_BIN}:"*) ;;
        *) warn "${CARGO_BIN} is not on your PATH — add it to your shell profile after install" ;;
    esac

    info "running: cargo install --git ${REPO} --branch ${BRANCH} --locked"
    cargo install --git "$REPO" --branch "$BRANCH" --locked

    # Hand the IDs off to the CLI — first mcp2cli run picks these up,
    # attaches them to its `first_run` telemetry event, and deletes
    # the file (one-shot).
    DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/mcp2cli"
    if mkdir -p "$DATA_DIR" 2>/dev/null; then
        printf 'visitor_id=%s\ninstall_id=%s\n' \
            "$MCP2CLI_INSTALL_REF" "$MCP2CLI_INSTALL_ID" \
            > "$DATA_DIR/install_ref"
    fi

    if command -v "$BIN" >/dev/null 2>&1; then
        ok  "installed: $(command -v "$BIN")"
        info "next: ${DIM}mcp2cli --help${RESET}  ·  https://mcp2cli.dev"
    else
        warn "cargo finished but '${BIN}' is not on PATH yet"
        warn "open a new shell, or add ${CARGO_BIN} to PATH"
    fi
}

main "$@"
