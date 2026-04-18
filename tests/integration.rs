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
