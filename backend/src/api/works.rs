//! "账号作品" — each logged-in account's real published history, pulled live
//! from the platforms via catbus `<p> user items me`. This is read-only and
//! independent of the app's own publish ledger (post_targets). Results are
//! cached for a few minutes because paging a platform is slow.

use crate::state::{ApiResult, AppState};
use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use tokio::task::JoinSet;

const TTL: Duration = Duration::from_secs(600);
/// Cap pages per account so a huge history can't stall the request.
const MAX_ITEMS: u32 = 500;
const PER_ACCOUNT_TIMEOUT: Duration = Duration::from_secs(120);
/// Recent works kept in the response per account (counts/sums use everything).
const KEEP: usize = 40;

#[derive(Deserialize)]
pub struct WorksQuery {
    #[serde(default)]
    pub refresh: bool,
}

pub async fn works(State(s): State<AppState>, Query(q): Query<WorksQuery>) -> ApiResult<Json<Value>> {
    if !q.refresh {
        if let Some((t, v)) = &*s.works_cache.lock().await {
            if t.elapsed() < TTL {
                return Ok(Json(v.clone()));
            }
        }
    }
    let out = build(&s).await?;
    *s.works_cache.lock().await = Some((Instant::now(), out.clone()));
    Ok(Json(out))
}

async fn build(s: &AppState) -> ApiResult<Value> {
    // Which (platform, account) pairs have a catbus login.
    let listed = s.catbus.run(vec!["auth".into(), "list".into()], None, Duration::from_secs(60)).await;
    let ids: Vec<&str> = crate::platforms::all().iter().map(|p| p.id).collect();
    let pairs: Vec<(String, String)> = listed
        .data
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|a| {
            let p = a.get("platform")?.as_str()?.to_string();
            let acc = a.get("account")?.as_str()?.to_string();
            ids.contains(&p.as_str()).then_some((p, acc))
        })
        .collect();

    let mut set = JoinSet::new();
    for (platform, account) in pairs {
        let cb = s.catbus.clone();
        set.spawn(async move {
            let args = vec![
                platform.clone(),
                "user".into(),
                "items".into(),
                "me".into(),
                "--limit".into(),
                MAX_ITEMS.to_string(),
                "-a".into(),
                account.clone(),
            ];
            let env = cb.run(args, None, PER_ACCOUNT_TIMEOUT).await;
            (platform, account, env)
        });
    }

    let mut accounts = Vec::new();
    let (mut total_count, mut total_likes) = (0i64, 0i64);
    let mut capped = false;
    while let Some(Ok((platform, account, env))) = set.join_next().await {
        if !env.ok {
            accounts.push(json!({
                "platform": platform, "account": account, "ok": false,
                "error": env.error.as_ref().map(|e| format!("{}: {}", e.code, e.message)),
                "count": 0, "likes": 0, "views": 0, "works": [],
            }));
            continue;
        }
        let items = env.data.as_array().cloned().unwrap_or_default();
        let n = items.len();
        if n as u32 >= MAX_ITEMS {
            capped = true;
        }
        let sum = |key: &str| -> i64 {
            items.iter().filter_map(|it| it.get("stats")?.get(key)?.as_i64()).sum()
        };
        let likes = sum("likes");
        let views = sum("views");
        total_count += n as i64;
        total_likes += likes;
        // Items already come newest-first from catbus; keep a slice for display.
        let works: Vec<Value> = items.into_iter().take(KEEP).collect();
        accounts.push(json!({
            "platform": platform, "account": account, "ok": true, "error": Value::Null,
            "count": n, "capped": n as u32 >= MAX_ITEMS,
            "likes": likes, "views": views, "works": works,
        }));
    }
    accounts.sort_by(|a, b| b["count"].as_i64().cmp(&a["count"].as_i64()));

    // Flat, newest-first recent list across all accounts for the dashboard.
    let mut recent: Vec<Value> = accounts
        .iter()
        .filter(|a| a["ok"].as_bool().unwrap_or(false))
        .flat_map(|a| {
            let platform = a["platform"].clone();
            let account = a["account"].clone();
            a["works"].as_array().cloned().unwrap_or_default().into_iter().map(move |mut w| {
                w["_platform"] = platform.clone();
                w["_account"] = account.clone();
                w
            })
        })
        .collect();
    recent.sort_by(|a, b| b["created_at"].as_str().cmp(&a["created_at"].as_str()));
    recent.truncate(30);

    Ok(json!({
        "accounts": accounts,
        "totals": { "count": total_count, "likes": total_likes, "capped": capped },
        "recent": recent,
        "fetched_at": crate::db::now(),
    }))
}
