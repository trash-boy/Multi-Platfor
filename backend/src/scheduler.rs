//! Background publisher. Every few seconds it claims due `pending` targets and
//! walks the platform's provider chain (see providers/mod.rs). Targets of the
//! same account run one at a time, with a short gap, so several posts don't
//! hit one account at once.

use crate::{
    db, platforms,
    providers::{self, Attempt, Job, Outcome},
    posts,
    state::AppState,
};
use chrono::Utc;
use serde_json::Value;
use sqlx::Row;
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::sync::{Mutex, Semaphore};

const TICK: Duration = Duration::from_secs(5);
const MAX_PARALLEL: usize = 3;
const MAX_AUTO_RETRIES: i64 = 3;
/// Gap between two posts on the same account.
const ACCOUNT_GAP: Duration = Duration::from_secs(20);

pub async fn run(state: AppState) {
    // A target still `running` means the server died mid-publish. It may or may not
    // have gone out, so never resend automatically.
    let res = sqlx::query(
        "UPDATE post_targets SET status = 'failed', error_code = 'INTERRUPTED',
         last_error = '服务重启时任务中断，请先到平台确认是否已发布，再决定是否重试', updated_at = ?
         WHERE status = 'running'",
    )
    .bind(db::now())
    .execute(&state.db)
    .await;
    if let Err(e) = res {
        tracing::error!("recover running targets: {e}");
    }

    let busy: Arc<Mutex<HashSet<String>>> = Arc::default();
    let slots = Arc::new(Semaphore::new(MAX_PARALLEL));
    let mut ticker = tokio::time::interval(TICK);
    loop {
        ticker.tick().await;
        if let Err(e) = tick(&state, &busy, &slots).await {
            tracing::error!("scheduler tick: {e:#}");
        }
    }
}

async fn tick(state: &AppState, busy: &Arc<Mutex<HashSet<String>>>, slots: &Arc<Semaphore>) -> anyhow::Result<()> {
    let due = sqlx::query(
        "SELECT id, platform, account FROM post_targets
         WHERE status = 'pending' AND run_at <= ? ORDER BY run_at LIMIT 20",
    )
    .bind(db::now())
    .fetch_all(&state.db)
    .await?;

    for row in due {
        let id: String = row.get("id");
        let key = format!("{}/{}", row.get::<&str, _>("platform"), row.get::<&str, _>("account"));
        let Ok(permit) = slots.clone().try_acquire_owned() else { break };
        {
            let mut b = busy.lock().await;
            if b.contains(&key) {
                continue;
            }
            let claimed = sqlx::query("UPDATE post_targets SET status = 'running', updated_at = ? WHERE id = ? AND status = 'pending'")
                .bind(db::now())
                .bind(&id)
                .execute(&state.db)
                .await?
                .rows_affected();
            if claimed != 1 {
                continue;
            }
            b.insert(key.clone());
        }
        let (state, busy) = (state.clone(), busy.clone());
        tokio::spawn(async move {
            if let Err(e) = execute(&state, &id).await {
                tracing::error!(target_id = %id, "publish crashed: {e:#}");
                let _ = sqlx::query("UPDATE post_targets SET status = 'failed', error_code = 'INTERNAL', last_error = ?, updated_at = ? WHERE id = ?")
                    .bind(format!("{e:#}"))
                    .bind(db::now())
                    .bind(&id)
                    .execute(&state.db)
                    .await;
            }
            tokio::time::sleep(ACCOUNT_GAP).await;
            busy.lock().await.remove(&key);
            drop(permit);
        });
    }
    Ok(())
}

