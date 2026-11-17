//! Publishing providers. Each platform can be served by several independent
//! implementations, tried in a configurable order:
//!
//! | id        | technique                                  | project |
//! |-----------|--------------------------------------------|---------|
//! | `catbus`  | reverse-engineered web signatures, HTTP    | cv-cat/catbus |
//! | `xhs-mcp` | headless browser automation (go-rod)       | xpzouying/xiaohongshu-mcp |
//! | `x-api`   | official X API v2 (OAuth 2.0, pay-per-use) | docs.x.com |
//! | `sau`     | browser automation (patchright)            | dreammis/social-auto-upload |
//!
//! The key safety rule: we only move on to the next provider when the failed
//! one *definitely* did not publish (`safe_to_fallback`). Anything ambiguous
//! stops the chain so a post is never duplicated.

pub mod catbus_p;
pub mod sau;
pub mod x_api;
pub mod xhs_mcp;

use crate::{settings::Settings, state::AppState};
use serde::Serialize;
use serde_json::{Map, Value};
use std::path::PathBuf;

pub const ALL: &[(&str, &str)] = &[
    ("catbus", "catbus（逆向签名）"),
    ("xhs-mcp", "xiaohongshu-mcp（浏览器自动化）"),
    ("x-api", "X 官方 API"),
    ("sau", "social-auto-upload（浏览器自动化）"),
];

pub fn label(id: &str) -> &str {
    ALL.iter().find(|(k, _)| *k == id).map(|(_, v)| *v).unwrap_or(id)
}

/// Everything a provider needs to publish one target.
#[derive(Clone, Debug)]
pub struct Job {
    pub target_id: String,
    pub platform: String,
    pub account: String,
    pub title: Option<String>,
    pub text: String,
    pub images: Vec<PathBuf>,
    pub video: Option<PathBuf>,
    pub cover: Option<PathBuf>,
    pub tags: Vec<String>,
    pub topics: Vec<String>,
    pub visibility: Option<String>,
    pub options: Map<String, Value>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Attempt {
    pub provider: String,
    pub ok: bool,
    pub code: Option<String>,
    pub message: Option<String>,
    pub at: String,
}

pub enum Outcome {
    Success { item_id: Option<String>, item_url: Option<String>, raw: Value },
    Failed {
        code: String,
        message: String,
        /// The provider certainly did not publish anything.
        safe_to_fallback: bool,
        /// Transient network error before anything was sent; retry the same provider later.
        retry_later: bool,
    },
}

impl Outcome {
    pub fn safe(code: &str, message: impl Into<String>) -> Self {
        Outcome::Failed { code: code.into(), message: message.into(), safe_to_fallback: true, retry_later: false }
    }
    pub fn unsafe_(code: &str, message: impl Into<String>) -> Self {
        Outcome::Failed { code: code.into(), message: message.into(), safe_to_fallback: false, retry_later: false }
    }
}

/// Providers able to post for (platform, account), in configured order.
pub fn chain(settings: &Settings, platform: &crate::platforms::Platform, account: &str) -> Vec<String> {
    settings
        .order_for(platform.id, platform.providers)
        .into_iter()
        .filter(|p| bound(settings, p, platform.id, account))
        .collect()
}

/// Whether a provider has credentials for this account. catbus is always tried:
/// when it has no login it fails with AUTH_REQUIRED, which is safe to skip.
pub fn bound(s: &Settings, provider: &str, platform: &str, account: &str) -> bool {
    match provider {
        "catbus" => true,
        "xhs-mcp" => platform == "xhs" && s.xhs_mcp.instances.iter().any(|i| i.account == account),
        "x-api" => platform == "x" && s.x_api.accounts.iter().any(|a| a.account == account),
        "sau" => s.sau.accounts.iter().any(|a| a.platform == platform && a.account == account),
        _ => false,
    }
}

pub async fn publish(state: &AppState, provider: &str, job: &Job) -> Outcome {
    match provider {
        "catbus" => catbus_p::publish(state, job).await,
        "xhs-mcp" => xhs_mcp::publish(state, job).await,
        "x-api" => x_api::publish(state, job).await,
        "sau" => sau::publish(state, job).await,
        other => Outcome::safe("UNSUPPORTED", format!("未知通道 {other}")),
    }
}
