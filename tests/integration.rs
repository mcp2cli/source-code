mod support;

use predicates::prelude::*;
use support::{TestFixture, mcp2cli_cmd, mcp2cli_with_config};

// ---------------------------------------------------------------------------
// Stdio Transport: discover
// ---------------------------------------------------------------------------

#[test]
fn stdio_discover_capabilities_lists_tools() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("integ");

    mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("discover")
        .arg("capabilities")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("echo"))
        .stdout(predicate::str::contains("tool"));
}

#[test]
fn stdio_discover_resources_lists_resources() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("integ");

    mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("discover")
        .arg("resources")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("text/markdown"));
}

#[test]
fn stdio_discover_prompts_lists_prompts() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("integ");

    mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("discover")
        .arg("prompts")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("simple-prompt"));
}

// ---------------------------------------------------------------------------
// Stdio Transport: invoke
// ---------------------------------------------------------------------------

#[test]
fn stdio_invoke_echo_returns_echoed_message() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("integ");

    mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("invoke")
        .arg("--capability")
        .arg("echo")
        .arg("--arg")
        .arg("message=integration-test-hello")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("integration-test-hello"));
}

#[test]
fn stdio_invoke_echo_json_output() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("integ");

    let output = mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("--json")
        .arg("invoke")
        .arg("--capability")
        .arg("echo")
        .arg("--arg")
        .arg("message=json-test")
        .timeout(std::time::Duration::from_secs(30))
        .output()
        .expect("command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("output should be valid JSON");
    assert_eq!(parsed["command"], "invoke");
    assert!(parsed["data"].is_object());
}

// ---------------------------------------------------------------------------
// Stdio Transport: read
// ---------------------------------------------------------------------------

#[test]
fn stdio_read_resource_returns_content() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("integ");

    mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("read")
        .arg("--uri")
        .arg("demo://resource/static/document/architecture.md")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "uri: demo://resource/static/document/architecture.md",
        ))
        .stdout(predicate::str::contains("text/markdown"));
}

// ---------------------------------------------------------------------------
// Stdio Transport: prompt
// ---------------------------------------------------------------------------

#[test]
fn stdio_prompt_simple_returns_output() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("integ");

    mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("prompt")
        .arg("run")
        .arg("simple-prompt")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("simple-prompt"))
        .stdout(predicate::str::contains("output:"));
}

// ---------------------------------------------------------------------------
// Stdio Transport: list
// ---------------------------------------------------------------------------

#[test]
fn stdio_list_tools_shows_echo() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("integ");

    mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("list")
        .arg("--capability")
        .arg("tools.echo")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("echo"))
        .stdout(predicate::str::contains("tool"));
}

#[test]
fn stdio_list_resources_shows_items() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("integ");

    mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("list")
        .arg("--capability")
        .arg("resources")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("resource"));
}

#[test]
fn stdio_list_prompts_shows_items() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("integ");

    mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("list")
        .arg("--capability")
        .arg("prompts")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("prompt"));
}

// ---------------------------------------------------------------------------
// Stdio Transport: doctor
// ---------------------------------------------------------------------------

#[test]
fn stdio_doctor_shows_server_info() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("integ");

    // Run a discover first to populate the cache
    mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("discover")
        .arg("capabilities")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success();

    // Doctor should now show cached capabilities
    mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("doctor")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("server:"))
        .stdout(predicate::str::contains("protocol"));
}

// ---------------------------------------------------------------------------
// Auth: token store flow
// ---------------------------------------------------------------------------

#[test]
fn stdio_auth_login_stores_token() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("integ");

    mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("auth")
        .arg("login")
        .write_stdin("test-integration-token\n")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("authenticated"));
}

#[test]
fn stdio_auth_status_after_login_shows_authenticated() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("integ");

    // Login first
    mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("auth")
        .arg("login")
        .write_stdin("test-token-status\n")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success();

    // Status should show authenticated
    mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("auth")
        .arg("status")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("authenticated"));
}

#[test]
fn stdio_auth_logout_clears_token() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("integ");

    // Login
    mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("auth")
        .arg("login")
        .write_stdin("test-token-logout\n")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success();

    // Logout
    mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("auth")
        .arg("logout")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("logged_out"));

    // Status should show logged out
    mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("auth")
        .arg("status")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("logged_out"));
}

