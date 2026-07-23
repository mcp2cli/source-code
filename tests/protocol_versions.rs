//! Dual-protocol-version integration tests.
//!
//! mcp2cli speaks two MCP revisions: 2025-11-25 (legacy, initialize
//! handshake) and 2026-07-28 (modern, stateless per-request `_meta`).
//! These tests drive the real binary against hermetic fake stdio servers
//! (tests/fixtures/fake_modern_stdio.py / fake_legacy_stdio.py) covering
//! era negotiation, the legacy fallback, MRTR retries, the tasks
//! extension, subscriptions/listen, and the removed-method shims.

mod support;

use predicates::prelude::*;
use support::{TestFixture, mcp2cli_with_config};

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

fn python3_available() -> bool {
    std::process::Command::new("python3")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

macro_rules! require_python {
    () => {
        if !python3_available() {
            eprintln!("skipping: python3 not available");
            return;
        }
    };
}

fn prime_inventory(fixture: &TestFixture, name: &str, config: &std::path::Path) {
    mcp2cli_with_config(fixture, name, config)
        .arg("discover")
        .arg("capabilities")
        .timeout(TIMEOUT)
        .assert()
        .success();
}

/// Select `name` as the active config so dynamic-surface commands
/// (`ping`, `log`, `subscribe`, …) can run as plain `mcp2cli <cmd>` —
/// the dynamic CLI is reached via alias/active-config dispatch, not the
/// explicit `--config` form.
fn activate_config(fixture: &TestFixture, name: &str) {
    let host_dir = fixture.data_dir().join("host");
    std::fs::create_dir_all(&host_dir).expect("host dir should be created");
    std::fs::write(
        host_dir.join("active-config.json"),
        format!("{{\"config_name\":\"{}\"}}", name),
    )
    .expect("active config should be written");
}

// ---------------------------------------------------------------------------
// Era negotiation
// ---------------------------------------------------------------------------

#[test]
fn modern_server_negotiates_2026_07_28_via_discover() {
    require_python!();
    let fixture = TestFixture::new();
    let config = fixture.write_fake_stdio_config("modern", "fake_modern_stdio.py", None);

    mcp2cli_with_config(&fixture, "modern", &config)
        .arg("discover")
        .arg("capabilities")
        .timeout(TIMEOUT)
        .assert()
        .success()
        .stdout(predicate::str::contains("echo"));

    // Doctor reflects the negotiated stateless revision.
    mcp2cli_with_config(&fixture, "modern", &config)
        .arg("doctor")
        .timeout(TIMEOUT)
        .assert()
        .success()
        .stdout(predicate::str::contains("2026-07-28"))
        .stdout(predicate::str::contains("stateless"));
}

#[test]
fn legacy_server_triggers_initialize_fallback() {
    require_python!();
    let fixture = TestFixture::new();
    let config = fixture.write_fake_stdio_config("legacy", "fake_legacy_stdio.py", None);

    mcp2cli_with_config(&fixture, "legacy", &config)
        .arg("discover")
        .arg("capabilities")
        .timeout(TIMEOUT)
        .assert()
        .success()
        .stdout(predicate::str::contains("echo"));

    mcp2cli_with_config(&fixture, "legacy", &config)
        .arg("doctor")
        .timeout(TIMEOUT)
        .assert()
        .success()
        .stdout(predicate::str::contains("2025-11-25"));
}

#[test]
fn pinning_modern_against_legacy_server_fails_without_fallback() {
    require_python!();
    let fixture = TestFixture::new();
    let config =
        fixture.write_fake_stdio_config("pinned", "fake_legacy_stdio.py", Some("2026-07-28"));

    mcp2cli_with_config(&fixture, "pinned", &config)
        .arg("discover")
        .arg("capabilities")
        .timeout(TIMEOUT)
        .assert()
        .failure()
        .stderr(predicate::str::contains("2026-07-28"));
}

#[test]
fn pinning_legacy_skips_the_probe_entirely() {
    require_python!();
    let fixture = TestFixture::new();
    let config = fixture.write_fake_stdio_config(
        "pinned-legacy",
        "fake_legacy_stdio.py",
        Some("2025-11-25"),
    );

    mcp2cli_with_config(&fixture, "pinned-legacy", &config)
        .arg("invoke")
        .arg("--capability")
        .arg("echo")
        .arg("--arg")
        .arg("message=direct-legacy")
        .timeout(TIMEOUT)
        .assert()
        .success()
        .stdout(predicate::str::contains("legacy echo: direct-legacy"));
}

// ---------------------------------------------------------------------------
// Modern request flow
// ---------------------------------------------------------------------------

#[test]
fn modern_tool_call_round_trips() {
    require_python!();
    let fixture = TestFixture::new();
    let config = fixture.write_fake_stdio_config("modern", "fake_modern_stdio.py", None);

    mcp2cli_with_config(&fixture, "modern", &config)
        .arg("invoke")
        .arg("--capability")
        .arg("echo")
        .arg("--arg")
        .arg("message=stateless-hello")
        .timeout(TIMEOUT)
        .assert()
        .success()
        .stdout(predicate::str::contains("echo: stateless-hello"));
}

#[test]
fn modern_mrtr_retry_echoes_request_state() {
    require_python!();
    let fixture = TestFixture::new();
    let config = fixture.write_fake_stdio_config("modern", "fake_modern_stdio.py", None);

    // `guarded` answers the first attempt with resultType input_required
    // (requestState only); the client must retry with the state echoed.
    mcp2cli_with_config(&fixture, "modern", &config)
        .arg("invoke")
        .arg("--capability")
        .arg("guarded")
        .timeout(TIMEOUT)
        .assert()
        .success()
        .stdout(predicate::str::contains("guarded completed after retry"));
}

#[test]
fn modern_task_result_is_polled_to_completion() {
    require_python!();
    let fixture = TestFixture::new();
    let config = fixture.write_fake_stdio_config("modern", "fake_modern_stdio.py", None);

    // `slow` returns resultType task; the client polls tasks/get until the
    // terminal state and surfaces the embedded result transparently.
    mcp2cli_with_config(&fixture, "modern", &config)
        .arg("invoke")
        .arg("--capability")
        .arg("slow")
        .timeout(TIMEOUT)
        .assert()
        .success()
        .stdout(predicate::str::contains("slow task finished"));
}

#[test]
fn modern_ping_uses_server_discover() {
    require_python!();
    let fixture = TestFixture::new();
    let config = fixture.write_fake_stdio_config("modern", "fake_modern_stdio.py", None);
    prime_inventory(&fixture, "modern", &config);
    activate_config(&fixture, "modern");

    support::mcp2cli_cmd(&fixture)
        .arg("ping")
        .timeout(TIMEOUT)
        .assert()
        .success()
        .stdout(predicate::str::contains("pong ("));
}

#[test]
fn modern_read_resource_round_trips() {
    require_python!();
    let fixture = TestFixture::new();
    let config = fixture.write_fake_stdio_config("modern", "fake_modern_stdio.py", None);

    mcp2cli_with_config(&fixture, "modern", &config)
        .arg("read")
        .arg("--uri")
        .arg("fake://doc")
        .timeout(TIMEOUT)
        .assert()
        .success()
        .stdout(predicate::str::contains("fake modern resource body"));
}

// ---------------------------------------------------------------------------
// Removed methods (logging/setLevel, resources/unsubscribe) and
// subscriptions/listen
// ---------------------------------------------------------------------------

#[test]
fn modern_log_level_is_stored_and_injected_per_request() {
    require_python!();
    let fixture = TestFixture::new();
    let config = fixture.write_fake_stdio_config("modern", "fake_modern_stdio.py", None);

    prime_inventory(&fixture, "modern", &config);
    activate_config(&fixture, "modern");

    // logging/setLevel no longer exists — the level resolves locally
    // (the summary explains the per-request _meta delivery)...
    support::mcp2cli_cmd(&fixture)
        .arg("--json")
        .arg("log")
        .arg("debug")
        .timeout(TIMEOUT)
        .assert()
        .success()
        .stdout(predicate::str::contains("io.modelcontextprotocol/logLevel"));

    // ...and subsequent requests carry it in _meta (the fake echo tool
    // appends the received logLevel to its reply).
    mcp2cli_with_config(&fixture, "modern", &config)
        .arg("invoke")
        .arg("--capability")
        .arg("echo")
        .arg("--arg")
        .arg("message=with-level")
        .timeout(TIMEOUT)
        .assert()
        .success()
        .stdout(predicate::str::contains("[logLevel=debug]"));
}

#[test]
fn legacy_log_level_still_calls_logging_set_level() {
    require_python!();
    let fixture = TestFixture::new();
    let config = fixture.write_fake_stdio_config("legacy", "fake_legacy_stdio.py", None);
    prime_inventory(&fixture, "legacy", &config);
    activate_config(&fixture, "legacy");

    support::mcp2cli_cmd(&fixture)
        .arg("--json")
        .arg("log")
        .arg("warning")
        .timeout(TIMEOUT)
        .assert()
        .success()
        .stdout(predicate::str::contains("logging level set to 'warning'"));
}

#[test]
fn modern_subscribe_is_acknowledged_via_subscriptions_listen() {
    require_python!();
    let fixture = TestFixture::new();
    let config = fixture.write_fake_stdio_config("modern", "fake_modern_stdio.py", None);
    prime_inventory(&fixture, "modern", &config);
    activate_config(&fixture, "modern");

    support::mcp2cli_cmd(&fixture)
        .arg("--json")
        .arg("subscribe")
        .arg("fake://doc")
        .timeout(TIMEOUT)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "acknowledged resource subscription",
        ));
}

#[test]
fn modern_unsubscribe_resolves_locally() {
    require_python!();
    let fixture = TestFixture::new();
    let config = fixture.write_fake_stdio_config("modern", "fake_modern_stdio.py", None);
    prime_inventory(&fixture, "modern", &config);
    activate_config(&fixture, "modern");

    support::mcp2cli_cmd(&fixture)
        .arg("--json")
        .arg("unsubscribe")
        .arg("fake://doc")
        .timeout(TIMEOUT)
        .assert()
        .success()
        .stdout(predicate::str::contains("subscriptions/listen stream"));
}
