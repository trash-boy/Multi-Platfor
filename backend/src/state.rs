use crate::{catbus::Catbus, config::Config, login::LoginManager, providers::x_api, settings};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: SqlitePool,
    pub catbus: Catbus,
    pub logins: LoginManager,
    pub settings: settings::Store,
    pub http: reqwest::Client,
    pub x_pending: x_api::Pending,
    /// Cached aggregate of each account's published works (see api::works).
    pub works_cache: Arc<tokio::sync::Mutex<Option<(std::time::Instant, serde_json::Value)>>>,
}

/// API error rendered as `{"error": "...", "details": [...]}`.
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
    pub details: Vec<String>,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self { status, message: message.into(), details: vec![] }
    }
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }
    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "不存在")
    }
}

impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(e: E) -> Self {
        let e = e.into();
        tracing::error!("{e:#}");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}"))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message, "details": self.details }))).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