#[test]
fn stdio_auth_status_json_output() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("integ");

    // Login
    mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("auth")
        .arg("login")
        .write_stdin("test-token-json\n")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success();

    // JSON status
    let output = mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("--json")
        .arg("auth")
        .arg("status")
        .timeout(std::time::Duration::from_secs(30))
        .output()
        .expect("command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("output should be valid JSON");
    assert_eq!(parsed["data"]["state"], "authenticated");
}

#[test]
fn stdio_auth_login_accepts_input_json() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("integ");

    // No stdin is piped — the token comes entirely from --input-json.
    mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("auth")
        .arg("login")
        .arg("--input-json")
        .arg(r#"{"bearer_token": "tok-from-json", "account": "ci@example.com"}"#)
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("authenticated"))
        .stdout(predicate::str::contains("ci@example.com"));

    // Status should now report authenticated with the supplied account.
    let output = mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("--json")
        .arg("auth")
        .arg("status")
        .timeout(std::time::Duration::from_secs(30))
        .output()
        .expect("command should run");
    assert!(output.status.success());
    let parsed: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&output.stdout))
        .expect("output should be valid JSON");
    assert_eq!(parsed["data"]["state"], "authenticated");
    assert_eq!(parsed["data"]["account"], "ci@example.com");
}

#[test]
fn stdio_auth_login_rejects_input_json_without_bearer_token() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("integ");

    mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("auth")
        .arg("login")
        .arg("--input-json")
        .arg(r#"{"account": "ci@example.com"}"#)
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .failure()
        .stderr(predicate::str::contains("bearer_token"));
}

// ---------------------------------------------------------------------------
// JSON output for all bridge commands
// ---------------------------------------------------------------------------

#[test]
fn stdio_discover_json_output() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("integ");

    let output = mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("--json")
        .arg("discover")
        .arg("capabilities")
        .timeout(std::time::Duration::from_secs(30))
        .output()
        .expect("command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("output should be valid JSON");
    assert_eq!(parsed["command"], "discover");
    assert!(parsed["data"]["items"].is_array());
}

#[test]
fn stdio_read_json_output() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("integ");

    let output = mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("--json")
        .arg("read")
        .arg("--uri")
        .arg("demo://resource/static/document/architecture.md")
        .timeout(std::time::Duration::from_secs(30))
        .output()
        .expect("command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("output should be valid JSON");
    assert_eq!(parsed["command"], "read");
    assert!(parsed["data"]["uri"].is_string());
}

#[test]
fn stdio_prompt_json_output() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("integ");

    let output = mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("--json")
        .arg("prompt")
        .arg("run")
        .arg("simple-prompt")
        .timeout(std::time::Duration::from_secs(30))
        .output()
        .expect("command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("output should be valid JSON");
    assert_eq!(parsed["command"], "prompt");
    assert!(parsed["data"]["output"].is_string());
}

// ---------------------------------------------------------------------------
// Config dispatch: active config selection
// ---------------------------------------------------------------------------

#[test]
fn active_config_routes_bridge_commands() {
    let fixture = TestFixture::new();
    fixture.write_stdio_config("integ");

    // Write an active-config selection pointing at "integ"
    let host_dir = fixture.data_dir().join("host");
    std::fs::create_dir_all(&host_dir).expect("host dir should be created");
    std::fs::write(
        host_dir.join("active-config.json"),
        r#"{"config_name":"integ"}"#,
    )
    .expect("active config should be written");

    // Now `mcp2cli invoke ...` should use the active config
    mcp2cli_cmd(&fixture)
        .arg("invoke")
        .arg("--capability")
        .arg("echo")
        .arg("--arg")
        .arg("message=active-config-test")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("active-config-test"));
}

// ---------------------------------------------------------------------------
// Config dispatch: explicit --config path
// ---------------------------------------------------------------------------

#[test]
fn explicit_config_path_overrides_named_lookup() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("custom");

    // Use --config with an arbitrary path (not in the named config directory)
    mcp2cli_with_config(&fixture, "custom", &config_path)
        .arg("discover")
        .arg("capabilities")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("echo"));
}

// ---------------------------------------------------------------------------
// Error cases
// ---------------------------------------------------------------------------

