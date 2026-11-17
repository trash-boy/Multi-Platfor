mod api;
mod catbus;
mod config;
mod db;
mod login;
mod platforms;
mod posts;
mod providers;
mod scheduler;
mod settings;
mod state;

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Router,
};
use base64::Engine;
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,sqlx=warn".into()))
        .init();

    let config = config::Config::from_env();
    for dir in [config.data_dir.clone(), config.media_dir(), config.tmp_dir(), config.catbus_home.clone()] {
        std::fs::create_dir_all(&dir)?;
    }
    if !config.catbus_bin.exists() {
        tracing::warn!("找不到 catbus：{}（在 runtime/ 下执行 npm install）", config.catbus_bin.display());
    }

    let catbus = catbus::Catbus { bin: config.catbus_bin.clone(), home: config.catbus_home.clone() };
    let state = state::AppState {
        db: db::connect(&config.data_dir).await?,
        logins: login::LoginManager::new(catbus.clone()),
        catbus,
        settings: settings::Store::load(config.data_dir.join("providers.json"))?,
        http: reqwest::Client::builder().user_agent("mpf-server").connect_timeout(std::time::Duration::from_secs(10)).build()?,
        x_pending: Default::default(),
        works_cache: Default::default(),
        config: Arc::new(config.clone()),
    };
    tokio::spawn(scheduler::run(state.clone()));

    // The app always serves at the root. When exposed under a path prefix
    // (e.g. /mpf/ on the VPS), the reverse proxy strips the prefix before
    // forwarding, so the backend never sees it. The frontend is built with a
    // matching base so its own asset/API URLs carry the prefix in the browser.
    let spa = ServeDir::new(&config.static_dir).fallback(ServeFile::new(config.static_dir.join("index.html")));
    let app = Router::new()
        .nest("/api", api::router())
        .nest_service("/media", ServeDir::new(config.media_dir()))
        .fallback_service(spa)
        .layer(middleware::from_fn_with_state(state.clone(), basic_auth))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&config.bind).await?;
    tracing::info!("listening on http://{}", config.bind);
    if config.password.is_none() && !config.bind.starts_with("127.") && !config.bind.starts_with("localhost") {
        tracing::warn!("服务监听在非本机地址但没有设置 MPF_PASSWORD，任何人都能操作你的账号");
    }
    axum::serve(listener, app).await?;
    Ok(())
}

/// Optional HTTP Basic auth (any username, password = MPF_PASSWORD). Browsers
/// handle it natively, so <img> and media requests work without extra code.
async fn basic_auth(State(s): State<state::AppState>, req: Request, next: Next) -> Response {
    let Some(expected) = s.config.password.as_deref() else {
        return next.run(req).await;
    };
    let ok = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|b| base64::engine::general_purpose::STANDARD.decode(b.trim()).ok())
        .and_then(|raw| String::from_utf8(raw).ok())
        .and_then(|cred| cred.split_once(':').map(|(_, p)| constant_eq(p.as_bytes(), expected.as_bytes())))
        .unwrap_or(false);
    if ok {
        next.run(req).await
    } else {
        (StatusCode::UNAUTHORIZED, [(header::WWW_AUTHENTICATE, "Basic realm=\"mpf\", charset=\"UTF-8\"")], "需要登录").into_response()
    }
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
