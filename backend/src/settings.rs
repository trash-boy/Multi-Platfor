//! Provider settings persisted to `data/providers.json` (mode 0600, it holds
//! OAuth tokens). Edited from the 通道设置 page.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Provider try-order per platform, e.g. `xhs: ["catbus", "xhs-mcp", "sau"]`.
    pub order: BTreeMap<String, Vec<String>>,
    pub xhs_mcp: XhsMcpSettings,
    pub x_api: XApiSettings,
    pub sau: SauSettings,
    pub automation: AutomationSettings,
}

/// Data collection and auto-engagement (see engage/).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AutomationSettings {
    /// Pull stats / comments / DMs every N minutes. 0 disables background sync.
    pub sync_minutes: u32,
    /// Only works published within this many days get stats snapshots and comment pulls.
    pub window_days: u32,
    /// Max automatic comments per account per rolling 24h (all rule kinds together).
    pub daily_limit: u32,
    /// Master switch for automatic comments (sync keeps running when off).
    pub auto_enabled: bool,
}

impl Default for AutomationSettings {
    fn default() -> Self {
        Self { sync_minutes: 120, window_days: 14, daily_limit: 20, auto_enabled: true }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct XhsMcpSettings {
    /// One xiaohongshu-mcp process = one logged-in account.
    pub instances: Vec<XhsMcpInstance>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct XhsMcpInstance {
    pub account: String,
    pub url: String,
    pub token: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct XApiSettings {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub accounts: Vec<XApiAccount>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct XApiAccount {
    pub account: String,
    pub user_id: String,
    pub username: String,
    pub name: String,
    pub access_token: String,
    pub refresh_token: String,
    /// RFC3339 UTC.
    pub expires_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SauSettings {
    /// Path or command name of the `sau` CLI.
    pub bin: String,
    pub accounts: Vec<SauAccount>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SauAccount {
    pub platform: String,
    /// Our account name (what the compose page shows).
    pub account: String,
    /// The `--account` name used with `sau <platform> login`.
    pub sau_account: String,
}

pub fn default_order() -> BTreeMap<String, Vec<String>> {
    let o = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    BTreeMap::from([
        ("xhs".into(), o(&["catbus", "xhs-mcp", "sau"])),
        ("x".into(), o(&["x-api", "catbus"])),
        ("xianyu".into(), o(&["catbus"])),
        ("weibo".into(), o(&["catbus", "sau"])),
        ("douyin".into(), o(&["catbus", "sau"])),
        ("bilibili".into(), o(&["catbus", "sau"])),
        ("kuaishou".into(), o(&["catbus", "sau"])),
        ("tencent".into(), o(&["sau"])),
    ])
}

impl Settings {
    /// Order for a platform, restricted to providers the platform supports.
    pub fn order_for(&self, platform: &str, supported: &[&str]) -> Vec<String> {
        let mut out: Vec<String> = self
            .order
            .get(platform)
            .cloned()
            .unwrap_or_else(|| default_order().remove(platform).unwrap_or_default())
            .into_iter()
            .filter(|p| supported.contains(&p.as_str()))
            .collect();
        out.dedup();
        out
    }

    /// Copy with secrets replaced, for the settings page.
    pub fn redacted(&self) -> Settings {
        let mut s = self.clone();
        let mask = |v: &mut String| {
            if !v.is_empty() {
                *v = "••••••".into();
            }
        };
        mask(&mut s.x_api.client_secret);
        for a in &mut s.x_api.accounts {
            mask(&mut a.access_token);
            mask(&mut a.refresh_token);
        }
        for i in &mut s.xhs_mcp.instances {
            mask(&mut i.token);
        }
        s
    }
}

#[derive(Clone)]
pub struct Store {
    path: PathBuf,
    inner: Arc<RwLock<Settings>>,
}

impl Store {
    pub fn load(path: PathBuf) -> anyhow::Result<Self> {
        let mut s: Settings = match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(e) => return Err(e.into()),
        };
        for (k, v) in default_order() {
            s.order.entry(k).or_insert(v);
        }
        if s.sau.bin.is_empty() {
            s.sau.bin = "sau".into();
        }
        Ok(Self { path, inner: Arc::new(RwLock::new(s)) })
    }

    pub async fn get(&self) -> Settings {
        self.inner.read().await.clone()
    }

    /// Apply a mutation and persist atomically.
    pub async fn update<R>(&self, f: impl FnOnce(&mut Settings) -> R) -> anyhow::Result<R> {
        let mut guard = self.inner.write().await;
        let r = f(&mut guard);
        let text = serde_json::to_string_pretty(&*guard)?;
        let tmp = self.path.with_extension("json.tmp");
        tokio::fs::write(&tmp, text).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)).await?;
        }
        tokio::fs::rename(&tmp, &self.path).await?;
        Ok(r)
    }
}