#[test]
fn missing_config_returns_error() {
    let fixture = TestFixture::new();

    mcp2cli_cmd(&fixture)
        .arg("nonexistent-config")
        .arg("discover")
        .arg("capabilities")
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .failure();
}

#[test]
fn invoke_unknown_tool_returns_error() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("integ");

    // First populate the cache by discovering
    mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("discover")
        .arg("capabilities")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success();

    // Now try to invoke a non-existent tool
    mcp2cli_with_config(&fixture, "integ", &config_path)
        .arg("invoke")
        .arg("--capability")
        .arg("nonexistent-tool-xyz")
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .failure()
        .stderr(predicate::str::contains("nonexistent-tool-xyz"));
}

// ---------------------------------------------------------------------------
// Host commands work without config
// ---------------------------------------------------------------------------

#[test]
fn host_config_list_succeeds_empty() {
    let fixture = TestFixture::new();

    mcp2cli_cmd(&fixture)
        .arg("config")
        .arg("list")
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// Link: create and invoke through symlink
// ---------------------------------------------------------------------------

#[test]
fn link_create_produces_symlink() {
    let fixture = TestFixture::new();
    fixture.write_stdio_config("mylink");

    let link_dir = fixture.dir.path().join("links");

    mcp2cli_cmd(&fixture)
        .arg("link")
        .arg("create")
        .arg("--name")
        .arg("mylink")
        .arg("--dir")
        .arg(&link_dir)
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success()
        .stdout(predicate::str::contains("name: mylink"))
        .stdout(predicate::str::contains("link:"))
        .stdout(predicate::str::contains("target:"));

    let link_path = link_dir.join("mylink");
    assert!(link_path.exists(), "symlink should exist");
    assert!(
        std::fs::symlink_metadata(&link_path)
            .unwrap()
            .file_type()
            .is_symlink(),
        "should be a symlink"
    );
}

#[test]
fn link_create_fails_without_named_config() {
    let fixture = TestFixture::new();
    // No config written for "orphan"

    let link_dir = fixture.dir.path().join("links");

    mcp2cli_cmd(&fixture)
        .arg("link")
        .arg("create")
        .arg("--name")
        .arg("orphan")
        .arg("--dir")
        .arg(&link_dir)
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .failure()
        .stderr(predicate::str::contains("no named config"));
}

#[test]
fn link_create_force_skips_config_check() {
    let fixture = TestFixture::new();
    // No named config, but --force should bypass

    let link_dir = fixture.dir.path().join("links");

    mcp2cli_cmd(&fixture)
        .arg("link")
        .arg("create")
        .arg("--name")
        .arg("forced")
        .arg("--dir")
        .arg(&link_dir)
        .arg("--force")
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success()
        .stdout(predicate::str::contains("name: forced"));
}

#[test]
fn link_create_fails_for_duplicate_without_force() {
    let fixture = TestFixture::new();
    fixture.write_stdio_config("duplink");

    let link_dir = fixture.dir.path().join("links");

    // First creation
    mcp2cli_cmd(&fixture)
        .arg("link")
        .arg("create")
        .arg("--name")
        .arg("duplink")
        .arg("--dir")
        .arg(&link_dir)
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success();

    // Second creation without --force should fail
    mcp2cli_cmd(&fixture)
        .arg("link")
        .arg("create")
        .arg("--name")
        .arg("duplink")
        .arg("--dir")
        .arg(&link_dir)
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .failure()
        .stderr(predicate::str::contains("link already exists"));
}

#[test]
fn link_create_force_replaces_existing() {
    let fixture = TestFixture::new();
    fixture.write_stdio_config("replacelink");

    let link_dir = fixture.dir.path().join("links");

    // First creation
    mcp2cli_cmd(&fixture)
        .arg("link")
        .arg("create")
        .arg("--name")
        .arg("replacelink")
        .arg("--dir")
        .arg(&link_dir)
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success();

    // Second creation with --force succeeds
    mcp2cli_cmd(&fixture)
        .arg("link")
        .arg("create")
        .arg("--name")
        .arg("replacelink")
        .arg("--dir")
        .arg(&link_dir)
        .arg("--force")
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success();
}

#[test]
fn link_create_rejects_reserved_names() {
    let fixture = TestFixture::new();

    let link_dir = fixture.dir.path().join("links");

    // "mcp2cli" is reserved
    mcp2cli_cmd(&fixture)
        .arg("link")
        .arg("create")
        .arg("--name")
        .arg("mcp2cli")
        .arg("--dir")
        .arg(&link_dir)
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .failure()
        .stderr(predicate::str::contains("reserved"));

    // "config" is a host command
    mcp2cli_cmd(&fixture)
        .arg("link")
        .arg("create")
        .arg("--name")
        .arg("config")
        .arg("--dir")
        .arg(&link_dir)
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .failure()
        .stderr(predicate::str::contains("reserved"));
}

// ---------------------------------------------------------------------------
// Alias-flow: symlinked binary routes to config
// ---------------------------------------------------------------------------

#[test]
fn symlinked_binary_invokes_named_config() {
    let fixture = TestFixture::new();
    fixture.write_stdio_config("aliasflow");

    let link_dir = fixture.dir.path().join("links");

    // Create symlink
    mcp2cli_cmd(&fixture)
        .arg("link")
        .arg("create")
        .arg("--name")
        .arg("aliasflow")
        .arg("--dir")
        .arg(&link_dir)
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success();

    // Invoke through the symlink
    let mut cmd = assert_cmd::Command::new(link_dir.join("aliasflow"));
    cmd.env("MCP2CLI_CONFIG_DIR", fixture.config_dir());
    cmd.env("MCP2CLI_DATA_DIR", fixture.data_dir());
    cmd.arg("invoke")
        .arg("--capability")
        .arg("echo")
        .arg("--arg")
        .arg("message=alias-flow-test")
        .timeout(std::time::Duration::from_secs(30));
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("alias-flow-test"));
}

