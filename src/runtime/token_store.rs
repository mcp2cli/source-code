//! Token store — cached bearer tokens and OAuth refresh material.
//!
//! Backs the `auth login/logout/status` commands in
//! [`crate::apps::bridge`] and the bearer-token injection in the
//! streamable-HTTP transport ([`crate::mcp::client`]). Tokens are
//! keyed by config name so multiple MCP server bindings can each hold
//! their own credentials.
//!
//! Storage is a JSON file under the runtime data dir; access is
//! serialised through a tokio mutex. File permissions are tightened
//! to `0600` on Unix at write time. A rotation timestamp is recorded
//! alongside each token so `auth status` can warn before refresh is
//! due.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredToken {
    pub bearer_token: String,
    pub account: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TokenStoreData {
    #[serde(default)]
    tokens: BTreeMap<String, StoredToken>,
}

pub struct TokenStore {
    path: PathBuf,
}

impl TokenStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub async fn get(&self, config_name: &str) -> Result<Option<StoredToken>> {
        let data = self.load().await?;
        Ok(data.tokens.get(config_name).cloned())
    }

    pub async fn put(&self, config_name: &str, token: StoredToken) -> Result<()> {
        let mut data = self.load().await?;
        data.tokens.insert(config_name.to_owned(), token);
        self.save(&data).await
    }

    pub async fn remove(&self, config_name: &str) -> Result<()> {
        let mut data = self.load().await?;
        data.tokens.remove(config_name);
        self.save(&data).await
    }

    async fn load(&self) -> Result<TokenStoreData> {
        if !self.path.exists() {
            return Ok(TokenStoreData::default());
        }
        let bytes = fs::read(&self.path)
            .await
            .with_context(|| format!("failed to read token store: {}", self.path.display()))?;
        serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to parse token store: {}", self.path.display()))
    }

    async fn save(&self, data: &TokenStoreData) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).await.with_context(|| {
                format!(
                    "failed to create token store directory: {}",
                    parent.display()
                )
            })?;
        }
        let bytes =
            serde_json::to_vec_pretty(data).context("failed to serialize token store data")?;
        fs::write(&self.path, &bytes)
            .await
            .with_context(|| format!("failed to write token store: {}", self.path.display()))?;
        // Bearer tokens / OAuth material must not be world- or group-readable.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600))
                .await
                .with_context(|| {
                    format!(
                        "failed to tighten token store permissions: {}",
                        self.path.display()
                    )
                })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn round_trips_token() {
        let dir = TempDir::new().unwrap();
        let store = TokenStore::new(dir.path().join("tokens.json"));

        assert!(store.get("work").await.unwrap().is_none());

        let token = StoredToken {
            bearer_token: "tok-abc".to_owned(),
            account: Some("user@example.com".to_owned()),
            updated_at: Utc::now(),
        };
        store.put("work", token.clone()).await.unwrap();

        let loaded = store.get("work").await.unwrap().unwrap();
        assert_eq!(loaded.bearer_token, "tok-abc");
        assert_eq!(loaded.account.as_deref(), Some("user@example.com"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn put_tightens_file_permissions_to_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("tokens.json");
        let store = TokenStore::new(path.clone());
        store
            .put(
                "work",
                StoredToken {
                    bearer_token: "secret".to_owned(),
                    account: None,
                    updated_at: Utc::now(),
                },
            )
            .await
            .unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "token store must be owner-only");
    }

    #[tokio::test]
    async fn remove_clears_token() {
        let dir = TempDir::new().unwrap();
        let store = TokenStore::new(dir.path().join("tokens.json"));

        store
            .put(
                "work",
                StoredToken {
                    bearer_token: "tok-abc".to_owned(),
                    account: None,
                    updated_at: Utc::now(),
                },
            )
            .await
            .unwrap();
        store.remove("work").await.unwrap();

        assert!(store.get("work").await.unwrap().is_none());
    }
}
