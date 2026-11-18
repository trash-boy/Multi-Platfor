//! Background collector. Every `sync_minutes` it walks each logged-in catbus
//! account and pulls, in order and with random gaps between calls:
//!
//! 1. own recent works with stats (`user items me`) → `items` + a snapshot in
//!    `item_stats` for works inside the window;
//! 2. comments on works whose comment count grew → `inbox` (kind `comment`);
//! 3. notices (小红书 comments / @, 抖音 @) → `inbox`;
//! 4. DM conversations → `inbox` (kind `dm`, one row per conversation).
//!
//! Accounts run concurrently, calls within one account run sequentially.

use super::{account_args, auto, caps_of, jitter, Caps};
use crate::{db, state::AppState};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::Row;
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::{Mutex, Notify},
    task::JoinSet,
};

const T: Duration = Duration::from_secs(120);
/// Works fetched per account per sync (newest first).
const ITEMS_LIMIT: u32 = 30;
/// Extra `item get` calls per account per sync when the list lacks stats.
const MAX_DETAIL: usize = 10;
/// Works whose comments are pulled per account per sync.
const MAX_COMMENT_PULLS: usize = 8;

#[derive(Serialize, Clone, Default, Debug)]
pub struct AccountSync {
    pub platform: String,
    pub account: String,
    pub ok: bool,
    pub error: Option<String>,
    pub items: usize,
    pub new_inbox: usize,
    pub at: String,
}

#[derive(Serialize, Clone, Default, Debug)]
pub struct Status {
    pub running: bool,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub next_at: Option<String>,
    pub accounts: Vec<AccountSync>,
}

#[derive(Clone, Default)]
pub struct Handle {
    pub status: Arc<Mutex<Status>>,
    pub wake: Arc<Notify>,
}

impl Handle {
    /// Ask the loop to sync now. Ignored when a sync is already running.
    pub fn trigger(&self) {
        self.wake.notify_one();
    }
}

pub async fn run(state: AppState) {
    // Let the server settle (and avoid hammering platforms on quick restarts).
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(90)) => {}
        _ = state.sync.wake.notified() => {}
    }
    loop {
        let minutes = state.settings.get().await.automation.sync_minutes;
        if minutes > 0 {
            sync_all(&state).await;
        }
        let wait = Duration::from_secs(60 * u64::from(if minutes == 0 { 60 } else { minutes }));
        state.sync.status.lock().await.next_at = (minutes > 0).then(|| db::ts(Utc::now() + chrono::Duration::from_std(wait).unwrap_or_default()));
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = state.sync.wake.notified() => {}
        }
    }
}

pub async fn sync_all(state: &AppState) {
    {
        let mut st = state.sync.status.lock().await;
        if st.running {
            return;
        }
        st.running = true;
        st.started_at = Some(db::now());
        st.next_at = None;
    }
    let accounts = logged_in(state).await;
    let mut set = JoinSet::new();
    for (platform, account, self_id) in accounts {
        let Some(caps) = caps_of(&platform) else { continue };
        let st = state.clone();
        set.spawn(async move {
            let res = sync_account(&st, caps, &account, self_id.as_deref()).await;
            match res {
                Ok((items, new_inbox)) => AccountSync { platform, account, ok: true, error: None, items, new_inbox, at: db::now() },
                Err(e) => {
                    tracing::warn!(platform, account, "sync failed: {e:#}");
                    AccountSync { platform, account, ok: false, error: Some(format!("{e:#}")), at: db::now(), ..Default::default() }
                }
            }
        });
    }
    let mut results = Vec::new();
    while let Some(r) = set.join_next().await {
        if let Ok(r) = r {
            results.push(r);
        }
    }
    results.sort_by(|a, b| (&a.platform, &a.account).cmp(&(&b.platform, &b.account)));
    if let Err(e) = auto::evaluate_milestones(state).await {
        tracing::error!("milestone rules: {e:#}");
    }
    let mut st = state.sync.status.lock().await;
    st.running = false;
    st.finished_at = Some(db::now());
    st.accounts = results;
}

/// (platform, account, own user id) for every catbus login.
async fn logged_in(state: &AppState) -> Vec<(String, String, Option<String>)> {
    let env = state.catbus.run(vec!["auth".into(), "list".into()], None, Duration::from_secs(60)).await;
    env.data
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|a| {
            let p = a.get("platform")?.as_str()?.to_string();
            let acc = a.get("account")?.as_str()?.to_string();
            let uid = a.pointer("/user/id").and_then(Value::as_str).map(String::from);
            Some((p, acc, uid))
        })
        .collect()
}