#[test]
fn symlinked_binary_discover_through_alias() {
    let fixture = TestFixture::new();
    fixture.write_stdio_config("aliasdiscover");

    let link_dir = fixture.dir.path().join("links");

    mcp2cli_cmd(&fixture)
        .arg("link")
        .arg("create")
        .arg("--name")
        .arg("aliasdiscover")
        .arg("--dir")
        .arg(&link_dir)
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success();

    let mut cmd = assert_cmd::Command::new(link_dir.join("aliasdiscover"));
    cmd.env("MCP2CLI_CONFIG_DIR", fixture.config_dir());
    cmd.env("MCP2CLI_DATA_DIR", fixture.data_dir());
    cmd.arg("discover")
        .arg("capabilities")
        .timeout(std::time::Duration::from_secs(30));
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("echo"))
        .stdout(predicate::str::contains("tool"));
}

// ---------------------------------------------------------------------------
// Config dispatch: --config= (equals form)
// ---------------------------------------------------------------------------

#[test]
fn config_equals_form_works() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("equalsform");

    let mut cmd = assert_cmd::Command::cargo_bin("mcp2cli").expect("binary should be built");
    cmd.env("MCP2CLI_CONFIG_DIR", fixture.config_dir());
    cmd.env("MCP2CLI_DATA_DIR", fixture.data_dir());
    cmd.env("MCP2CLI_TELEMETRY", "off");
    cmd.arg("equalsform");
    cmd.arg(format!("--config={}", config_path.display()));
    cmd.arg("discover");
    cmd.arg("capabilities");
    cmd.timeout(std::time::Duration::from_secs(30));
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("echo"));
}

// ---------------------------------------------------------------------------
// Config dispatch: --json flag with config name
// ---------------------------------------------------------------------------

#[test]
fn json_flag_before_config_name() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_stdio_config("jsonbefore");

    let output = mcp2cli_with_config(&fixture, "jsonbefore", &config_path)
        .arg("--json")
        .arg("invoke")
        .arg("--capability")
        .arg("echo")
        .arg("--arg")
        .arg("message=json-before-test")
        .timeout(std::time::Duration::from_secs(30))
        .output()
        .expect("command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("output should be valid JSON");
    assert_eq!(parsed["command"], "invoke");
}

// ---------------------------------------------------------------------------
// Config dispatch: no active config, no config name
// ---------------------------------------------------------------------------

#[test]
fn bridge_command_without_config_fails_with_guidance() {
    let fixture = TestFixture::new();

    // No active config set, no config name — should fail with guidance
    mcp2cli_cmd(&fixture)
        .arg("invoke")
        .arg("--capability")
        .arg("echo")
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .failure();
}

