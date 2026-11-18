//! 数据 (analytics), 收件箱 (inbox) and 自动互动 (automation) endpoints.

use crate::{
    db,
    engage::{
        self,
        auto::{self, CommentResult, Rule},
        caps_of,
    },
    settings::AutomationSettings,
    state::{ApiError, ApiResult, AppState},
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, FixedOffset, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sqlx::{sqlite::SqliteRow, Row};
use std::collections::BTreeMap;

const METRICS: [&str; 5] = ["views", "likes", "comments", "collects", "shares"];

fn metrics_of(r: &SqliteRow) -> [Option<i64>; 5] {
    METRICS.map(|m| r.get::<Option<i64>, _>(m))
}

fn metrics_json(v: &[Option<i64>; 5]) -> Value {
    Value::Object(METRICS.iter().zip(v).map(|(k, v)| (k.to_string(), json!(v))).collect())
}

// ------------------------------------------------------------------ analytics

/// One stats snapshot of one work. Snapshots must be grouped by `key`, then sorted by `at`.
pub struct Snap {
    pub key: String,
    pub at: DateTime<Utc>,
    pub published: Option<DateTime<Utc>>,
    pub values: [Option<i64>; 5],
}

/// Per-day gains (local days of `tz`) from consecutive snapshot deltas.
/// A work's first snapshot is only a baseline, unless the work was published
/// inside the range — then it counts from zero. Drops (deleted likes) count as 0.
pub fn daily_gains(snaps: &[Snap], first_day: NaiveDate, days: i64, range_start: DateTime<Utc>, tz: FixedOffset) -> BTreeMap<NaiveDate, [i64; 5]> {
    let mut daily: BTreeMap<NaiveDate, [i64; 5]> = (0..days).map(|i| (first_day + chrono::Duration::days(i), [0; 5])).collect();
    let mut key = "";
    let mut prev: [Option<i64>; 5] = [None; 5];
    for sn in snaps {
        if sn.key != key {
            key = &sn.key;
            prev = if sn.published.is_some_and(|p| p >= range_start) { [Some(0); 5] } else { [None; 5] };
        }
        let day = sn.at.with_timezone(&tz).date_naive();
        for i in 0..5 {
            if let (Some(c), Some(p)) = (sn.values[i], prev[i]) {
                if let Some(bucket) = daily.get_mut(&day) {
                    bucket[i] += (c - p).max(0);
                }
            }
            if sn.values[i].is_some() {
                prev[i] = sn.values[i];
            }
        }
    }
    daily
}

#[derive(Deserialize)]
pub struct SummaryQuery {
    pub days: Option<i64>,
    /// Client UTC offset in minutes (e.g. 480 for UTC+8), for day buckets.
    pub tz: Option<i32>,
}

pub async fn summary(State(s): State<AppState>, Query(q): Query<SummaryQuery>) -> ApiResult<Json<Value>> {
    let days = q.days.unwrap_or(30).clamp(7, 90);
    let tz = FixedOffset::east_opt(q.tz.unwrap_or(480).clamp(-720, 840) * 60).unwrap_or(FixedOffset::east_opt(8 * 3600).expect("valid offset"));
    let today = Utc::now().with_timezone(&tz).date_naive();
    let first_day = today - chrono::Duration::days(days - 1);
    let range_start = first_day.and_hms_opt(0, 0, 0).expect("midnight").and_local_timezone(tz).single().expect("fixed offset").with_timezone(&Utc);

    // Current totals per platform.
    let rows = sqlx::query(
        "SELECT platform, COUNT(*) AS works, COUNT(DISTINCT account) AS accounts,
           SUM(views) AS views, SUM(likes) AS likes, SUM(comments) AS comments, SUM(collects) AS collects, SUM(shares) AS shares,
           MAX(synced_at) AS synced_at
         FROM items GROUP BY platform ORDER BY works DESC",
    )
    .fetch_all(&s.db)
    .await?;
    let platforms: Vec<Value> = rows
        .iter()
        .map(|r| {
            let mut o = metrics_json(&metrics_of(r));
            o["platform"] = json!(r.get::<String, _>("platform"));
            o["works"] = json!(r.get::<i64, _>("works"));
            o["accounts"] = json!(r.get::<i64, _>("accounts"));
            o["synced_at"] = json!(r.get::<Option<String>, _>("synced_at"));
            o
        })
        .collect();
    let mut totals = [0i64; 5];
    for p in &platforms {
        for (i, m) in METRICS.iter().enumerate() {
            totals[i] += p[*m].as_i64().unwrap_or(0);
        }
    }

    // Daily gains from snapshot deltas. Two days of lead-in give each work a baseline.
    let snaps = sqlx::query(
        "SELECT s.platform, s.account, s.item_id, s.at, s.views, s.likes, s.comments, s.collects, s.shares, i.published_at
         FROM item_stats s JOIN items i ON i.platform = s.platform AND i.account = s.account AND i.item_id = s.item_id
         WHERE s.at >= ? ORDER BY s.platform, s.account, s.item_id, s.at",
    )
    .bind(db::ts(range_start - chrono::Duration::days(2)))
    .fetch_all(&s.db)
    .await?;
    let snaps: Vec<Snap> = snaps
        .iter()
        .filter_map(|r| {
            Some(Snap {
                key: format!("{}\u{1}{}\u{1}{}", r.get::<&str, _>("platform"), r.get::<&str, _>("account"), r.get::<&str, _>("item_id")),
                at: DateTime::parse_from_rfc3339(r.get::<&str, _>("at")).ok()?.with_timezone(&Utc),
                published: r.get::<Option<String>, _>("published_at").and_then(|p| DateTime::parse_from_rfc3339(&p).ok()).map(|p| p.with_timezone(&Utc)),
                values: metrics_of(r),
            })
        })
        .collect();
    let daily = daily_gains(&snaps, first_day, days, range_start, tz);
    let week_from = today - chrono::Duration::days(6);
    let mut last7 = [0i64; 5];
    for (d, v) in &daily {
        if *d >= week_from {
            for i in 0..5 {
                last7[i] += v[i];
            }
        }
    }
    let daily: Vec<Value> = daily
        .into_iter()
        .map(|(d, v)| {
            let mut o = metrics_json(&v.map(Some));
            o["date"] = json!(d.format("%Y-%m-%d").to_string());
            o
        })
        .collect();

    let status = s.sync.status.lock().await.clone();
    Ok(Json(json!({
        "platforms": platforms,
        "totals": metrics_json(&totals.map(Some)),
        "last7": metrics_json(&last7.map(Some)),
        "daily": daily,
        "sync": status,
    })))
}

#[derive(Deserialize)]
pub struct ItemsQuery {
    pub platform: Option<String>,
    pub sort: Option<String>,
    pub limit: Option<i64>,
}

pub async fn items(State(s): State<AppState>, Query(q): Query<ItemsQuery>) -> ApiResult<Json<Value>> {
    let sort = q.sort.as_deref().filter(|x| METRICS.contains(x) || *x == "published").unwrap_or("published");
    let rows = sqlx::query(
        "SELECT i.*, t.post_id FROM items i LEFT JOIN post_targets t ON t.id = i.target_id
         WHERE (?1 IS NULL OR i.platform = ?1)",
    )
    .bind(q.platform.as_deref().filter(|p| !p.is_empty()))
    .fetch_all(&s.db)
    .await?;
    // Baseline: the latest snapshot at least 24h old, per work (SQLite returns the
    // row holding MAX(at) for bare columns).
    let base = sqlx::query(
        "SELECT platform, account, item_id, views, likes, comments, collects, shares, MAX(at) AS at
         FROM item_stats WHERE at <= ? GROUP BY platform, account, item_id",
    )
    .bind(db::ts(Utc::now() - chrono::Duration::hours(24)))
    .fetch_all(&s.db)
    .await?;
    let base: BTreeMap<(String, String, String), [Option<i64>; 5]> =
        base.iter().map(|r| ((r.get("platform"), r.get("account"), r.get("item_id")), metrics_of(r))).collect();

    let mut out: Vec<Value> = rows
        .iter()
        .map(|r| {
            let k: (String, String, String) = (r.get("platform"), r.get("account"), r.get("item_id"));
            let cur = metrics_of(r);
            let delta: [Option<i64>; 5] = match base.get(&k) {
                Some(b) => std::array::from_fn(|i| cur[i].zip(b[i]).map(|(c, b)| c - b)),
                None => [None; 5],
            };
            json!({
                "platform": k.0, "account": k.1, "item_id": k.2,
                "kind": r.get::<Option<String>, _>("kind"),
                "title": r.get::<Option<String>, _>("title"),
                "url": r.get::<Option<String>, _>("url"),
                "cover": r.get::<Option<String>, _>("cover"),
                "published_at": r.get::<Option<String>, _>("published_at"),
                "post_id": r.get::<Option<String>, _>("post_id"),
                "synced_at": r.get::<String, _>("synced_at"),
                "stats": metrics_json(&cur),
                "delta24h": metrics_json(&delta),
            })
        })
        .collect();
    let num = |v: &Value| v["stats"][sort].as_i64().unwrap_or(-1);
    if sort == "published" {
        out.sort_by(|a, b| b["published_at"].as_str().cmp(&a["published_at"].as_str()));
    } else {
        out.sort_by_key(|v| std::cmp::Reverse(num(v)));
    }
    out.truncate(q.limit.unwrap_or(100).clamp(1, 500) as usize);
    Ok(Json(json!(out)))
}

pub async fn item_history(State(s): State<AppState>, Path((platform, account, item_id)): Path<(String, String, String)>) -> ApiResult<Json<Value>> {
    let rows = sqlx::query("SELECT * FROM item_stats WHERE platform = ? AND account = ? AND item_id = ? ORDER BY at")
        .bind(&platform)
        .bind(&account)
        .bind(&item_id)
        .fetch_all(&s.db)
        .await?;
    Ok(Json(json!(rows
        .iter()
        .map(|r| {
            let mut o = metrics_json(&metrics_of(r));
            o["at"] = json!(r.get::<String, _>("at"));
            o
        })
        .collect::<Vec<_>>())))
}

pub async fn sync_status(State(s): State<AppState>) -> Json<Value> {
    Json(json!(s.sync.status.lock().await.clone()))
}

pub async fn sync_now(State(s): State<AppState>) -> Json<Value> {
    let running = s.sync.status.lock().await.running;
    if !running {
        s.sync.trigger();
    }
    Json(json!({ "ok": true, "already_running": running }))
}

// ------------------------------------------------------------------ inbox

#[derive(Deserialize)]
pub struct InboxQuery {
    pub kind: Option<String>,
    pub platform: Option<String>,
    #[serde(default)]
    pub unread: bool,
    pub limit: Option<i64>,
    /// Paging: return rows older than this sort key.
    pub before: Option<String>,
}

fn can_reply(kind: &str, platform: &str, external_id: &str) -> bool {
    let Some(c) = caps_of(platform) else { return false };
    match kind {
        // Rows from notices (id prefixed `n`) carry no comment id to reply to.
        "comment" => c.comment_add && !external_id.starts_with('n'),
        "dm" => c.dm_send,
        _ => false,
    }
}

fn inbox_json(r: &SqliteRow) -> Value {
    let (kind, platform, ext): (String, String, String) = (r.get("kind"), r.get("platform"), r.get("external_id"));
    json!({
        "id": r.get::<String, _>("id"),
        "platform": platform,
        "account": r.get::<String, _>("account"),
        "kind": kind,
        "item_id": r.get::<Option<String>, _>("item_id"),
        "item_url": r.get::<Option<String>, _>("item_url"),
        "item_title": r.get::<Option<String>, _>("item_title"),
        "author": {
            "id": r.get::<Option<String>, _>("author_id"),
            "name": r.get::<Option<String>, _>("author_name"),
            "url": r.get::<Option<String>, _>("author_url"),
            "avatar": r.get::<Option<String>, _>("author_avatar"),
        },
        "text": r.get::<Option<String>, _>("text"),
        "created_at": r.get::<Option<String>, _>("created_at"),
        "fetched_at": r.get::<String, _>("fetched_at"),
        "sort_key": r.get::<String, _>("sort_key"),
        "is_read": r.get::<i64, _>("is_read") != 0,
        "replied_at": r.get::<Option<String>, _>("replied_at"),
        "reply_text": r.get::<Option<String>, _>("reply_text"),
        "can_reply": can_reply(&kind, &platform, &ext),
    })
}

pub async fn inbox(State(s): State<AppState>, Query(q): Query<InboxQuery>) -> ApiResult<Json<Value>> {
    let kind = q.kind.as_deref().filter(|k| ["comment", "mention", "dm"].contains(k));
    let platform = q.platform.as_deref().filter(|p| !p.is_empty());
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let rows = sqlx::query(
        "SELECT *, COALESCE(created_at, fetched_at) || id AS sort_key FROM inbox
         WHERE (?1 IS NULL OR kind = ?1) AND (?2 IS NULL OR platform = ?2) AND (?3 = 0 OR is_read = 0)
           AND (?4 IS NULL OR COALESCE(created_at, fetched_at) || id < ?4)
         ORDER BY sort_key DESC LIMIT ?5",
    )
    .bind(kind)
    .bind(platform)
    .bind(q.unread)
    .bind(q.before.as_deref().filter(|b| !b.is_empty()))
    .bind(limit)
    .fetch_all(&s.db)
    .await?;
    let items: Vec<Value> = rows.iter().map(inbox_json).collect();
    let more = items.len() as i64 == limit;
    Ok(Json(json!({ "items": items, "has_more": more, "unread": unread_counts(&s).await? })))
}

async fn unread_counts(s: &AppState) -> ApiResult<Value> {
    let rows = sqlx::query("SELECT kind, COUNT(*) AS n FROM inbox WHERE is_read = 0 GROUP BY kind").fetch_all(&s.db).await?;
    let mut o = Map::new();
    let mut total = 0;
    for r in rows {
        let n: i64 = r.get("n");
        total += n;
        o.insert(r.get("kind"), json!(n));
    }
    o.insert("total".into(), json!(total));
    Ok(Value::Object(o))
}

pub async fn inbox_unread(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    Ok(Json(unread_counts(&s).await?))
}

#[derive(Deserialize)]
pub struct MarkRead {
    #[serde(default)]
    pub ids: Vec<String>,
    #[serde(default)]
    pub all: bool,
    #[serde(default = "yes")]
    pub read: bool,
}

fn yes() -> bool {
    true
}

pub async fn inbox_read(State(s): State<AppState>, Json(b): Json<MarkRead>) -> ApiResult<Json<Value>> {
    let flag = i64::from(b.read);
    if b.all {
        sqlx::query("UPDATE inbox SET is_read = ?").bind(flag).execute(&s.db).await?;
    } else {
        for id in &b.ids {
            sqlx::query("UPDATE inbox SET is_read = ? WHERE id = ?").bind(flag).bind(id).execute(&s.db).await?;
        }
    }
    Ok(Json(json!({ "unread": unread_counts(&s).await? })))
}

#[derive(Deserialize)]
pub struct Reply {
    pub text: String,
}

pub async fn inbox_reply(State(s): State<AppState>, Path(id): Path<String>, Json(b): Json<Reply>) -> ApiResult<Json<Value>> {
    let text = b.text.trim();
    if text.is_empty() {
        return Err(ApiError::bad_request("回复内容为空"));
    }
    if text.chars().count() > 500 {
        return Err(ApiError::bad_request("回复不能超过 500 字"));
    }
    let r = sqlx::query("SELECT *, '' AS sort_key FROM inbox WHERE id = ?").bind(&id).fetch_optional(&s.db).await?.ok_or_else(ApiError::not_found)?;
    let (kind, platform, account, ext): (String, String, String, String) = (r.get("kind"), r.get("platform"), r.get("account"), r.get("external_id"));
    if !can_reply(&kind, &platform, &ext) {
        return Err(ApiError::bad_request("这个平台暂不支持在这里回复，请到平台上回复"));
    }
    let result = if kind == "dm" {
        let env = s
            .catbus
            .run(
                vec![platform.clone(), "msg".into(), "send".into(), "-a".into(), account.clone(), "--conversation".into(), ext.clone(), "--".into(), text.into()],
                None,
                std::time::Duration::from_secs(60),
            )
            .await;
        if env.ok { Ok(()) } else { Err(env.error_text()) }
    } else {
        let item: Option<String> = r.get("item_id");
        let item = item.ok_or_else(|| ApiError::bad_request("缺少作品 ID，无法回复"))?;
        match auto::post_comment(&s, &platform, &account, &item, text, Some(&ext)).await {
            CommentResult::Ok { .. } => Ok(()),
            CommentResult::Err { message, .. } => Err(message),
        }
    };
    if let Err(msg) = result {
        // The platform may still have accepted it; tell the user to check before resending.
        return Err(ApiError::new(StatusCode::BAD_GATEWAY, format!("回复失败（请先到平台确认是否已发出）：{msg}")));
    }
    sqlx::query("UPDATE inbox SET replied_at = ?, reply_text = ?, is_read = 1 WHERE id = ?")
        .bind(db::now())
        .bind(text)
        .bind(&id)
        .execute(&s.db)
        .await?;
    let r = sqlx::query("SELECT *, COALESCE(created_at, fetched_at) || id AS sort_key FROM inbox WHERE id = ?").bind(&id).fetch_one(&s.db).await?;
    Ok(Json(inbox_json(&r)))
}

/// Live message history of a DM conversation (not stored locally).
pub async fn inbox_messages(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let r = sqlx::query("SELECT platform, account, kind, external_id FROM inbox WHERE id = ?").bind(&id).fetch_optional(&s.db).await?.ok_or_else(ApiError::not_found)?;
    let (kind, platform, account, ext): (String, String, String, String) = (r.get("kind"), r.get("platform"), r.get("account"), r.get("external_id"));
    if kind != "dm" {
        return Err(ApiError::bad_request("不是私信会话"));
    }
    let env = s
        .catbus
        .run(
            vec![platform, "msg".into(), "history".into(), "-a".into(), account, "--limit".into(), "30".into(), "--".into(), ext],
            None,
            std::time::Duration::from_secs(60),
        )
        .await;
    if env.ok {
        sqlx::query("UPDATE inbox SET is_read = 1 WHERE id = ?").bind(&id).execute(&s.db).await?;
    }
    Ok(Json(json!(env)))
}

// ------------------------------------------------------------------ automation

pub async fn automation(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let settings = s.settings.get().await.automation;
    let day_ago = db::ts(Utc::now() - chrono::Duration::hours(24));
    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auto_actions WHERE status = 'pending'").fetch_one(&s.db).await?;
    let sent: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auto_actions WHERE status = 'success' AND updated_at >= ?").bind(&day_ago).fetch_one(&s.db).await?;
    let failed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auto_actions WHERE status = 'failed' AND updated_at >= ?").bind(&day_ago).fetch_one(&s.db).await?;
    Ok(Json(json!({
        "settings": settings,
        "rules": auto::rules(&s).await?,
        "caps": engage::CAPS,
        "stats": { "pending": pending, "sent24h": sent, "failed24h": failed },
    })))
}

pub async fn set_automation(State(s): State<AppState>, Json(b): Json<AutomationSettings>) -> ApiResult<Json<Value>> {
    if b.sync_minutes != 0 && !(30..=24 * 60).contains(&b.sync_minutes) {
        return Err(ApiError::bad_request("同步间隔需在 30 分钟到 24 小时之间（0 表示关闭）"));
    }
    if !(1..=90).contains(&b.window_days) {
        return Err(ApiError::bad_request("统计窗口需在 1 到 90 天之间"));
    }
    if b.daily_limit > 200 {
        return Err(ApiError::bad_request("每日上限不能超过 200"));
    }
    let wake = {
        let cur = s.settings.get().await.automation;
        cur.sync_minutes != b.sync_minutes
    };
    s.settings.update(|st| st.automation = b.clone()).await?;
    if wake {
        // Re-arm the timer with the new interval (also runs a sync now).
        s.sync.trigger();
    }
    Ok(Json(json!(b)))
}

pub async fn create_rule(State(s): State<AppState>, Json(mut b): Json<Rule>) -> ApiResult<(StatusCode, Json<Rule>)> {
    normalize(&mut b);
    b.check().map_err(ApiError::bad_request)?;
    b.id = uuid::Uuid::new_v4().to_string();
    b.created_at = db::now();
    sqlx::query("INSERT INTO auto_rules (id, kind, enabled, name, platform, account, keywords, metric, threshold, text, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(&b.id)
        .bind(&b.kind)
        .bind(b.enabled)
        .bind(&b.name)
        .bind(&b.platform)
        .bind(&b.account)
        .bind(serde_json::to_string(&b.keywords)?)
        .bind(&b.metric)
        .bind(b.threshold)
        .bind(&b.text)
        .bind(&b.created_at)
        .execute(&s.db)
        .await?;
    Ok((StatusCode::CREATED, Json(b)))
}

fn normalize(b: &mut Rule) {
    b.name = b.name.trim().to_string();
    b.text = b.text.trim().to_string();
    b.platform = b.platform.take().filter(|p| !p.is_empty());
    b.account = b.account.take().filter(|a| !a.is_empty());
    b.keywords = b.keywords.iter().map(|k| k.trim().to_string()).filter(|k| !k.is_empty()).collect();
    if b.kind == "keyword" {
        b.metric = None;
        b.threshold = None;
    } else {
        b.keywords.clear();
    }
}

pub async fn update_rule(State(s): State<AppState>, Path(id): Path<String>, Json(mut b): Json<Rule>) -> ApiResult<Json<Rule>> {
    normalize(&mut b);
    b.check().map_err(ApiError::bad_request)?;
    let n = sqlx::query("UPDATE auto_rules SET kind = ?, enabled = ?, name = ?, platform = ?, account = ?, keywords = ?, metric = ?, threshold = ?, text = ? WHERE id = ?")
        .bind(&b.kind)
        .bind(b.enabled)
        .bind(&b.name)
        .bind(&b.platform)
        .bind(&b.account)
        .bind(serde_json::to_string(&b.keywords)?)
        .bind(&b.metric)
        .bind(b.threshold)
        .bind(&b.text)
        .bind(&id)
        .execute(&s.db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    // Disabling a rule drops what it already queued.
    if !b.enabled {
        cancel_pending_of(&s, &id).await?;
    }
    let r = sqlx::query("SELECT * FROM auto_rules WHERE id = ?").bind(&id).fetch_one(&s.db).await?;
    Ok(Json(Rule::from_row(&r)))
}

async fn cancel_pending_of(s: &AppState, rule_id: &str) -> ApiResult<()> {
    sqlx::query("UPDATE auto_actions SET status = 'canceled', updated_at = ? WHERE rule_id = ? AND status = 'pending'")
        .bind(db::now())
        .bind(rule_id)
        .execute(&s.db)
        .await?;
    Ok(())
}

pub async fn delete_rule(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    cancel_pending_of(&s, &id).await?;
    let n = sqlx::query("DELETE FROM auto_rules WHERE id = ?").bind(&id).execute(&s.db).await?.rows_affected();
    if n == 0 { Err(ApiError::not_found()) } else { Ok(StatusCode::NO_CONTENT) }
}

#[derive(Deserialize)]
pub struct ActionsQuery {
    pub limit: Option<i64>,
}

pub async fn actions(State(s): State<AppState>, Query(q): Query<ActionsQuery>) -> ApiResult<Json<Value>> {
    let rows = sqlx::query(
        "SELECT a.*, r.name AS rule_name, i.title AS item_title, i.url AS item_url
         FROM auto_actions a
         LEFT JOIN auto_rules r ON r.id = a.rule_id
         LEFT JOIN items i ON i.platform = a.platform AND i.account = a.account AND i.item_id = a.item_id
         ORDER BY CASE a.status WHEN 'pending' THEN 0 WHEN 'running' THEN 0 ELSE 1 END, a.updated_at DESC LIMIT ?",
    )
    .bind(q.limit.unwrap_or(100).clamp(1, 500))
    .fetch_all(&s.db)
    .await?;
    let out: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<String, _>("id"),
                "source": r.get::<String, _>("source"),
                "rule_id": r.get::<Option<String>, _>("rule_id"),
                "rule_name": r.get::<Option<String>, _>("rule_name"),
                "platform": r.get::<String, _>("platform"),
                "account": r.get::<String, _>("account"),
                "item_id": r.get::<String, _>("item_id"),
                "item_title": r.get::<Option<String>, _>("item_title"),
                "item_url": r.get::<Option<String>, _>("item_url"),
                "reply_to": r.get::<Option<String>, _>("reply_to"),
                "text": r.get::<String, _>("text"),
                "status": r.get::<String, _>("status"),
                "run_at": r.get::<String, _>("run_at"),
                "error_code": r.get::<Option<String>, _>("error_code"),
                "error": r.get::<Option<String>, _>("error"),
                "updated_at": r.get::<String, _>("updated_at"),
            })
        })
        .collect();
    Ok(Json(json!(out)))
}

