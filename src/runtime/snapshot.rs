//! Inventory snapshots: a server's discovered capabilities as a portable file.
//!
//! The discovery cache in [`StateStore`](super::StateStore) is per-user and
//! starts empty, so on a fresh machine the dynamic CLI has no commands until a
//! discovery round-trip succeeds — which needs the network, and on a protected
//! server needs a login that has not happened yet. A snapshot is the same
//! inventory captured ahead of time by `mcp2cli package snapshot` and shipped
//! next to the config (`discovery.snapshot`), so `--help` and command parsing
//! work immediately, offline, and before login.

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::state::DiscoveryInventoryView;

/// Current snapshot file format.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// A server inventory captured at `generated_at`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InventorySnapshot {
    pub schema_version: u32,
    pub generated_at: DateTime<Utc>,
    /// Tool that wrote the file, e.g. `mcp2cli 0.2.0`. Informational.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<String>,
    #[serde(default)]
    pub tools: Vec<Value>,
    #[serde(default)]
    pub resources: Vec<Value>,
    #[serde(default)]
    pub resource_templates: Vec<Value>,
    #[serde(default)]
    pub prompts: Vec<Value>,
}

impl InventorySnapshot {
    /// Capture the inventory a config currently has cached.
    pub fn from_inventory(inventory: &DiscoveryInventoryView) -> Self {
        Self {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            generated_at: inventory.updated_at,
            generator: Some(format!("mcp2cli {}", env!("CARGO_PKG_VERSION"))),
            tools: inventory.tools.clone().unwrap_or_default(),
            resources: inventory.resources.clone().unwrap_or_default(),
            resource_templates: inventory.resource_templates.clone().unwrap_or_default(),
            prompts: inventory.prompts.clone().unwrap_or_default(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("failed to read inventory snapshot: {}", path.display()))?;
        let snapshot: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to parse inventory snapshot: {}", path.display()))?;
        if snapshot.schema_version != SNAPSHOT_SCHEMA_VERSION {
            return Err(anyhow!(
                "inventory snapshot {} has schema_version {}, but this runtime reads version {}",
                path.display(),
                snapshot.schema_version,
                SNAPSHOT_SCHEMA_VERSION
            ));
        }
        Ok(snapshot)
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        let mut json =
            serde_json::to_string_pretty(self).context("failed to serialize inventory snapshot")?;
        json.push('\n');
        std::fs::write(path, json)
            .with_context(|| format!("failed to write inventory snapshot: {}", path.display()))
    }

    /// The inventory this snapshot describes, as the cache stores it for
    /// `config_name`. It keeps the snapshot's timestamp, so a later live
    /// discovery is always newer and a re-read of the same file is not.
    pub fn into_inventory(self, config_name: &str, app_id: &str) -> DiscoveryInventoryView {
        DiscoveryInventoryView {
            config_name: config_name.to_owned(),
            app_id: app_id.to_owned(),
            tools: Some(self.tools),
            resources: Some(self.resources),
            resource_templates: (!self.resource_templates.is_empty())
                .then_some(self.resource_templates),
            prompts: Some(self.prompts),
            updated_at: self.generated_at,
        }
    }

    pub fn item_count(&self) -> usize {
        self.tools.len() + self.resources.len() + self.resource_templates.len() + self.prompts.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn snapshot_round_trips_through_a_file_and_back_into_an_inventory() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let path = dir.path().join("inventory.json");
        let inventory = DiscoveryInventoryView {
            config_name: "author-config".to_owned(),
            app_id: "bridge".to_owned(),
            tools: Some(vec![json!({"id": "send", "kind": "tool"})]),
            resources: Some(vec![json!({"uri": "mail://inbox"})]),
            resource_templates: None,
            prompts: None,
            updated_at: Utc::now(),
        };

        InventorySnapshot::from_inventory(&inventory)
            .write(&path)
            .expect("snapshot should be written");
        let restored = InventorySnapshot::load(&path)
            .expect("snapshot should load")
            .into_inventory("email", "bridge");

        // The author's config name stays on the author's machine.
        assert_eq!(restored.config_name, "email");
        assert_eq!(restored.tools, inventory.tools);
        assert_eq!(restored.resources, inventory.resources);
        assert_eq!(restored.resource_templates, None);
        assert_eq!(restored.updated_at, inventory.updated_at);
    }

    #[test]
    fn rejects_a_snapshot_from_a_newer_format() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let path = dir.path().join("inventory.json");
        std::fs::write(
            &path,
            r#"{"schema_version": 99, "generated_at": "2026-01-01T00:00:00Z"}"#,
        )
        .expect("snapshot should be written");

        let error = InventorySnapshot::load(&path).expect_err("newer format must be rejected");
        assert!(error.to_string().contains("schema_version 99"), "{error}");
    }
}