// ---------------------------------------------------------------------------
// Use command: set and clear active config
// ---------------------------------------------------------------------------

#[test]
fn use_set_and_show_active_config() {
    let fixture = TestFixture::new();
    fixture.write_stdio_config("usetest");

    // Set active config
    mcp2cli_cmd(&fixture)
        .arg("use")
        .arg("usetest")
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success()
        .stdout(predicate::str::contains("usetest"));

    // Show active config
    mcp2cli_cmd(&fixture)
        .arg("use")
        .arg("--show")
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success()
        .stdout(predicate::str::contains("usetest"));
}

#[test]
fn use_clear_removes_active_config() {
    let fixture = TestFixture::new();
    fixture.write_stdio_config("cleartest");

    // Set active config
    mcp2cli_cmd(&fixture)
        .arg("use")
        .arg("cleartest")
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success();

    // Clear
    mcp2cli_cmd(&fixture)
        .arg("use")
        .arg("--clear")
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success();

    // Show should indicate no active config
    mcp2cli_cmd(&fixture)
        .arg("use")
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success()
        .stdout(predicate::str::contains("mcp2cli use <name>"));
}

// ---------------------------------------------------------------------------
// Demo config: auth with demo backend
// ---------------------------------------------------------------------------

#[test]
fn demo_config_auth_login_uses_demo_backend() {
    let fixture = TestFixture::new();
    let config_path = fixture.write_demo_config("demosrv");

    mcp2cli_with_config(&fixture, "demosrv", &config_path)
        .arg("auth")
        .arg("login")
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success()
        .stdout(predicate::str::contains("authenticated"));
}

// ---------------------------------------------------------------------------
// Config init: create config via host command
// ---------------------------------------------------------------------------

#[test]
fn config_init_creates_named_config() {
    let fixture = TestFixture::new();

    mcp2cli_cmd(&fixture)
        .arg("config")
        .arg("init")
        .arg("--name")
        .arg("newserver")
        .arg("--app")
        .arg("bridge")
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success()
        .stdout(predicate::str::contains("newserver"));

    // The config file should now exist
    let config_path = fixture.config_dir().join("configs").join("newserver.yaml");
    assert!(config_path.exists(), "config file should exist");
}

// ---------------------------------------------------------------------------
// Config list: shows created configs
// ---------------------------------------------------------------------------

#[test]
fn config_list_shows_created_configs() {
    let fixture = TestFixture::new();
    fixture.write_stdio_config("listed1");
    fixture.write_stdio_config("listed2");

    mcp2cli_cmd(&fixture)
        .arg("config")
        .arg("list")
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success()
        .stdout(predicate::str::contains("listed1"))
        .stdout(predicate::str::contains("listed2"));
}

// ---------------------------------------------------------------------------
// Dynamic surface: mapped tool arguments
// ---------------------------------------------------------------------------

/// Write a demo-backed config and cache its inventory so mapped tool
/// subcommands (`tools.echo`, …) are available on the dynamic surface.
fn demo_config_with_inventory(fixture: &TestFixture, name: &str) {
    fixture.write_demo_config(name);
    mcp2cli_cmd(fixture)
        .arg(name)
        .arg("tool")
        .arg("list")
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success();
}

#[test]
fn mapped_tool_accepts_required_field_from_args_json() {
    let fixture = TestFixture::new();
    demo_config_with_inventory(&fixture, "email");

    mcp2cli_cmd(&fixture)
        .arg("email")
        .arg("--json")
        .arg("tools.echo")
        .arg("--args-json")
        .arg(r#"{"message":"hi"}"#)
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""message": "hi""#));
}

#[test]
fn mapped_tool_missing_required_field_is_not_an_unrecognized_subcommand() {
    let fixture = TestFixture::new();
    demo_config_with_inventory(&fixture, "email");

    mcp2cli_cmd(&fixture)
        .arg("email")
        .arg("tools.echo")
        .arg("--args-json")
        .arg("{}")
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "the following required arguments were not provided",
        ))
        .stderr(predicate::str::contains("--message <TEXT>"))
        .stderr(predicate::str::contains("unrecognized subcommand").not());
}

