//! dreammis/social-auto-upload CLI (`sau`). Browser automation (patchright),
//! covers platforms catbus doesn't, e.g. 视频号 (`tencent`). Login happens once
//! in a terminal: `sau <platform> login --account <name>`.

use super::{Job, Outcome};
use crate::{catbus::stderr_tail, state::AppState};
use serde_json::json;
use std::{process::Stdio, time::Duration};
use tokio::process::Command;

const TIMEOUT: Duration = Duration::from_secs(25 * 60);

/// Our platform id -> sau platform name.
pub fn sau_platform(platform: &str) -> Option<&'static str> {
    Some(match platform {
        "xhs" => "xiaohongshu",
        "douyin" => "douyin",
        "kuaishou" => "kuaishou",
        "bilibili" => "bilibili",
        "tencent" => "tencent",
        "weibo" => "weibo",
        _ => return None,
    })
}

const NOTE_PLATFORMS: &[&str] = &["douyin", "kuaishou", "xiaohongshu"];
const THUMBNAIL_PLATFORMS: &[&str] = &["bilibili", "weibo"];

/// Build sau argv. Values use `--flag=value` so text starting with `-` is never read as a flag.
pub fn args(job: &Job, sp: &str, sau_account: &str) -> Result<Vec<String>, String> {
    let title = job.title.clone().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| job.text.chars().take(20).collect());
    let mut tags: Vec<String> = job.tags.iter().chain(job.topics.iter()).map(|t| t.trim().replace(',', " ")).filter(|t| !t.is_empty()).collect();
    tags.dedup();
    let mut a = vec![sp.to_string()];
    if let Some(video) = &job.video {
        a.push("upload-video".into());
        a.push(format!("--account={sau_account}"));
        a.push(format!("--file={}", video.display()));
        a.push(format!("--title={title}"));
        a.push(format!("--desc={}", job.text));
        if sp == "bilibili" {
            let tid = job.options.get("category").map(|v| v.as_str().map(String::from).unwrap_or_else(|| v.to_string())).filter(|s| !s.trim().is_empty());
            let Some(tid) = tid else { return Err("B站通过 sau 投稿需要填写分区 ID".into()) };
            a.push(format!("--tid={}", tid.trim()));
        }
        if let (Some(c), true) = (&job.cover, THUMBNAIL_PLATFORMS.contains(&sp)) {
            a.push(format!("--thumbnail={}", c.display()));
        }
    } else if !job.images.is_empty() {
        if !NOTE_PLATFORMS.contains(&sp) {
            return Err(format!("sau 的 {sp} 只支持视频"));
        }
        a.push("upload-note".into());
        a.push(format!("--account={sau_account}"));
        a.push("--images".into());
        a.extend(job.images.iter().map(|p| p.display().to_string()));
        a.push(format!("--title={title}"));
        a.push(format!("--note={}", job.text));
    } else {
        return Err("sau 只能发布视频或图文".into());
    }
    if !tags.is_empty() {
        a.push(format!("--tags={}", tags.join(",")));
    }
    Ok(a)
}

pub async fn publish(state: &AppState, job: &Job) -> Outcome {
    let s = state.settings.get().await.sau;
    let Some(binding) = s.accounts.iter().find(|a| a.platform == job.platform && a.account == job.account) else {
        return Outcome::safe("UNSUPPORTED", "该账号没有绑定 sau 账号");
    };
    let Some(sp) = sau_platform(&job.platform) else {
        return Outcome::safe("UNSUPPORTED", "sau 不支持该平台");
    };
    let argv = match args(job, sp, &binding.sau_account) {
        Ok(a) => a,
        Err(e) => return Outcome::safe("USAGE", e),
    };
    let child = Command::new(&s.bin).args(&argv).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).spawn();
    let child = match child {
        Ok(c) => c,
        Err(e) => return Outcome::safe("SPAWN", format!("无法启动 sau（{}）：{e}", s.bin)),
    };
    // Once sau runs, the browser may submit at any point; failures are never safe to fall back from.
    match tokio::time::timeout(TIMEOUT, child.wait_with_output()).await {
        Err(_) => Outcome::unsafe_("TIMEOUT", "sau 超时，可能已提交，请先到平台创作中心确认"),
        Ok(Err(e)) => Outcome::unsafe_("UNKNOWN", format!("sau 执行异常：{e}")),
        Ok(Ok(out)) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let tail = format!("{}\n{}", stderr_tail(&out.stdout, 300), stderr_tail(&out.stderr, 300)).trim().to_string();
            if out.status.success() && !stdout.contains("submission_unknown") {
                let url = stdout.split_whitespace().rev().find(|w| w.starts_with("https://")).map(|u| u.trim_matches(|c| c == '"' || c == '\'' || c == ',').to_string());
                Outcome::Success { item_id: None, item_url: url, raw: json!({ "stdout": stderr_tail(&out.stdout, 2000) }) }
            } else {
                Outcome::unsafe_("UPSTREAM", format!("sau 退出码 {:?}：{tail}（可能已提交，请先确认）", out.status.code()))
            }
        }
    }
}

/// `sau <platform> check --account <name>` for the settings page.
pub async fn check(state: &AppState, platform: &str, sau_account: &str) -> serde_json::Value {
    let bin = state.settings.get().await.sau.bin;
    let Some(sp) = sau_platform(platform) else { return json!({ "ok": false, "message": "sau 不支持该平台" }) };
    let fut = Command::new(&bin).args([sp, "check", &format!("--account={sau_account}")]).stdin(Stdio::null()).kill_on_drop(true).output();
    match tokio::time::timeout(Duration::from_secs(120), fut).await {
        Err(_) => json!({ "ok": false, "message": "检查超时" }),
        Ok(Err(e)) => json!({ "ok": false, "message": format!("无法启动 sau（{bin}）：{e}") }),
        Ok(Ok(o)) => json!({ "ok": o.status.success(), "message": format!("{}\n{}", stderr_tail(&o.stdout, 300), stderr_tail(&o.stderr, 300)).trim() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn job() -> Job {
        Job {
            target_id: "t".into(),
            platform: "douyin".into(),
            account: "a".into(),
            title: Some("标题".into()),
            text: "-以减号开头的正文".into(),
            images: vec![PathBuf::from("/m/1.png"), PathBuf::from("/m/2.png")],
            video: None,
            cover: None,
            tags: vec!["露营".into()],
            topics: vec!["户外".into()],
            visibility: None,
            options: Default::default(),
        }
    }

    #[test]
    fn note_args() {
        let a = args(&job(), "douyin", "creator").unwrap();
        assert_eq!(a[..3], ["douyin", "upload-note", "--account=creator"]);
        assert!(a.contains(&"--note=-以减号开头的正文".to_string()));
        assert!(a.contains(&"--tags=露营,户外".to_string()));
    }

    #[test]
    fn bilibili_needs_tid() {
        let mut j = job();
        j.images.clear();
        j.video = Some(PathBuf::from("/m/v.mp4"));
        assert!(args(&j, "bilibili", "c").is_err());
        j.options.insert("category".into(), json!("249"));
        assert!(args(&j, "bilibili", "c").unwrap().contains(&"--tid=249".to_string()));
    }

    #[test]
    fn tencent_rejects_images() {
        assert!(args(&job(), "tencent", "c").is_err());
    }
}