async fn execute(state: &AppState, target_id: &str) -> anyhow::Result<()> {
    let t = posts::load_target(&state.db, target_id).await?.ok_or_else(|| anyhow::anyhow!("target gone"))?;
    let post = posts::load_post(&state.db, &t.post_id).await?.ok_or_else(|| anyhow::anyhow!("post gone"))?;
    let platform = platforms::get(&t.platform).ok_or_else(|| anyhow::anyhow!("未知平台 {}", t.platform))?;

    let media_dir = state.config.media_dir();
    let path = |m: &posts::Media| media_dir.join(&m.stored);
    let job = Job {
        target_id: target_id.to_string(),
        platform: t.platform.clone(),
        account: t.account.clone(),
        title: t.title.clone().or_else(|| post.title.clone()),
        text: t.text.clone().unwrap_or_else(|| post.text.clone()),
        images: post.images.iter().map(path).collect(),
        video: post.video.as_ref().map(path),
        cover: post.cover.as_ref().map(path),
        tags: post.tags.clone(),
        topics: post.topics.clone(),
        visibility: t.visibility.clone(),
        options: t.options.clone(),
    };

    let settings = state.settings.get().await;
    let chain: Vec<String> = match &t.force_provider {
        Some(p) => vec![p.clone()],
        None => providers::chain(&settings, &platform, &t.account),
    };
    let mut log: Vec<Value> = t.attempt_log.clone();
    if chain.is_empty() {
        return finish(state, target_id, "failed", None, Some("NO_PROVIDER"), Some("这个账号没有可用的发布通道，请到「通道设置」配置"), None, None, &log).await;
    }

    let mut last: Option<(String, String, String)> = None; // provider, code, message
    for provider in &chain {
        tracing::info!(target_id, platform = platform.id, account = %t.account, provider, "publishing");
        let outcome = providers::publish(state, provider, &job).await;
        match outcome {
            Outcome::Success { item_id, item_url, raw } => {
                log.push(attempt(provider, true, None, None));
                return finish(state, target_id, "success", Some(provider), None, None, item_id.as_deref(), item_url.as_deref(), &log)
                    .await
                    .and(store_result(state, target_id, &raw).await);
            }
            Outcome::Failed { code, message, safe_to_fallback, retry_later } => {
                log.push(attempt(provider, false, Some(&code), Some(&message)));
                if retry_later && t.attempts + 1 < MAX_AUTO_RETRIES {
                    // Plain network error before anything was sent: retry the whole chain later.
                    let delay = chrono::Duration::minutes(2_i64.pow(t.attempts as u32 + 1));
                    sqlx::query(
                        "UPDATE post_targets SET status = 'pending', attempts = attempts + 1, run_at = ?,
                         error_code = ?, last_error = ?, attempt_log = ?, updated_at = ? WHERE id = ?",
                    )
                    .bind(db::ts(Utc::now() + delay))
                    .bind(&code)
                    .bind(format!("{message}（{} 分钟后自动重试）", delay.num_minutes()))
                    .bind(serde_json::to_string(&log)?)
                    .bind(db::now())
                    .bind(target_id)
                    .execute(&state.db)
                    .await?;
                    return Ok(());
                }
                last = Some((provider.clone(), code, message));
                if !safe_to_fallback {
                    break; // outcome unknown: stop, a human must check the platform
                }
            }
        }
    }
    let (provider, code, message) = last.expect("chain not empty");
    let msg = format!("{}：{}", providers::label(&provider), message);
    finish(state, target_id, "failed", Some(&provider), Some(&code), Some(&msg), None, None, &log).await
}

fn attempt(provider: &str, ok: bool, code: Option<&str>, message: Option<&str>) -> Value {
    serde_json::to_value(Attempt { provider: provider.into(), ok, code: code.map(Into::into), message: message.map(Into::into), at: db::now() }).unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
async fn finish(
    state: &AppState,
    id: &str,
    status: &str,
    provider: Option<&str>,
    code: Option<&str>,
    error: Option<&str>,
    item_id: Option<&str>,
    item_url: Option<&str>,
    log: &[Value],
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE post_targets SET status = ?, attempts = attempts + 1, provider = ?, error_code = ?, last_error = ?,
         item_id = ?, item_url = ?, attempt_log = ?, force_provider = NULL, updated_at = ? WHERE id = ?",
    )
    .bind(status)
    .bind(provider)
    .bind(code)
    .bind(error)
    .bind(item_id)
    .bind(item_url)
    .bind(serde_json::to_string(log)?)
    .bind(db::now())
    .bind(id)
    .execute(&state.db)
    .await?;
    Ok(())
}

async fn store_result(state: &AppState, id: &str, raw: &Value) -> anyhow::Result<()> {
    sqlx::query("UPDATE post_targets SET result = ? WHERE id = ?").bind(serde_json::to_string(raw)?).bind(id).execute(&state.db).await?;
    Ok(())
}