// ---------------------------------------------------------------------------
// Cold start: the first command works without a prior discovery
// ---------------------------------------------------------------------------

#[test]
fn cold_start_discovers_commands_on_first_use() {
    let fixture = TestFixture::new();
    fixture.write_demo_config("email");

    // Nothing has populated the discovery cache: no `tool list`, no `ls`.
    mcp2cli_cmd(&fixture)
        .arg("email")
        .arg("--json")
        .arg("tools.echo")
        .arg("--message")
        .arg("hi")
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""message": "hi""#));
}

#[test]
fn cold_start_ls_works_with_an_empty_cache() {
    let fixture = TestFixture::new();
    fixture.write_demo_config("email");

    mcp2cli_cmd(&fixture)
        .arg("email")
        .arg("ls")
        .arg("--tools")
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success()
        .stdout(predicate::str::contains("tools.echo"));
}

#[test]
fn man_install_reaches_the_host_cli() {
    let fixture = TestFixture::new();
    let man_dir = fixture.dir.path().join("man1");

    mcp2cli_cmd(&fixture)
        .arg("man")
        .arg("install")
        .arg("--dir")
        .arg(&man_dir)
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success();
    assert!(man_dir.join("mcp2cli.1").exists());
}

// ---------------------------------------------------------------------------
// Published CLI: `package init`, then run it the way its launcher does
// ---------------------------------------------------------------------------

/// Scaffold the demo-backed `email` config as a package and return its directory.
fn init_email_package(fixture: &TestFixture, extra_args: &[&str]) -> std::path::PathBuf {
    fixture.write_demo_config("email");
    let out = fixture.dir.path().join("email-cli");
    mcp2cli_cmd(fixture)
        .arg("package")
        .arg("init")
        .arg("--name")
        .arg("email")
        .arg("--about")
        .arg("Send and search Acme Mail from your terminal")
        .arg("--out")
        .arg(&out)
        .args(extra_args)
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success();
    out
}

/// The mcp2cli binary as a package's launcher starts it: named through
/// `MCP2CLI_INVOKED_AS`, bound to the packaged config, on a machine that has no
/// mcp2cli configuration or state of its own.
fn packaged_cli(fixture: &TestFixture, package: &std::path::Path) -> assert_cmd::Command {
    let home = fixture.dir.path().join("home");
    let mut cmd = assert_cmd::Command::cargo_bin("mcp2cli").expect("binary should be built");
    cmd.env_remove("MCP2CLI_CONFIG_DIR")
        .env_remove("MCP2CLI_DATA_DIR")
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("MCP2CLI_TELEMETRY", "off")
        .env("MCP2CLI_INVOKED_AS", "email")
        .env("MCP2CLI_CONFIG", package.join("email.yaml"))
        .env("MCP2CLI_BRANDING__VERSION", "2.0.0")
        .timeout(std::time::Duration::from_secs(10));
    cmd
}

/// Every file below `dir`, as paths relative to it.
fn files_under(dir: &std::path::Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if let Ok(relative) = path.strip_prefix(dir) {
                found.push(relative.to_string_lossy().into_owned());
            }
        }
    }
    found
}

#[test]
fn packaged_cli_presents_only_its_own_identity() {
    let fixture = TestFixture::new();
    let package = init_email_package(&fixture, &[]);

    packaged_cli(&fixture, &package)
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::starts_with(
            "Send and search Acme Mail from your terminal",
        ))
        .stdout(predicate::str::contains("Usage: email [OPTIONS] <COMMAND>"))
        .stdout(predicate::str::contains("tools.echo"))
        .stdout(predicate::str::contains("auth"))
        .stdout(predicate::str::contains("mcp2cli").not())
        .stdout(predicate::str::contains("doctor").not())
        .stdout(predicate::str::contains("--background").not());

    packaged_cli(&fixture, &package)
        .arg("--version")
        .assert()
        .success()
        .stdout("email 2.0.0\n");
}

