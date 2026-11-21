use crate::{
    platforms,
    providers::{self, sau, x_api, xhs_mcp},
    settings::{SauAccount, XhsMcpInstance},
    state::{ApiError, ApiResult, AppState},
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

const MASK: &str = "••••••";

fn valid_name(s: &str) -> ApiResult<()> {
    let ok = !s.is_empty() && s.len() <= 40 && s.chars().all(|c| c.is_alphanumeric() || "_-.".contains(c)) && !s.starts_with(['-', '.']);
    if ok { Ok(()) } else { Err(ApiError::bad_request("名称只能包含字母、数字、中文、_ - .，且不能以 - 或 . 开头")) }
}

pub async fn overview(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let settings = s.settings.get().await;
    let catbus = s.catbus.run(vec!["version".into()], None, Duration::from_secs(20)).await;
    let order: serde_json::Map<String, Value> = platforms::all()
        .iter()
        .map(|p| (p.id.to_string(), json!({ "order": settings.order_for(p.id, p.providers), "supported": p.providers })))
        .collect();
    Ok(Json(json!({
        "providers": providers::ALL.iter().map(|(id, label)| json!({ "id": id, "label": label })).collect::<Vec<_>>(),
        "order": order,
        "settings": settings.redacted(),
        "x_redirect_uri": x_api::redirect_uri(&settings.x_api, &s.config.bind),
        "catbus": if catbus.ok { json!({ "ok": true, "version": catbus.data["version"] }) } else { json!({ "ok": false, "error": catbus.error_text() }) },
    })))
}

#[derive(Deserialize)]
pub struct OrderBody {
    pub platform: String,
    pub order: Vec<String>,
}

pub async fn set_order(State(s): State<AppState>, Json(b): Json<OrderBody>) -> ApiResult<Json<Value>> {
    let p = platforms::get(&b.platform).ok_or_else(|| ApiError::bad_request("未知平台"))?;
    if let Some(bad) = b.order.iter().find(|x| !p.providers.contains(&x.as_str())) {
        return Err(ApiError::bad_request(format!("{} 不支持通道 {bad}", p.name)));
    }
    s.settings.update(|st| st.order.insert(b.platform.clone(), b.order.clone())).await?;
    Ok(Json(json!({ "ok": true })))
}

// ---------- X official API ----------

#[derive(Deserialize)]
pub struct XAppBody {
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
    #[serde(default)]
    pub redirect_uri: String,
}

pub async fn set_x_app(State(s): State<AppState>, Json(b): Json<XAppBody>) -> ApiResult<Json<Value>> {
    s.settings
        .update(|st| {
            st.x_api.client_id = b.client_id.trim().into();
            if b.client_secret != MASK {
                st.x_api.client_secret = b.client_secret.trim().into();
            }
            st.x_api.redirect_uri = b.redirect_uri.trim().into();
        })
        .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct AccountParam {
    pub account: String,
}

pub async fn x_start(State(s): State<AppState>, Query(q): Query<AccountParam>) -> ApiResult<Redirect> {
    valid_name(&q.account)?;
    let url = x_api::start(&s, &q.account).await.map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    Ok(Redirect::to(&url))
}

#[derive(Deserialize)]
pub struct Callback {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

pub async fn x_callback(State(s): State<AppState>, Query(q): Query<Callback>) -> Response {
    let result = match (q.code, q.state, q.error) {
        (_, _, Some(e)) => Err(format!("{e} {}", q.error_description.unwrap_or_default())),
        (Some(code), Some(st), None) => x_api::finish(&s, &st, &code).await.map_err(|e| format!("{e:#}")),
        _ => Err("回调缺少 code/state".into()),
    };
    // Relative to /api/oauth/x/callback so it also works behind a path prefix
    // that the reverse proxy strips (e.g. /mpf/api/oauth/x/callback → /mpf/providers).
    let target = match result {
        Ok(acc) => format!("../../../providers?x_ok={}", urlencode(&format!("@{}", acc.username))),
        Err(e) => format!("../../../providers?x_error={}", urlencode(&e)),
    };
    Redirect::to(&target).into_response()
}

fn urlencode(s: &str) -> String {
    reqwest::Url::parse_with_params("http://x/", &[("v", s)]).map(|u| u.query().unwrap_or("").trim_start_matches("v=").to_string()).unwrap_or_default()
}

pub async fn x_remove(State(s): State<AppState>, Path(account): Path<String>) -> ApiResult<StatusCode> {
    s.settings.update(|st| st.x_api.accounts.retain(|a| a.account != account)).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- xiaohongshu-mcp ----------

pub async fn xhs_mcp_upsert(State(s): State<AppState>, Json(mut b): Json<XhsMcpInstance>) -> ApiResult<Json<Value>> {
    valid_name(&b.account)?;
    let url = reqwest::Url::parse(b.url.trim()).map_err(|_| ApiError::bad_request("地址格式错误，例如 http://127.0.0.1:18060"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ApiError::bad_request("地址必须是 http(s)"));
    }
    b.url = b.url.trim().trim_end_matches('/').into();
    s.settings
        .update(|st| {
            let old_token = st.xhs_mcp.instances.iter().find(|i| i.account == b.account).map(|i| i.token.clone()).unwrap_or_default();
            if b.token == MASK {
                b.token = old_token;
            }
            st.xhs_mcp.instances.retain(|i| i.account != b.account);
            st.xhs_mcp.instances.push(b);
        })
        .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn xhs_mcp_remove(State(s): State<AppState>, Path(account): Path<String>) -> ApiResult<StatusCode> {
    s.settings.update(|st| st.xhs_mcp.instances.retain(|i| i.account != account)).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn xhs_mcp_status(State(s): State<AppState>, Path(account): Path<String>) -> ApiResult<Json<Value>> {
    let inst = xhs_mcp::instance(&s, &account).await.ok_or_else(ApiError::not_found)?;
    Ok(Json(xhs_mcp::status(&s, &inst).await))
}

pub async fn xhs_mcp_qrcode(State(s): State<AppState>, Path(account): Path<String>) -> ApiResult<Json<Value>> {
    let inst = xhs_mcp::instance(&s, &account).await.ok_or_else(ApiError::not_found)?;
    xhs_mcp::qrcode(&s, &inst).await.map(Json).map_err(|e| ApiError::new(StatusCode::BAD_GATEWAY, format!("{e:#}")))
}

// ---------- social-auto-upload ----------

#[derive(Deserialize)]
pub struct SauBin {
    pub bin: String,
}

pub async fn sau_set_bin(State(s): State<AppState>, Json(b): Json<SauBin>) -> ApiResult<Json<Value>> {
    let bin = b.bin.trim();
    if bin.is_empty() || bin.starts_with('-') {
        return Err(ApiError::bad_request("路径无效"));
    }
    s.settings.update(|st| st.sau.bin = bin.into()).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn sau_upsert(State(s): State<AppState>, Json(b): Json<SauAccount>) -> ApiResult<Json<Value>> {
    valid_name(&b.account)?;
    valid_name(&b.sau_account)?;
    if sau::sau_platform(&b.platform).is_none() {
        return Err(ApiError::bad_request("sau 不支持该平台"));
    }
    s.settings
        .update(|st| {
            st.sau.accounts.retain(|a| !(a.platform == b.platform && a.account == b.account));
            st.sau.accounts.push(b);
        })
        .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn sau_remove(State(s): State<AppState>, Path((platform, account)): Path<(String, String)>) -> ApiResult<StatusCode> {
    s.settings.update(|st| st.sau.accounts.retain(|a| !(a.platform == platform && a.account == account))).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn sau_check(State(s): State<AppState>, Path((platform, account)): Path<(String, String)>) -> ApiResult<Json<Value>> {
    let st = s.settings.get().await;
    let a = st.sau.accounts.iter().find(|a| a.platform == platform && a.account == account).ok_or_else(ApiError::not_found)?;
    Ok(Json(sau::check(&s, &platform, &a.sau_account).await))
}
