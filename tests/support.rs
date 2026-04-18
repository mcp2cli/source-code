#![allow(dead_code)]

use std::path::{Path, PathBuf};

use tempfile::TempDir;

/// Isolated test fixture with dedicated config and data directories.
pub struct TestFixture {
    pub dir: TempDir,
}

impl TestFixture {
    pub fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("mcp2cli-integ.")
            .tempdir()
            .expect("tempdir should be created");
        Self { dir }
    }

    pub fn config_dir(&self) -> PathBuf {
        self.dir.path().join("config")
    }

    pub fn data_dir(&self) -> PathBuf {
        self.dir.path().join("data")
    }

    /// Write a stdio config YAML that points at server-everything.
    pub fn write_stdio_config(&self, name: &str) -> PathBuf {
        let config_dir = self.config_dir().join("configs");
        std::fs::create_dir_all(&config_dir).expect("config dir should be created");
        let path = config_dir.join(format!("{}.yaml", name));
        let yaml = format!(
            r#"schema_version: 1
app:
  profile: bridge
server:
  display_name: Integration Test Server ({name})
  transport: stdio
  stdio:
    command: npx
    args:
      - '@modelcontextprotocol/server-everything'
    cwd: /tmp
    env: {{}}
events:
  enable_stdio_events: false
"#,
            name = name,
        );
        std::fs::write(&path, yaml).expect("config should be written");
        path
    }

    /// Write a demo config YAML that uses the demo.invalid endpoint.
    pub fn write_demo_config(&self, name: &str) -> PathBuf {
        let config_dir = self.config_dir().join("configs");
        std::fs::create_dir_all(&config_dir).expect("config dir should be created");
        let path = config_dir.join(format!("{}.yaml", name));
        let yaml = format!(
            r#"schema_version: 1
app:
  profile: bridge
server:
  display_name: Demo Test Server ({name})
  transport: streamable_http
  endpoint: https://demo.invalid/mcp
events:
  enable_stdio_events: false
"#,
            name = name,
        );
        std::fs::write(&path, yaml).expect("config should be written");
        path
    }
}

/// Build an `assert_cmd::Command` for the mcp2cli binary with the fixture's
/// environment variables for isolated config/data directories.
pub fn mcp2cli_cmd(fixture: &TestFixture) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::cargo_bin("mcp2cli").expect("binary should be built");
    cmd.env("MCP2CLI_CONFIG_DIR", fixture.config_dir());
    cmd.env("MCP2CLI_DATA_DIR", fixture.data_dir());
    cmd
}

/// Build an `assert_cmd::Command` for the mcp2cli binary, invoked with an
/// explicit `--config` path and a config name (used as the first argv token
/// to simulate link-name invocation).
pub fn mcp2cli_with_config(
    fixture: &TestFixture,
    config_name: &str,
    config_path: &Path,
) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::cargo_bin("mcp2cli").expect("binary should be built");
    cmd.env("MCP2CLI_CONFIG_DIR", fixture.config_dir());
    cmd.env("MCP2CLI_DATA_DIR", fixture.data_dir());
    cmd.arg(config_name);
    cmd.arg("--config");
    cmd.arg(config_path);
    cmd
}
