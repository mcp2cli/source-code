//! CLI tests for the `mcp-<server>-<tool>` shim dispatch path.
//!
//! Uses Unix's `CommandExt::arg0` to rename argv[0] without needing a
//! symlink on disk — keeps the fixture hermetic and avoids racing on
//! filesystem cleanup.

use assert_cmd::cargo::CommandCargoExt;
use std::os::unix::process::CommandExt;
use std::process::Command;

fn spawn_as(argv0: &str, args: &[&str], envs: &[(&str, &std::path::Path)]) -> std::process::Output {
    let path = Command::cargo_bin("mcp2cli")
        .expect("binary should be built")
        .get_program()
        .to_owned();
    let mut cmd = Command::new(&path);
    cmd.arg0(argv0);
    for a in args {
        cmd.arg(a);
    }
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("spawn mcp2cli")
}

fn write_cache(dir: &std::path::Path, server: &str, body: &serde_json::Value) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join(format!("{server}.json")),
        serde_json::to_vec_pretty(body).unwrap(),
    )
    .unwrap();
}

#[test]
fn shim_help_prints_tool_description_from_cache() {
    let cache_dir = tempfile::tempdir().unwrap();
    write_cache(
        cache_dir.path(),
        "demo",
        &serde_json::json!({
            "name": "demo",
            "vsock_port": 6001,
            "tools": [
                {"name": "read-file", "description": "read a file from /workspace"},
                {"name": "list-dir", "description": "list a directory"}
            ],
            "allowed_tools": []
        }),
    );

    let out = spawn_as(
        "mcp-demo-read-file",
        &["--help"],
        &[("MCP_CACHE_DIR", cache_dir.path())],
    );
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        out.status.success(),
        "expected success; stdout=`{stdout}` stderr=`{}`",
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(stdout.contains("Server:   demo"), "stdout: {stdout}");
    assert!(stdout.contains("Tool:     read-file"), "stdout: {stdout}");
    assert!(stdout.contains("vsock port 6001"), "stdout: {stdout}");
    assert!(
        stdout.contains("read a file from /workspace"),
        "stdout: {stdout}"
    );
}

#[test]
fn shim_help_warns_when_tool_not_in_allowed_list() {
    let cache_dir = tempfile::tempdir().unwrap();
    write_cache(
        cache_dir.path(),
        "demo",
        &serde_json::json!({
            "name": "demo",
            "vsock_port": 6001,
            "tools": [
                {"name": "read-file", "description": "read"},
                {"name": "write-file", "description": "write"}
            ],
            "allowed_tools": ["read-file"]
        }),
    );
    let out = spawn_as(
        "mcp-demo-write-file",
        &["--help"],
        &[("MCP_CACHE_DIR", cache_dir.path())],
    );
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(out.status.success());
    assert!(
        stdout.contains("NOT in the server's allowed_tools"),
        "stdout: {stdout}"
    );
}

#[test]
fn shim_without_help_reports_no_dial_target_when_env_unset() {
    // Without MCP_SHIM_UNIX_DIR or MCP_HOST_CID the
    // shim exits non-zero and points the operator at the env vars
    // (superseding the older "VSOCK transport pending" message).
    let cache_dir = tempfile::tempdir().unwrap();
    write_cache(
        cache_dir.path(),
        "demo",
        &serde_json::json!({
            "name": "demo",
            "vsock_port": 6001,
            "tools": [{"name": "read-file", "description": "x"}],
            "allowed_tools": []
        }),
    );
    let out = spawn_as(
        "mcp-demo-read-file",
        &["/workspace/README.md"],
        &[("MCP_CACHE_DIR", cache_dir.path())],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no dial target configured")
            && stderr.contains("MCP_SHIM_UNIX_DIR")
            && stderr.contains("MCP_HOST_CID"),
        "stderr: {stderr}"
    );
}

#[test]
fn shim_help_errors_when_cache_missing() {
    let empty_dir = tempfile::tempdir().unwrap();
    let out = spawn_as(
        "mcp-ghost-ping",
        &["--help"],
        &[("MCP_CACHE_DIR", empty_dir.path())],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no tool cache") && stderr.contains("no tool cache"),
        "stderr: {stderr}"
    );
}

#[test]
fn shim_help_with_unknown_tool_notes_stale_cache() {
    let cache_dir = tempfile::tempdir().unwrap();
    write_cache(
        cache_dir.path(),
        "demo",
        &serde_json::json!({
            "name": "demo",
            "vsock_port": 6001,
            "tools": [{"name": "read-file", "description": "x"}],
            "allowed_tools": []
        }),
    );
    let out = spawn_as(
        "mcp-demo-not-declared",
        &["--help"],
        &[("MCP_CACHE_DIR", cache_dir.path())],
    );
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("tool not listed in cache"),
        "stdout: {stdout}"
    );
}