#[test]
fn packaged_cli_runs_a_command_on_a_machine_without_mcp2cli_state() {
    let fixture = TestFixture::new();
    let package = init_email_package(&fixture, &[]);

    packaged_cli(&fixture, &package)
        .arg("--json")
        .arg("tools.echo")
        .arg("--message")
        .arg("hi")
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""message": "hi""#));

    // Its state is its own: a user's mcp2cli config that happens to be called
    // `email` must never share tokens or cache with this CLI.
    let files = files_under(&fixture.dir.path().join("home"));
    assert!(
        files
            .iter()
            .any(|file| file.ends_with("email/instances/email/state.json")),
        "{files:?}"
    );
    assert!(
        files.iter().all(|file| !file.contains("mcp2cli")),
        "{files:?}"
    );
}

#[test]
fn packaged_cli_exposes_only_the_builtins_it_lists() {
    let fixture = TestFixture::new();
    let package = init_email_package(&fixture, &[]);

    // `auth` is the default for an HTTP server…
    packaged_cli(&fixture, &package)
        .arg("auth")
        .arg("status")
        .assert()
        .success();

    // …everything else of mcp2cli's surface is gone, on both parsers.
    for hidden in [
        &["doctor"][..],
        &["ls"],
        &["tool", "list"],
        &["invoke", "--capability", "tools.echo"],
    ] {
        packaged_cli(&fixture, &package)
            .args(hidden)
            .assert()
            .failure()
            .stderr(predicate::str::contains(format!(
                "unrecognized subcommand '{}'",
                hidden[0]
            )));
    }
}

#[test]
fn packaged_cli_help_comes_from_the_snapshot_when_the_server_is_unreachable() {
    let fixture = TestFixture::new();
    let package = init_email_package(&fixture, &[]);
    assert!(package.join("inventory.json").exists());

    // Same package, but the server can no longer be reached.
    let config = package.join("email.yaml");
    let yaml = std::fs::read_to_string(&config).expect("packaged config should exist");
    std::fs::write(
        &config,
        yaml.replace("https://demo.invalid/mcp", "http://127.0.0.1:9/mcp"),
    )
    .expect("packaged config should be rewritten");

    packaged_cli(&fixture, &package)
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("tools.echo"));
    packaged_cli(&fixture, &package)
        .arg("tools.echo")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--message <TEXT>"));

    // Running the command still needs the server — and says so.
    packaged_cli(&fixture, &package)
        .arg("tools.echo")
        .arg("--message")
        .arg("hi")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand").not());
}

#[test]
fn packaged_cli_without_a_command_list_blames_the_connection_not_the_user() {
    let fixture = TestFixture::new();
    let package = init_email_package(&fixture, &["--no-snapshot"]);
    assert!(!package.join("inventory.json").exists());

    let config = package.join("email.yaml");
    let yaml = std::fs::read_to_string(&config).expect("packaged config should exist");
    std::fs::write(
        &config,
        yaml.replace("https://demo.invalid/mcp", "http://127.0.0.1:9/mcp"),
    )
    .expect("packaged config should be rewritten");

    packaged_cli(&fixture, &package)
        .arg("tools.echo")
        .arg("--message")
        .arg("hi")
        .assert()
        .failure()
        .stderr(predicate::str::contains("could not load the command list"))
        .stderr(predicate::str::contains("email auth login"))
        .stderr(predicate::str::contains("unrecognized subcommand").not());

    // Login must stay reachable in exactly this situation.
    packaged_cli(&fixture, &package)
        .arg("auth")
        .arg("status")
        .assert()
        .success();
    packaged_cli(&fixture, &package)
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("auth"));
}