async fn gap() {
    tokio::time::sleep(jitter(Duration::from_secs(2), Duration::from_secs(6))).await;
}

/// Run one catbus command for an account; Err on a failed envelope.
async fn call(state: &AppState, platform: &str, mut args: Vec<String>, account: &str, positional: &[&str]) -> anyhow::Result<crate::catbus::Envelope> {
    let mut full = vec![platform.to_string()];
    full.append(&mut args);
    full.extend(account_args(account));
    if !positional.is_empty() {
        full.push("--".into());
        full.extend(positional.iter().map(|s| s.to_string()));
    }
    let env = state.catbus.run(full, None, T).await;
    if env.ok { Ok(env) } else { Err(anyhow::anyhow!(env.error_text())) }
}

fn s(v: &Value, ptr: &str) -> Option<String> {
    v.pointer(ptr).and_then(|x| match x {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

fn n(v: &Value, key: &str) -> Option<i64> {
    v.get("stats").and_then(|st| st.get(key)).and_then(Value::as_i64)
}

const METRICS: [&str; 5] = ["views", "likes", "comments", "collects", "shares"];

async fn sync_account(state: &AppState, caps: Caps, account: &str, self_id: Option<&str>) -> anyhow::Result<(usize, usize)> {
    let platform = caps.platform;
    let window_days = state.settings.get().await.automation.window_days.max(1);
    let since = Utc::now() - chrono::Duration::days(i64::from(window_days));
    let mut n_items = 0;
    let mut n_new = 0;
    let mut first_error: Option<anyhow::Error> = None;

    // 1. works + stats
    let mut comment_candidates: Vec<(String, String)> = Vec::new(); // (item_id, ref for catbus)
    if caps.stats {
        match call(state, platform, vec!["user".into(), "items".into(), "me".into(), "--limit".into(), ITEMS_LIMIT.to_string()], account, &[]).await {
            Ok(env) => {
                let items = env.data.as_array().cloned().unwrap_or_default();
                n_items = items.len();
                let mut details = 0;
                for mut it in items {
                    let Some(id) = s(&it, "/id") else { continue };
                    let published = s(&it, "/created_at").and_then(|t| DateTime::parse_from_rfc3339(&t).ok()).map(|t| t.with_timezone(&Utc));
                    let recent = published.is_none_or(|p| p >= since);
                    // List endpoints often omit some counters (小红书 only has likes).
                    if recent && details < MAX_DETAIL && (n(&it, "likes").is_none() || n(&it, "comments").is_none()) {
                        let r = s(&it, "/url").unwrap_or_else(|| id.clone());
                        gap().await;
                        details += 1;
                        if let Ok(full) = call(state, platform, vec!["item".into(), "get".into()], account, &[&r]).await {
                            if let Some(stats) = full.data.get("stats").filter(|v| v.is_object()) {
                                for k in METRICS {
                                    if let Some(v) = stats.get(k).filter(|v| !v.is_null()) {
                                        it["stats"][k] = v.clone();
                                    }
                                }
                            }
                        }
                    }
                    let prev_seen = upsert_item(state, platform, account, &id, &it, published.map(db::ts)).await?;
                    if recent {
                        snapshot(state, platform, account, &id, &it).await?;
                        if let Some(c) = n(&it, "comments") {
                            if c > prev_seen.unwrap_or(0) {
                                comment_candidates.push((id.clone(), s(&it, "/url").filter(|_| platform == "xhs").unwrap_or_else(|| id.clone())));
                            }
                        }
                    }
                }
            }
            Err(e) => first_error = Some(e),
        }
    }

    // 2. comments on works with new comments
    if caps.comments {
        for (item_id, r) in comment_candidates.into_iter().take(MAX_COMMENT_PULLS) {
            gap().await;
            let env = match call(state, platform, vec!["comment".into(), "list".into(), "--limit".into(), "20".into()], account, &[&r]).await {
                Ok(e) => e,
                Err(e) => {
                    first_error.get_or_insert(e);
                    continue;
                }
            };
            let (title, url) = item_meta(state, platform, account, &item_id).await;
            let mut fresh = Vec::new();
            for c in env.data.as_array().cloned().unwrap_or_default() {
                let Some(cid) = s(&c, "/id") else { continue };
                if self_id.is_some() && s(&c, "/author/id").as_deref() == self_id {
                    continue;
                }
                let row = InboxRow {
                    kind: "comment",
                    external_id: cid.clone(),
                    item_id: Some(item_id.clone()),
                    item_url: url.clone(),
                    item_title: title.clone(),
                    parent_id: s(&c, "/parent_id"),
                    author: &c["author"],
                    text: s(&c, "/text"),
                    created_at: s(&c, "/created_at"),
                };
                if insert_inbox(state, platform, account, &row).await? {
                    n_new += 1;
                    fresh.push(auto::NewComment { comment_id: cid, item_id: item_id.clone(), text: row.text.clone().unwrap_or_default(), created_at: row.created_at.clone() });
                }
            }
            sqlx::query("UPDATE items SET comments_seen = comments WHERE platform = ? AND account = ? AND item_id = ?")
                .bind(platform)
                .bind(account)
                .bind(&item_id)
                .execute(&state.db)
                .await?;
            if !fresh.is_empty() {
                auto::on_new_comments(state, platform, account, &fresh).await?;
            }
        }
    }

    // 3. notices
    if !caps.notices.is_empty() {
        gap().await;
        match call(state, platform, vec!["notice".into(), "list".into(), "--limit".into(), "30".into()], account, &[]).await {
            Ok(env) => {
                for nt in env.data.as_array().cloned().unwrap_or_default() {
                    let Some(kind) = s(&nt, "/type").filter(|t| caps.notices.contains(&t.as_str())) else { continue };
                    let Some(nid) = s(&nt, "/id") else { continue };
                    let item_id = s(&nt, "/target/id");
                    let (title, _) = match &item_id {
                        Some(i) => item_meta(state, platform, account, i).await,
                        None => (None, None),
                    };
                    let kind: &'static str = if kind == "mention" { "mention" } else { "comment" };
                    let row = InboxRow {
                        kind,
                        external_id: format!("n{nid}"),
                        item_id,
                        item_url: s(&nt, "/target/url"),
                        item_title: title,
                        parent_id: None,
                        author: &nt["user"],
                        text: s(&nt, "/text"),
                        created_at: s(&nt, "/created_at"),
                    };
                    if insert_inbox(state, platform, account, &row).await? {
                        n_new += 1;
                    }
                }
            }
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }

    // 4. DM conversations
    if caps.dm_read {
        gap().await;
        match call(state, platform, vec!["msg".into(), "list".into(), "--limit".into(), "20".into()], account, &[]).await {
            Ok(env) => {
                for c in env.data.as_array().cloned().unwrap_or_default() {
                    let Some(cid) = s(&c, "/id") else { continue };
                    if upsert_dm(state, platform, account, &cid, &c).await? {
                        n_new += 1;
                    }
                }
            }
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }

    // Report failure only when nothing at all worked.
    match first_error {
        Some(e) if n_items == 0 && n_new == 0 => Err(e),
        Some(e) => {
            tracing::warn!(platform, account, "partial sync: {e:#}");
            Ok((n_items, n_new))
        }
        None => Ok((n_items, n_new)),
    }
}

/// Upsert a work; returns the previous `comments_seen` (None for a new row).
async fn upsert_item(state: &AppState, platform: &str, account: &str, id: &str, it: &Value, published: Option<String>) -> anyhow::Result<Option<i64>> {
    let prev: Option<Option<i64>> = sqlx::query_scalar("SELECT comments_seen FROM items WHERE platform = ? AND account = ? AND item_id = ?")
        .bind(platform)
        .bind(account)
        .bind(id)
        .fetch_optional(&state.db)
        .await?;
    let now = db::now();
    sqlx::query(
        "INSERT INTO items (platform, account, item_id, kind, title, url, cover, published_at, views, likes, comments, collects, shares, first_seen, synced_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)
         ON CONFLICT (platform, account, item_id) DO UPDATE SET
           kind = excluded.kind, title = excluded.title, url = excluded.url, cover = excluded.cover,
           published_at = COALESCE(excluded.published_at, items.published_at),
           views = COALESCE(excluded.views, items.views), likes = COALESCE(excluded.likes, items.likes),
           comments = COALESCE(excluded.comments, items.comments), collects = COALESCE(excluded.collects, items.collects),
           shares = COALESCE(excluded.shares, items.shares), synced_at = excluded.synced_at",
    )
    .bind(platform)
    .bind(account)
    .bind(id)
    .bind(s(it, "/kind"))
    .bind(s(it, "/title").or_else(|| s(it, "/text").map(|t| t.chars().take(60).collect())))
    .bind(s(it, "/url"))
    .bind(s(it, "/cover"))
    .bind(published)
    .bind(n(it, "views"))
    .bind(n(it, "likes"))
    .bind(n(it, "comments"))
    .bind(n(it, "collects"))
    .bind(n(it, "shares"))
    .bind(&now)
    .execute(&state.db)
    .await?;
    // Link to the post_target that published it (matched by platform item id).
    sqlx::query(
        "UPDATE items SET target_id = (
            SELECT id FROM post_targets WHERE platform = ?1 AND account = ?2 AND status = 'success'
              AND (item_id = ?3 OR (item_url IS NOT NULL AND instr(item_url, ?3) > 0)) LIMIT 1)
         WHERE platform = ?1 AND account = ?2 AND item_id = ?3 AND target_id IS NULL",
    )
    .bind(platform)
    .bind(account)
    .bind(id)
    .execute(&state.db)
    .await?;
    Ok(prev.flatten().or(prev.map(|_| 0)))
}

async fn snapshot(state: &AppState, platform: &str, account: &str, id: &str, it: &Value) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO item_stats (platform, account, item_id, at, views, likes, comments, collects, shares) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(platform)
        .bind(account)
        .bind(id)
        .bind(db::now())
        .bind(n(it, "views"))
        .bind(n(it, "likes"))
        .bind(n(it, "comments"))
        .bind(n(it, "collects"))
        .bind(n(it, "shares"))
        .execute(&state.db)
        .await?;
    Ok(())
}

async fn item_meta(state: &AppState, platform: &str, account: &str, id: &str) -> (Option<String>, Option<String>) {
    sqlx::query("SELECT title, url FROM items WHERE platform = ? AND account = ? AND item_id = ?")
        .bind(platform)
        .bind(account)
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .map(|r| (r.get("title"), r.get("url")))
        .unwrap_or((None, None))
}

struct InboxRow<'a> {
    kind: &'static str,
    external_id: String,
    item_id: Option<String>,
    item_url: Option<String>,
    item_title: Option<String>,
    parent_id: Option<String>,
    author: &'a Value,
    text: Option<String>,
    created_at: Option<String>,
}

pub fn inbox_id(platform: &str, account: &str, kind: &str, external_id: &str) -> String {
    format!("{platform}:{account}:{kind}:{external_id}")
}

/// Insert if unseen; returns true when the row is new.
async fn insert_inbox(state: &AppState, platform: &str, account: &str, r: &InboxRow<'_>) -> anyhow::Result<bool> {
    let n = sqlx::query(
        "INSERT OR IGNORE INTO inbox (id, platform, account, kind, external_id, item_id, item_url, item_title, parent_id,
           author_id, author_name, author_url, author_avatar, text, created_at, fetched_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(inbox_id(platform, account, r.kind, &r.external_id))
    .bind(platform)
    .bind(account)
    .bind(r.kind)
    .bind(&r.external_id)
    .bind(&r.item_id)
    .bind(&r.item_url)
    .bind(&r.item_title)
    .bind(&r.parent_id)
    .bind(s(r.author, "/id"))
    .bind(s(r.author, "/name"))
    .bind(s(r.author, "/url"))
    .bind(s(r.author, "/avatar"))
    .bind(&r.text)
    .bind(r.created_at.as_deref().and_then(db::parse_ts))
    .bind(db::now())
    .execute(&state.db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

/// One inbox row per conversation; a newer last message marks it unread again.
async fn upsert_dm(state: &AppState, platform: &str, account: &str, cid: &str, c: &Value) -> anyhow::Result<bool> {
    let id = inbox_id(platform, account, "dm", cid);
    let updated = s(c, "/updated_at").as_deref().and_then(db::parse_ts);
    let prev: Option<Option<String>> = sqlx::query_scalar("SELECT created_at FROM inbox WHERE id = ?").bind(&id).fetch_optional(&state.db).await?;
    let changed = match &prev {
        None => true,
        Some(p) => updated.is_some() && *p != updated,
    };
    if !changed {
        return Ok(false);
    }
    // `unread == 0` from the platform means the user already read it there.
    let unread = c.get("unread").and_then(Value::as_i64);
    let is_read = i64::from(unread == Some(0));
    sqlx::query(
        "INSERT INTO inbox (id, platform, account, kind, external_id, author_id, author_name, author_url, author_avatar, text, created_at, fetched_at, is_read)
         VALUES (?, ?, ?, 'dm', ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET text = excluded.text, created_at = excluded.created_at,
           fetched_at = excluded.fetched_at, is_read = excluded.is_read,
           author_name = COALESCE(excluded.author_name, inbox.author_name)",
    )
    .bind(&id)
    .bind(platform)
    .bind(account)
    .bind(cid)
    .bind(s(c, "/peer/id"))
    .bind(s(c, "/peer/name"))
    .bind(s(c, "/peer/url"))
    .bind(s(c, "/peer/avatar"))
    .bind(s(c, "/last_message"))
    .bind(&updated)
    .bind(db::now())
    .bind(is_read)
    .execute(&state.db)
    .await?;
    Ok(is_read == 0)
}
