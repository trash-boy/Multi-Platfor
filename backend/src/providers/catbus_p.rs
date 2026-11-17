use super::{Job, Outcome};
use crate::{
    platforms::{self, PublishInput},
    state::AppState,
};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// catbus error codes that are reported before any write reaches the platform.
/// UPSTREAM / TIMEOUT / INTERNAL may hide a successful post, so they stop the chain.
const SAFE_CODES: &[&str] = &["SPAWN", "AUTH_REQUIRED", "AUTH_EXPIRED", "NOT_IMPLEMENTED", "UNSUPPORTED", "USAGE", "RISK_CONTROL"];

pub async fn publish(state: &AppState, job: &Job) -> Outcome {
    let Some(platform) = platforms::get(&job.platform) else {
        return Outcome::safe("USAGE", "未知平台");
    };
    if platform.publish.is_none() {
        return Outcome::safe("UNSUPPORTED", "catbus 不支持该平台发布");
    }
    let text_file = state.config.tmp_dir().join(format!("{}.txt", job.target_id));
    if let Err(e) = tokio::fs::write(&text_file, &job.text).await {
        return Outcome::safe("INTERNAL", format!("写临时文件失败：{e}"));
    }
    let input = PublishInput {
        account: &job.account,
        title: job.title.as_deref(),
        text: &job.text,
        images: job.images.clone(),
        video: job.video.clone(),
        cover: job.cover.clone(),
        tags: &job.tags,
        topics: &job.topics,
        visibility: job.visibility.as_deref(),
        options: &job.options,
    };
    let args = platforms::publish_args(&platform, &input, &text_file);
    let env = state.catbus.run(args, None, TIMEOUT).await;
    let _ = tokio::fs::remove_file(&text_file).await;

    if env.ok {
        let s = |k: &str| env.data.get(k).and_then(|v| v.as_str()).map(String::from);
        return Outcome::Success { item_id: s("id"), item_url: s("url"), raw: serde_json::to_value(&env).unwrap_or_default() };
    }
    let code = env.error_code().unwrap_or("ERROR").to_string();
    Outcome::Failed {
        safe_to_fallback: SAFE_CODES.contains(&code.as_str()),
        retry_later: code == "NETWORK",
        message: env.error_text(),
        code,
    }
}