pub async fn cancel_action(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let n = sqlx::query("UPDATE auto_actions SET status = 'canceled', updated_at = ? WHERE id = ? AND status = 'pending'")
        .bind(db::now())
        .bind(&id)
        .execute(&s.db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::new(StatusCode::CONFLICT, "只有排队中的动作可以取消"));
    }
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(key: &str, at: &str, published: Option<&str>, likes: i64) -> Snap {
        let t = |s: &str| DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);
        Snap { key: key.into(), at: t(at), published: published.map(t), values: [None, Some(likes), None, None, None] }
    }

    #[test]
    fn gains_use_deltas_and_local_days() {
        let tz = FixedOffset::east_opt(8 * 3600).unwrap();
        let first = NaiveDate::from_ymd_opt(2026, 11, 10).unwrap();
        let start = DateTime::parse_from_rfc3339("2026-11-10T00:00:00+08:00").unwrap().with_timezone(&Utc);
        let snaps = vec![
            // old work: first snapshot is a baseline only
            snap("a", "2026-11-09T12:00:00Z", Some("2026-10-01T00:00:00Z"), 100),
            snap("a", "2026-11-10T01:00:00Z", Some("2026-10-01T00:00:00Z"), 110),
            // 23:30 UTC on the 10th is the 11th in UTC+8
            snap("a", "2026-11-10T23:30:00Z", Some("2026-10-01T00:00:00Z"), 105),
            snap("a", "2026-11-11T02:00:00Z", Some("2026-10-01T00:00:00Z"), 120),
            // new work published in range: counts from zero
            snap("b", "2026-11-11T03:00:00Z", Some("2026-11-11T01:00:00Z"), 7),
        ];
        let d = daily_gains(&snaps, first, 3, start, tz);
        assert_eq!(d[&first][1], 10);
        assert_eq!(d[&first.succ_opt().unwrap()][1], 15 + 7); // drop to 105 counts 0, then +15
        assert_eq!(d[&NaiveDate::from_ymd_opt(2026, 11, 12).unwrap()][1], 0);
    }
}
