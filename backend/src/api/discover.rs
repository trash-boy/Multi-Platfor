//! Read side of the aggregator: cross-platform search, feeds, item detail, comments.

use crate::{
    catbus::Envelope,
    platforms,
    providers::xhs_mcp,
    state::{ApiError, ApiResult, AppState},
};
use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::task::JoinSet;

const T: Duration = Duration::from_secs(90);

fn account_args(account: &Option<String>) -> Vec<String> {
    match account.as_deref().filter(|a| !a.is_empty()) {
        Some(a) => vec!["-a".into(), a.into()],
        None => vec![],
    }
}

fn limit(l: Option<u32>) -> String {
    l.unwrap_or(20).clamp(1, 50).to_string()
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    /// Comma separated platform ids; defaults to every searchable platform.
    pub platforms: Option<String>,
    pub limit: Option<u32>,
    pub sort: Option<String>,
}

pub async fn search(State(s): State<AppState>, Query(q): Query<SearchQuery>) -> ApiResult<Json<Value>> {
    let kw = q.q.trim().to_string();
    if kw.is_empty() {
        return Err(ApiError::bad_request("关键词为空"));
    }
    let wanted: Option<Vec<String>> = q.platforms.map(|p| p.split(',').map(|x| x.trim().to_string()).collect());
    let mut set = JoinSet::new();
    for p in platforms::all().into_iter().filter(|p| p.can_search) {
        if wanted.as_ref().is_some_and(|w| !w.iter().any(|x| x == p.id)) {
            continue;
        }
        let mut args = vec![p.id.to_string(), "item".into(), "search".into(), "--limit".into(), limit(q.limit)];
        if let Some(sort) = q.sort.as_deref().filter(|x| ["latest", "popular"].contains(x)) {
            args.extend(["--sort".into(), sort.into()]);
        }
        args.extend(["--".into(), kw.clone()]);
        let (st, kw2) = (s.clone(), kw.clone());
        set.spawn(async move {
            let env = st.catbus.run(args, None, T).await;
            let (env, via) = with_fallback(&st, p.id, env, Some(&kw2)).await;
            (p.id, env, via)
        });
    }
    let mut results: Vec<Value> = Vec::new();
    while let Some(Ok((id, env, via))) = set.join_next().await {
        results.push(json!({ "platform": id, "envelope": env, "via": via }));
    }
    results.sort_by(|a, b| a["platform"].as_str().cmp(&b["platform"].as_str()));
    Ok(Json(json!(results)))
}

#[derive(Deserialize)]
pub struct FeedQuery {
    pub platform: String,
    pub account: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

/// Reads have no side effects, so when catbus fails for 小红书 we transparently
/// retry through xiaohongshu-mcp (browser automation) if one is configured.
async fn with_fallback(s: &AppState, platform: &str, env: Envelope, keyword: Option<&str>) -> (Envelope, &'static str) {
    if env.ok || platform != "xhs" {
        return (env, "catbus");
    }
    let Some(inst) = s.settings.get().await.xhs_mcp.instances.into_iter().next() else {
        return (env, "catbus");
    };
    let alt = xhs_mcp::read(s, &inst, keyword).await;
    if alt.ok { (alt, "xhs-mcp") } else { (env, "catbus") }
}

pub async fn feed(State(s): State<AppState>, Query(q): Query<FeedQuery>) -> ApiResult<Json<Value>> {
    let p = platforms::get(&q.platform).filter(|p| p.can_feed).ok_or_else(|| ApiError::bad_request("该平台不支持信息流"))?;
    let mut args = vec![p.id.to_string(), "feed".into(), "list".into(), "--limit".into(), limit(q.limit)];
    args.extend(account_args(&q.account));
    let first_page = q.cursor.as_deref().is_none_or(str::is_empty);
    if let Some(c) = q.cursor.filter(|c| !c.is_empty()) {
        args.extend(["--cursor".into(), c]);
    }
    let env = s.catbus.run(args, None, T).await;
    let (env, via) = if first_page { with_fallback(&s, p.id, env, None).await } else { (env, "catbus") };
    let mut v = serde_json::to_value(env)?;
    v["via"] = json!(via);
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct ItemQuery {
    pub platform: String,
    /// Item id or URL (xhs needs the URL with xsec_token).
    pub item: String,
    pub account: Option<String>,
    pub limit: Option<u32>,
}

pub async fn item(State(s): State<AppState>, Query(q): Query<ItemQuery>) -> ApiResult<Json<Envelope>> {
    let p = platforms::get(&q.platform).ok_or_else(|| ApiError::bad_request("未知平台"))?;
    let mut args = vec![p.id.to_string(), "item".into(), "get".into()];
    args.extend(account_args(&q.account));
    args.extend(["--".into(), q.item]);
    Ok(Json(s.catbus.run(args, None, T).await))
}

pub async fn comments(State(s): State<AppState>, Query(q): Query<ItemQuery>) -> ApiResult<Json<Envelope>> {
    let p = platforms::get(&q.platform).filter(|p| p.can_comments).ok_or_else(|| ApiError::bad_request("该平台不支持评论列表"))?;
    let mut args = vec![p.id.to_string(), "comment".into(), "list".into(), "--limit".into(), limit(q.limit)];
    args.extend(account_args(&q.account));
    args.extend(["--".into(), q.item]);
    Ok(Json(s.catbus.run(args, None, T).await))
}
