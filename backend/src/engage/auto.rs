//! Automatic comments. Three sources feed one queue (`auto_actions`):
//!
//! - `first_comment` – a post's own "首评", queued when a target publishes;
//! - `milestone`     – a rule fires once per work when a metric crosses a threshold;
//! - `keyword`       – a rule replies once to a new comment containing a keyword.
//!
//! A single worker drains the queue with guard rails, because these are writes
//! made with the user's account:
//! - per-account cap over a rolling 24h (`daily_limit`), plus a minimum gap;
//! - after a RISK_CONTROL error the account's automation pauses for 24h;
//! - failures are never retried automatically (a comment may have gone out).

use super::{can_comment, jitter, sync::inbox_id};
use crate::{db, state::AppState};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteRow, Row};
use std::time::Duration;

const TICK: Duration = Duration::from_secs(20);
const T: Duration = Duration::from_secs(90);
/// Minimum gap between two automatic comments on the same account.
const ACCOUNT_GAP_SECS: i64 = 90;
const RISK_PAUSE_HOURS: i64 = 24;

pub const METRICS: &[&str] = &["views", "likes", "comments", "collects", "shares"];

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Rule {
    #[serde(default)]
    pub id: String,
    /// keyword | milestone
    pub kind: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub name: String,
    /// None = every platform that supports commenting.
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub metric: Option<String>,
    #[serde(default)]
    pub threshold: Option<i64>,
    pub text: String,
    #[serde(default)]
    pub created_at: String,
}

fn yes() -> bool {
    true
}

impl Rule {
    pub fn from_row(r: &SqliteRow) -> Self {
        Self {
            id: r.get("id"),
            kind: r.get("kind"),
            enabled: r.get::<i64, _>("enabled") != 0,
            name: r.get("name"),
            platform: r.get("platform"),
            account: r.get("account"),
            keywords: serde_json::from_str(r.get::<&str, _>("keywords")).unwrap_or_default(),
            metric: r.get("metric"),
            threshold: r.get("threshold"),
            text: r.get("text"),
            created_at: r.get("created_at"),
        }
    }

    fn applies(&self, platform: &str, account: &str) -> bool {
        self.enabled && self.platform.as_deref().is_none_or(|p| p == platform) && self.account.as_deref().is_none_or(|a| a == account)
    }

    /// Validation shared by create / update.
    pub fn check(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("请填写规则名称".into());
        }
        if self.text.trim().is_empty() {
            return Err("请填写评论内容".into());
        }
        if self.text.chars().count() > 500 {
            return Err("评论内容不能超过 500 字".into());
        }
        if let Some(p) = &self.platform {
            if !can_comment(p) {
                return Err(format!("{p} 暂不支持自动评论（目前只有抖音、B站、X）"));
            }
        }
        match self.kind.as_str() {
            "keyword" => {
                if self.keywords.iter().all(|k| k.trim().is_empty()) {
                    return Err("至少填写一个关键词".into());
                }
            }
            "milestone" => {
                if !self.metric.as_deref().is_some_and(|m| METRICS.contains(&m)) {
                    return Err("请选择指标".into());
                }
                if !self.threshold.is_some_and(|t| t > 0) {
                    return Err("阈值必须大于 0".into());
                }
            }
            _ => return Err("未知规则类型".into()),
        }
        Ok(())
    }
}

pub async fn rules(state: &AppState) -> anyhow::Result<Vec<Rule>> {
    Ok(sqlx::query("SELECT * FROM auto_rules ORDER BY created_at").fetch_all(&state.db).await?.iter().map(Rule::from_row).collect())
}

struct Enqueue<'a> {
    source: &'static str,
    rule_id: Option<&'a str>,
    target_id: Option<&'a str>,
    platform: &'a str,
    account: &'a str,
    item_id: &'a str,
    reply_to: Option<&'a str>,
    dedup: String,
    text: &'a str,
    run_at: DateTime<Utc>,
}

