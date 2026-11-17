use crate::{
    catbus::Envelope,
    platforms,
    state::{ApiError, ApiResult, AppState},
};
use axum::{
    extract::{Path, Query, State},
    http::header,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

const T: Duration = Duration::from_secs(60);

fn known(platform: &str) -> ApiResult<()> {
    platforms::get(platform).map(|_| ()).ok_or_else(|| ApiError::bad_request(format!("未知平台 {platform}")))
}

/// Account names become a CLI argument and a directory name inside catbus.
fn check_account(name: &str) -> ApiResult<()> {
    let ok = !name.is_empty()
        && name.len() <= 40
        && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
        && !name.starts_with(['-', '.']);
    if ok { Ok(()) } else { Err(ApiError::bad_request("账号名只能包含字母、数字、中文、_ - .，且不能以 - 或 . 开头")) }
}

pub async fn platforms() -> Json<Value> {
    Json(json!(platforms::all()))
}

/// All usable accounts: catbus logins merged with accounts bound to other
/// providers (X official API, xiaohongshu-mcp, sau). Each entry lists the
/// providers that can post for it.
pub async fn list(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let env = s.catbus.run(vec!["auth".into(), "list".into()], None, T).await;
    let ids: Vec<&str> = platforms::all().iter().map(|p| p.id).collect();
    let mut out: Vec<Value> = if env.ok {
        env.data
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|a| a.get("platform").and_then(Value::as_str).is_some_and(|p| ids.contains(&p)))
            .collect()
    } else {
        tracing::warn!("catbus auth list: {}", env.error_text());
        vec![]
    };
    for a in &mut out {
        a["providers"] = json!(["catbus"]);
    }
    let st = s.settings.get().await;
    let mut add = |platform: &str, account: &str, provider: &str, name: String| {
        if let Some(a) = out.iter_mut().find(|a| a["platform"] == platform && a["account"] == account) {
            if let Some(list) = a["providers"].as_array_mut() {
                list.push(json!(provider));
            }
        } else {
            out.push(json!({
                "platform": platform, "endpoint": null, "account": account, "current": false,
                "user": { "id": null, "name": name, "url": null, "avatar": null },
                "method": provider, "updated_at": null, "providers": [provider],
            }));
        }
    };
    for a in &st.x_api.accounts {
        add("x", &a.account, "x-api", format!("{} (@{})", a.name, a.username));
    }
    for i in &st.xhs_mcp.instances {
        add("xhs", &i.account, "xhs-mcp", i.account.clone());
    }
    for a in &st.sau.accounts {
        add(&a.platform, &a.account, "sau", a.sau_account.clone());
    }
    Ok(Json(json!(out)))
}

#[derive(Deserialize)]
pub struct AccountQuery {
    #[serde(default = "default_account")]
    pub account: String,
}

fn default_account() -> String {
    "default".into()
}

pub async fn check(State(s): State<AppState>, Path(platform): Path<String>, Query(q): Query<AccountQuery>) -> ApiResult<Json<Envelope>> {
    known(&platform)?;
    check_account(&q.account)?;
    Ok(Json(s.catbus.run(vec![platform, "auth".into(), "status".into(), "-a".into(), q.account], None, T).await))
}

pub async fn logout(State(s): State<AppState>, Path((platform, account)): Path<(String, String)>) -> ApiResult<Json<Envelope>> {
    known(&platform)?;
    check_account(&account)?;
    Ok(Json(s.catbus.run(vec![platform, "auth".into(), "logout".into(), "-a".into(), account, "-y".into()], None, T).await))
}

#[derive(Deserialize)]
pub struct CookieLogin {
    #[serde(default = "default_account")]
    pub account: String,
    pub cookie: String,
}

/// Import a browser cookie. It goes through stdin so it never appears in argv.
pub async fn cookie_login(State(s): State<AppState>, Path(platform): Path<String>, Json(b): Json<CookieLogin>) -> ApiResult<Json<Envelope>> {
    known(&platform)?;
    check_account(&b.account)?;
    if b.cookie.trim().is_empty() {
        return Err(ApiError::bad_request("cookie 为空"));
    }
    let args = vec![platform, "auth".into(), "login".into(), "--method".into(), "cookie".into(), "--cookie".into(), "-".into(), "-a".into(), b.account];
    Ok(Json(s.catbus.run(args, Some(b.cookie), T).await))
}

pub async fn qr_start(State(s): State<AppState>, Path(platform): Path<String>, Json(b): Json<AccountQuery>) -> ApiResult<impl IntoResponse> {
    let p = platforms::get(&platform).ok_or_else(|| ApiError::bad_request("未知平台"))?;
    if !p.login_methods.contains(&"qrcode") {
        return Err(ApiError::bad_request(format!("{} 不支持扫码登录，请导入 cookie", p.name)));
    }
    check_account(&b.account)?;
    Ok(Json(s.logins.start(&platform, &b.account).await))
}

pub async fn qr_status(State(s): State<AppState>, Path(platform): Path<String>) -> ApiResult<impl IntoResponse> {
    known(&platform)?;
    s.logins.status(&platform).await.map(Json).ok_or_else(ApiError::not_found)
}

pub async fn qr_cancel(State(s): State<AppState>, Path(platform): Path<String>) -> ApiResult<Json<Value>> {
    known(&platform)?;
    s.logins.cancel(&platform).await;
    Ok(Json(json!({ "ok": true })))
}

pub async fn qr_image(State(s): State<AppState>, Path(platform): Path<String>) -> ApiResult<impl IntoResponse> {
    known(&platform)?;
    let bytes = tokio::fs::read(s.logins.qrcode_path(&platform)).await.map_err(|_| ApiError::not_found())?;
    Ok(([(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "no-store")], bytes))
}