#[test]
fn package_snapshot_refreshes_the_inventory_from_a_packaged_config() {
    let fixture = TestFixture::new();
    let package = init_email_package(&fixture, &["--no-snapshot"]);
    let snapshot = package.join("inventory.json");

    mcp2cli_cmd(&fixture)
        .arg("package")
        .arg("snapshot")
        .arg("--config")
        .arg(package.join("email.yaml"))
        .arg("--out")
        .arg(&snapshot)
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success()
        .stdout(predicate::str::contains("tools: 2"));

    let written = std::fs::read_to_string(&snapshot).expect("snapshot should be written");
    assert!(written.contains(r#""schema_version": 1"#), "{written}");
    assert!(written.contains("tools.echo"), "{written}");
}

#[test]
fn packaged_cli_cannot_be_turned_back_into_mcp2cli_from_argv() {
    let fixture = TestFixture::new();
    let package = init_email_package(&fixture, &[]);

    // `--url`/`--stdio` select an ad-hoc server for mcp2cli itself. For a CLI
    // bound to one server they are just unknown options (or a tool's own flag).
    for adhoc in [
        &["--url", "https://demo.invalid/mcp", "--help"][..],
        &["--stdio", "cat", "--help"],
    ] {
        let output = packaged_cli(&fixture, &package)
            .args(adhoc)
            .output()
            .expect("cli should run");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!text.contains("mcp2cli"), "{adhoc:?} printed: {text}");
        assert!(!text.contains("doctor"), "{adhoc:?} printed: {text}");
    }

    // `--config` cannot swap the bound config, and cannot reach the static
    // bridge's help, which only that parser knows the option for.
    let output = packaged_cli(&fixture, &package)
        .args(["--config", "/nonexistent/other.yaml", "--help"])
        .output()
        .expect("cli should run");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!text.contains("MCP bridge CLI"), "{text}");
    assert!(!text.contains("not found"), "{text}");

    // None of that may have woken mcp2cli's own telemetry or state.
    let files = files_under(&fixture.dir.path().join("home"));
    assert!(
        files.iter().all(|file| !file.contains("mcp2cli")),
        "{files:?}"
    );
}

#[test]
fn packaged_cli_keeps_its_tokens_apart_under_an_exported_data_dir() {
    let fixture = TestFixture::new();
    let package = init_email_package(&fixture, &[]);

    // The user logs in to their own `email` config…
    mcp2cli_cmd(&fixture)
        .arg("email")
        .arg("auth")
        .arg("login")
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success();

    // …and has MCP2CLI_DATA_DIR exported when running a published CLI that is
    // also called `email`. It must not find itself logged in with those tokens.
    packaged_cli(&fixture, &package)
        .env("MCP2CLI_DATA_DIR", fixture.data_dir())
        .arg("auth")
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("logged_out"));
    assert!(
        fixture
            .data_dir()
            .join("apps/email/instances/email/state.json")
            .exists()
    );
}

#[test]
fn packaged_cli_refuses_background_runs_it_could_not_collect() {
    let fixture = TestFixture::new();
    let package = init_email_package(&fixture, &["--builtin", "auth", "--builtin", "tool"]);

    packaged_cli(&fixture, &package)
        .args(["tool", "call", "tasks.run", "--background"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--background is not available"));
}

#[test]
fn packaged_cli_version_is_one_line_even_without_a_configured_version() {
    let fixture = TestFixture::new();
    let package = init_email_package(&fixture, &[]);

    packaged_cli(&fixture, &package)
        .env_remove("MCP2CLI_BRANDING__VERSION")
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"^email \d+\.\d+\.\d+\n$").expect("valid regex"));

    // A launcher's version is text: `1.10` must not come back as `1.1`.
    packaged_cli(&fixture, &package)
        .env("MCP2CLI_BRANDING__VERSION", "1.10")
        .arg("--version")
        .assert()
        .success()
        .stdout("email 1.10\n");
}

#[test]
fn cold_start_help_against_a_silent_server_returns_within_the_timeout() {
    // Accepts connections and never answers — the worst case for a discovery
    // that runs before `--help` can print anything.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let port = listener
        .local_addr()
        .expect("listener has an address")
        .port();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for stream in listener.incoming().flatten() {
            held.push(stream);
        }
    });

    let fixture = TestFixture::new();
    let config_dir = fixture.config_dir().join("configs");
    std::fs::create_dir_all(&config_dir).expect("config dir should be created");
    std::fs::write(
        config_dir.join("slow.yaml"),
        format!(
            "schema_version: 1\nserver:\n  display_name: Slow Server\n  transport: streamable_http\n  endpoint: http://127.0.0.1:{port}/mcp\nevents:\n  enable_stdio_events: false\n"
        ),
    )
    .expect("config should be written");

    let started = std::time::Instant::now();
    mcp2cli_cmd(&fixture)
        .args(["slow", "--timeout", "2", "--help"])
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: slow"))
        .stdout(predicate::str::contains("could not load the command list"));
    // One budget for the whole discovery, not one operation timeout (120 s by
    // default) per category.
    assert!(
        started.elapsed() < std::time::Duration::from_secs(15),
        "help took {:?}",
        started.elapsed()
    );
}