async fn enqueue(state: &AppState, e: Enqueue<'_>) -> anyhow::Result<bool> {
    let now = db::now();
    let n = sqlx::query(
        "INSERT OR IGNORE INTO auto_actions (id, source, rule_id, target_id, platform, account, item_id, reply_to, dedup, text, status, run_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(e.source)
    .bind(e.rule_id)
    .bind(e.target_id)
    .bind(e.platform)
    .bind(e.account)
    .bind(e.item_id)
    .bind(e.reply_to)
    .bind(&e.dedup)
    .bind(e.text)
    .bind(db::ts(e.run_at))
    .bind(&now)
    .bind(&now)
    .execute(&state.db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

fn after(lo_secs: u64, hi_secs: u64) -> DateTime<Utc> {
    Utc::now() + chrono::Duration::from_std(jitter(Duration::from_secs(lo_secs), Duration::from_secs(hi_secs))).unwrap_or_default()
}

/// Called by the publisher when a target succeeds: queue the post's 首评.
pub async fn on_target_success(state: &AppState, target_id: &str) -> anyhow::Result<()> {
    let Some(r) = sqlx::query(
        "SELECT t.platform, t.account, t.item_id, t.item_url, p.first_comment, p.first_comment_delay
         FROM post_targets t JOIN posts p ON p.id = t.post_id WHERE t.id = ?",
    )
    .bind(target_id)
    .fetch_optional(&state.db)
    .await?
    else {
        return Ok(());
    };
    let text: Option<String> = r.get("first_comment");
    let Some(text) = text.filter(|t| !t.trim().is_empty()) else { return Ok(()) };
    let platform: String = r.get("platform");
    if !can_comment(&platform) {
        return Ok(());
    }
    let item: Option<String> = r.get::<Option<String>, _>("item_id").or_else(|| r.get("item_url"));
    let Some(item) = item else {
        tracing::warn!(target_id, "首评跳过：发布结果里没有作品 ID");
        return Ok(());
    };
    let delay = r.get::<Option<i64>, _>("first_comment_delay").unwrap_or(10).clamp(1, 24 * 60);
    let base = Utc::now() + chrono::Duration::minutes(delay);
    let run_at = base + chrono::Duration::seconds((super::rand_u64() % 90) as i64);
    enqueue(
        state,
        Enqueue {
            source: "first_comment",
            rule_id: None,
            target_id: Some(target_id),
            platform: &platform,
            account: &r.get::<String, _>("account"),
            item_id: &item,
            reply_to: None,
            dedup: format!("first:{target_id}"),
            text: &text,
            run_at,
        },
    )
    .await?;
    Ok(())
}

pub struct NewComment {
    pub comment_id: String,
    pub item_id: String,
    pub text: String,
    pub created_at: Option<String>,
}

/// Keyword rules: reply once to new comments that contain a keyword.
/// Only comments written after the rule was created count (no backlog replies).
pub async fn on_new_comments(state: &AppState, platform: &str, account: &str, comments: &[NewComment]) -> anyhow::Result<()> {
    if !can_comment(platform) {
        return Ok(());
    }
    let rules: Vec<Rule> = rules(state).await?.into_iter().filter(|r| r.kind == "keyword" && r.applies(platform, account)).collect();
    for c in comments {
        let Some(at) = c.created_at.as_deref().and_then(db::parse_ts) else { continue };
        let Some(rule) = match_keyword(&rules, &c.text, &at) else { continue };
        enqueue(
            state,
            Enqueue {
                source: "keyword",
                rule_id: Some(&rule.id),
                target_id: None,
                platform,
                account,
                item_id: &c.item_id,
                reply_to: Some(&c.comment_id),
                // One automatic reply per comment, whichever rule matched first.
                dedup: format!("kw:{platform}:{account}:{}", c.comment_id),
                text: &rule.text,
                run_at: after(60, 300),
            },
        )
        .await?;
    }
    Ok(())
}

/// First keyword rule (in creation order) that matches a comment written after the rule existed.
pub fn match_keyword<'a>(rules: &'a [Rule], text: &str, written_at: &str) -> Option<&'a Rule> {
    let lower = text.to_lowercase();
    rules.iter().find(|r| {
        r.kind == "keyword" && r.enabled && written_at >= r.created_at.as_str() && r.keywords.iter().any(|k| !k.trim().is_empty() && lower.contains(&k.trim().to_lowercase()))
    })
}

/// Milestone rules: once per (rule, work), after the work crosses the threshold.
pub async fn evaluate_milestones(state: &AppState) -> anyhow::Result<()> {
    let rules: Vec<Rule> = rules(state).await?.into_iter().filter(|r| r.kind == "milestone" && r.enabled).collect();
    if rules.is_empty() {
        return Ok(());
    }
    let window = state.settings.get().await.automation.window_days.max(1);
    let since = db::ts(Utc::now() - chrono::Duration::days(i64::from(window)));
    for rule in rules {
        let (Some(metric), Some(threshold)) = (rule.metric.as_deref().filter(|m| METRICS.contains(m)), rule.threshold) else { continue };
        // Only works published after the rule exists (and inside the window), so
        // creating a rule never sprays comments over older posts.
        let from = std::cmp::max(since.clone(), rule.created_at.clone());
        let sql = match metric {
            "views" => "SELECT platform, account, item_id FROM items WHERE views >= ? AND published_at >= ?",
            "likes" => "SELECT platform, account, item_id FROM items WHERE likes >= ? AND published_at >= ?",
            "comments" => "SELECT platform, account, item_id FROM items WHERE comments >= ? AND published_at >= ?",
            "collects" => "SELECT platform, account, item_id FROM items WHERE collects >= ? AND published_at >= ?",
            _ => "SELECT platform, account, item_id FROM items WHERE shares >= ? AND published_at >= ?",
        };
        let rows = sqlx::query(sql)
        .bind(threshold)
        .bind(&from)
        .fetch_all(&state.db)
        .await?;
        for r in rows {
            let (platform, account, item_id): (String, String, String) = (r.get("platform"), r.get("account"), r.get("item_id"));
            if !can_comment(&platform) || !rule.applies(&platform, &account) {
                continue;
            }
            enqueue(
                state,
                Enqueue {
                    source: "milestone",
                    rule_id: Some(&rule.id),
                    target_id: None,
                    platform: &platform,
                    account: &account,
                    item_id: &item_id,
                    reply_to: None,
                    dedup: format!("ms:{}:{platform}:{account}:{item_id}", rule.id),
                    text: &rule.text,
                    run_at: after(60, 600),
                },
            )
            .await?;
        }
    }
    Ok(())
}

pub enum CommentResult {
    Ok { id: Option<String> },
    Err { code: String, message: String },
}

/// Post a comment (or a reply) through catbus.
pub async fn post_comment(state: &AppState, platform: &str, account: &str, item: &str, text: &str, reply_to: Option<&str>) -> CommentResult {
    let mut args: Vec<String> = vec![platform.into(), "comment".into(), "add".into(), "-a".into(), account.into(), "-y".into()];
    if let Some(r) = reply_to {
        args.extend(["--reply-to".into(), r.into()]);
    }
    args.extend(["--".into(), item.into(), text.into()]);
    let env = state.catbus.run(args, None, T).await;
    if env.ok {
        let id = env.data.get("id").and_then(|v| v.as_str().map(String::from).or_else(|| v.as_i64().map(|n| n.to_string())));
        CommentResult::Ok { id }
    } else {
        CommentResult::Err { code: env.error_code().unwrap_or("ERROR").into(), message: env.error_text() }
    }
}

pub async fn run(state: AppState) {
    // A `running` action means we died mid-request; it may have posted.
    let _ = sqlx::query("UPDATE auto_actions SET status = 'failed', error_code = 'INTERRUPTED', error = '服务重启时中断，可能已经发出，请到平台确认', updated_at = ? WHERE status = 'running'")
        .bind(db::now())
        .execute(&state.db)
        .await;
    let mut ticker = tokio::time::interval(TICK);
    loop {
        ticker.tick().await;
        if let Err(e) = tick(&state).await {
            tracing::error!("auto tick: {e:#}");
        }
    }
}

async fn defer(state: &AppState, id: &str, to: DateTime<Utc>, why: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE auto_actions SET run_at = ?, error = ?, updated_at = ? WHERE id = ?")
        .bind(db::ts(to))
        .bind(why)
        .bind(db::now())
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(())
}

async fn tick(state: &AppState) -> anyhow::Result<()> {
    let settings = state.settings.get().await.automation;
    if !settings.auto_enabled {
        return Ok(());
    }
    let due = sqlx::query("SELECT * FROM auto_actions WHERE status = 'pending' AND run_at <= ? ORDER BY run_at LIMIT 10")
        .bind(db::now())
        .fetch_all(&state.db)
        .await?;
    for a in due {
        let id: String = a.get("id");
        let (platform, account): (String, String) = (a.get("platform"), a.get("account"));
        let day_ago = db::ts(Utc::now() - chrono::Duration::hours(24));

        // Risk-control pause.
        let risk: Option<String> = sqlx::query_scalar(
            "SELECT MAX(updated_at) FROM auto_actions WHERE platform = ? AND account = ? AND error_code = 'RISK_CONTROL' AND updated_at >= ?",
        )
        .bind(&platform)
        .bind(&account)
        .bind(db::ts(Utc::now() - chrono::Duration::hours(RISK_PAUSE_HOURS)))
        .fetch_one(&state.db)
        .await?;
        if let Some(at) = risk.and_then(|t| DateTime::parse_from_rfc3339(&t).ok()) {
            let until = at.with_timezone(&Utc) + chrono::Duration::hours(RISK_PAUSE_HOURS);
            defer(state, &id, until, "账号最近触发过风控，自动评论暂停 24 小时后再发").await?;
            continue;
        }

        // Rolling 24h cap.
        let used: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auto_actions WHERE platform = ? AND account = ? AND status IN ('success', 'running') AND updated_at >= ?")
            .bind(&platform)
            .bind(&account)
            .bind(&day_ago)
            .fetch_one(&state.db)
            .await?;
        if used >= i64::from(settings.daily_limit) {
            defer(state, &id, after(3600, 5400), &format!("已达每日上限 {} 条，顺延", settings.daily_limit)).await?;
            continue;
        }

        // Minimum gap per account.
        let last: Option<String> = sqlx::query_scalar("SELECT MAX(updated_at) FROM auto_actions WHERE platform = ? AND account = ? AND status = 'success'")
            .bind(&platform)
            .bind(&account)
            .fetch_one(&state.db)
            .await?;
        if let Some(t) = last.and_then(|t| DateTime::parse_from_rfc3339(&t).ok()) {
            let next = t.with_timezone(&Utc) + chrono::Duration::seconds(ACCOUNT_GAP_SECS);
            if next > Utc::now() {
                defer(state, &id, next + chrono::Duration::seconds((super::rand_u64() % 60) as i64), "和上一条自动评论间隔太近，稍后再发").await?;
                continue;
            }
        }

        let claimed = sqlx::query("UPDATE auto_actions SET status = 'running', error = NULL, updated_at = ? WHERE id = ? AND status = 'pending'")
            .bind(db::now())
            .bind(&id)
            .execute(&state.db)
            .await?
            .rows_affected();
        if claimed != 1 {
            continue;
        }
        let (item, text, reply_to): (String, String, Option<String>) = (a.get("item_id"), a.get("text"), a.get("reply_to"));
        tracing::info!(platform, account, source = a.get::<&str, _>("source"), "auto comment");
        let (status, code, err, cid) = match post_comment(state, &platform, &account, &item, &text, reply_to.as_deref()).await {
            CommentResult::Ok { id } => ("success", None, None, id),
            CommentResult::Err { code, message } => ("failed", Some(code), Some(message), None),
        };
        sqlx::query("UPDATE auto_actions SET status = ?, error_code = ?, error = ?, comment_id = ?, updated_at = ? WHERE id = ?")
            .bind(status)
            .bind(&code)
            .bind(&err)
            .bind(&cid)
            .bind(db::now())
            .bind(&id)
            .execute(&state.db)
            .await?;
        if status == "success" {
            if let Some(r) = &reply_to {
                mark_replied(state, &platform, &account, r, &text).await?;
            }
        }
        // Spread several due actions out instead of firing them back to back.
        tokio::time::sleep(jitter(Duration::from_secs(5), Duration::from_secs(15))).await;
    }
    Ok(())
}

pub async fn mark_replied(state: &AppState, platform: &str, account: &str, comment_id: &str, text: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE inbox SET replied_at = ?, reply_text = ?, is_read = 1 WHERE id = ?")
        .bind(db::now())
        .bind(text)
        .bind(inbox_id(platform, account, "comment", comment_id))
        .execute(&state.db)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(kind: &str, keywords: &[&str], created: &str) -> Rule {
        Rule {
            id: "r".into(),
            kind: kind.into(),
            enabled: true,
            name: "n".into(),
            platform: None,
            account: None,
            keywords: keywords.iter().map(|s| s.to_string()).collect(),
            metric: Some("likes".into()),
            threshold: Some(100),
            text: "谢谢".into(),
            created_at: created.into(),
        }
    }

    #[test]
    fn keyword_match_is_case_insensitive_and_ignores_backlog() {
        let rules = vec![rule("keyword", &["链接", "Link"], "2026-11-18T00:00:00.000Z")];
        assert!(match_keyword(&rules, "求LINK", "2026-11-18T01:00:00.000Z").is_some());
        assert!(match_keyword(&rules, "有链接吗", "2026-11-18T01:00:00.000Z").is_some());
        assert!(match_keyword(&rules, "有链接吗", "2026-11-17T23:00:00.000Z").is_none(), "comment older than rule");
        assert!(match_keyword(&rules, "好看", "2026-11-18T01:00:00.000Z").is_none());
    }

    #[test]
    fn rule_validation() {
        assert!(rule("keyword", &["a"], "").check().is_ok());
        assert!(rule("keyword", &[" "], "").check().is_err());
        let mut m = rule("milestone", &[], "");
        assert!(m.check().is_ok());
        m.threshold = Some(0);
        assert!(m.check().is_err());
        let mut p = rule("keyword", &["a"], "");
        p.platform = Some("xhs".into());
        assert!(p.check().is_err(), "xhs cannot comment via catbus");
    }
}
