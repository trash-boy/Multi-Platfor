//! Official X API v2: OAuth 2.0 Authorization Code + PKCE, chunked media
//! upload, POST /2/tweets. Pay-per-use billing applies (see docs.x.com).

use super::{Job, Outcome};
use crate::{
    db,
    settings::{XApiAccount, XApiSettings},
    state::AppState,
};
use base64::Engine;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::Duration;

const AUTHORIZE: &str = "https://x.com/i/oauth2/authorize";
/// Overridable for tests: `MPF_X_API_BASE=http://127.0.0.1:9999`.
fn base() -> String {
    std::env::var("MPF_X_API_BASE").unwrap_or_else(|_| "https://api.x.com".into())
}
const SCOPES: &str = "tweet.read tweet.write users.read media.write offline.access";
const CHUNK: usize = 4 * 1024 * 1024;

/// In-flight authorization: state -> (code_verifier, account).
pub type Pending = std::sync::Arc<tokio::sync::Mutex<std::collections::HashMap<String, (String, String, std::time::Instant)>>>;

fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn pkce_challenge(verifier: &str) -> String {
    b64url(&Sha256::digest(verifier.as_bytes()))
}

pub fn redirect_uri(s: &XApiSettings, bind: &str) -> String {
    if s.redirect_uri.is_empty() { format!("http://{bind}/api/oauth/x/callback") } else { s.redirect_uri.clone() }
}

/// Build the authorize URL and remember the verifier.
pub async fn start(state: &AppState, account: &str) -> anyhow::Result<String> {
    let s = state.settings.get().await.x_api;
    anyhow::ensure!(!s.client_id.is_empty(), "请先在「通道设置」填写 X 应用的 Client ID");
    let verifier = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    let st = uuid::Uuid::new_v4().simple().to_string();
    {
        let mut p = state.x_pending.lock().await;
        p.retain(|_, v| v.2.elapsed() < Duration::from_secs(600));
        p.insert(st.clone(), (verifier.clone(), account.to_string(), std::time::Instant::now()));
    }
    let mut url = reqwest::Url::parse(AUTHORIZE)?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &s.client_id)
        .append_pair("redirect_uri", &redirect_uri(&s, &state.config.bind))
        .append_pair("scope", SCOPES)
        .append_pair("state", &st)
        .append_pair("code_challenge", &pkce_challenge(&verifier))
        .append_pair("code_challenge_method", "S256");
    Ok(url.to_string())
}

fn token_request(state: &AppState, s: &XApiSettings, mut form: Vec<(&str, String)>) -> reqwest::RequestBuilder {
    let r = state.http.post(format!("{}/2/oauth2/token", base())).timeout(Duration::from_secs(30));
    if s.client_secret.is_empty() {
        form.push(("client_id", s.client_id.clone()));
        r.form(&form)
    } else {
        r.basic_auth(&s.client_id, Some(&s.client_secret)).form(&form)
    }
}

async fn read_token(resp: reqwest::Response) -> anyhow::Result<(String, String, String)> {
    let code = resp.status();
    let v: Value = resp.json().await.unwrap_or(Value::Null);
    anyhow::ensure!(code.is_success(), "X 返回 {code}：{}", v);
    let access = v["access_token"].as_str().unwrap_or_default().to_string();
    anyhow::ensure!(!access.is_empty(), "X 没有返回 access_token");
    let refresh = v["refresh_token"].as_str().unwrap_or_default().to_string();
    let expires = db::ts(Utc::now() + chrono::Duration::seconds(v["expires_in"].as_i64().unwrap_or(7200)));
    Ok((access, refresh, expires))
}

/// OAuth callback: exchange the code, look up the user, store the account.
pub async fn finish(state: &AppState, st: &str, code: &str) -> anyhow::Result<XApiAccount> {
    let (verifier, account, _) = state.x_pending.lock().await.remove(st).ok_or_else(|| anyhow::anyhow!("授权已过期，请重新发起"))?;
    let s = state.settings.get().await.x_api;
    let form = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code.to_string()),
        ("redirect_uri", redirect_uri(&s, &state.config.bind)),
        ("code_verifier", verifier),
    ];
    let (access, refresh, expires) = read_token(token_request(state, &s, form).send().await?).await?;
    let me: Value = state.http.get(format!("{}/2/users/me", base())).bearer_auth(&access).timeout(Duration::from_secs(30)).send().await?.json().await?;
    let acc = XApiAccount {
        account: account.clone(),
        user_id: me["data"]["id"].as_str().unwrap_or_default().into(),
        username: me["data"]["username"].as_str().unwrap_or_default().into(),
        name: me["data"]["name"].as_str().unwrap_or_default().into(),
        access_token: access,
        refresh_token: refresh,
        expires_at: expires,
    };
    let saved = acc.clone();
    state
        .settings
        .update(move |s| {
            s.x_api.accounts.retain(|a| a.account != account);
            s.x_api.accounts.push(saved);
        })
        .await?;
    Ok(acc)
}

/// Valid access token, refreshing (and persisting the rotated refresh token) when close to expiry.
async fn access_token(state: &AppState, account: &str) -> anyhow::Result<String> {
    let s = state.settings.get().await.x_api;
    let acc = s.accounts.iter().find(|a| a.account == account).ok_or_else(|| anyhow::anyhow!("账号未授权 X 官方 API"))?;
    let fresh = DateTime::parse_from_rfc3339(&acc.expires_at).map(|t| t.with_timezone(&Utc) > Utc::now() + chrono::Duration::minutes(5)).unwrap_or(false);
    if fresh {
        return Ok(acc.access_token.clone());
    }
    anyhow::ensure!(!acc.refresh_token.is_empty(), "访问令牌已过期且没有 refresh_token，请重新授权");
    let form = vec![("grant_type", "refresh_token".to_string()), ("refresh_token", acc.refresh_token.clone())];
    let (access, refresh, expires) = read_token(token_request(state, &s, form).send().await?).await?;
    let (acct, a2) = (account.to_string(), access.clone());
    state
        .settings
        .update(move |s| {
            if let Some(a) = s.x_api.accounts.iter_mut().find(|a| a.account == acct) {
                a.access_token = a2;
                if !refresh.is_empty() {
                    a.refresh_token = refresh;
                }
                a.expires_at = expires;
            }
        })
        .await?;
    Ok(access)
}

fn mime_of(path: &std::path::Path) -> (&'static str, &'static str) {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("png") => ("image/png", "tweet_image"),
        Some("gif") => ("image/gif", "tweet_gif"),
        Some("webp") => ("image/webp", "tweet_image"),
        Some("mp4") | Some("m4v") => ("video/mp4", "tweet_video"),
        Some("mov") => ("video/quicktime", "tweet_video"),
        _ => ("image/jpeg", "tweet_image"),
    }
}

async fn api_json(resp: reqwest::Response) -> anyhow::Result<Value> {
    let code = resp.status();
    let v: Value = resp.json().await.unwrap_or(Value::Null);
    anyhow::ensure!(code.is_success(), "HTTP {code}：{}", v);
    Ok(v)
}

/// Chunked upload: initialize -> append* -> finalize (-> poll status for video).
async fn upload(state: &AppState, token: &str, path: &std::path::Path) -> anyhow::Result<String> {
    let bytes = tokio::fs::read(path).await?;
    let (mime, category) = mime_of(path);
    let init = api_json(
        state
            .http
            .post(format!("{}/2/media/upload/initialize", base()))
            .bearer_auth(token)
            .json(&json!({ "media_type": mime, "total_bytes": bytes.len(), "media_category": category }))
            .timeout(Duration::from_secs(60))
            .send()
            .await?,
    )
    .await?;
    let id = init["data"]["id"].as_str().ok_or_else(|| anyhow::anyhow!("initialize 没有返回 id"))?.to_string();
    for (i, chunk) in bytes.chunks(CHUNK).enumerate() {
        let form = reqwest::multipart::Form::new()
            .text("segment_index", i.to_string())
            .part("media", reqwest::multipart::Part::bytes(chunk.to_vec()).file_name("chunk"));
        api_json(state.http.post(format!("{}/2/media/upload/{id}/append", base())).bearer_auth(token).multipart(form).timeout(Duration::from_secs(300)).send().await?).await?;
    }
    let mut v = api_json(state.http.post(format!("{}/2/media/upload/{id}/finalize", base())).bearer_auth(token).timeout(Duration::from_secs(120)).send().await?).await?;
    for _ in 0..120 {
        let info = &v["data"]["processing_info"];
        match info["state"].as_str() {
            None | Some("succeeded") => return Ok(id),
            Some("failed") => anyhow::bail!("媒体处理失败：{}", info["error"]),
            _ => {}
        }
        tokio::time::sleep(Duration::from_secs(info["check_after_secs"].as_u64().unwrap_or(3).clamp(1, 30))).await;
        v = api_json(state.http.get(format!("{}/2/media/upload", base())).query(&[("command", "STATUS"), ("media_id", id.as_str())]).bearer_auth(token).timeout(Duration::from_secs(60)).send().await?).await?;
    }
    anyhow::bail!("媒体处理超时")
}

fn tweet_id(s: &str) -> String {
    let s = s.trim().trim_end_matches('/');
    s.rsplit('/').next().unwrap_or(s).split('?').next().unwrap_or(s).to_string()
}

pub async fn publish(state: &AppState, job: &Job) -> Outcome {
    let token = match access_token(state, &job.account).await {
        Ok(t) => t,
        Err(e) => return Outcome::safe("AUTH_REQUIRED", format!("{e:#}")),
    };
    // Media uploads don't create a post; any failure here is safe to fall back from.
    let mut media_ids = Vec::new();
    for p in job.video.iter().chain(job.images.iter()) {
        match upload(state, &token, p).await {
            Ok(id) => media_ids.push(id),
            Err(e) => return Outcome::safe("MEDIA_UPLOAD", format!("上传媒体失败：{e:#}")),
        }
    }
    let mut body = json!({ "text": job.text });
    if !media_ids.is_empty() {
        body["media"] = json!({ "media_ids": media_ids });
    }
    if let Some(q) = job.options.get("quote").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()) {
        body["quote_tweet_id"] = json!(tweet_id(q));
    }
    let resp = state.http.post(format!("{}/2/tweets", base())).bearer_auth(&token).json(&body).timeout(Duration::from_secs(60)).send().await;
    match resp {
        Err(e) if e.is_connect() => Outcome::safe("NETWORK", format!("连接 X 失败：{e}")),
        Err(e) => Outcome::unsafe_("UNKNOWN", format!("请求中断，可能已发布，请先到 X 确认：{e}")),
        Ok(r) => {
            let code = r.status();
            let v: Value = r.json().await.unwrap_or(Value::Null);
            if code.is_success() {
                let id = v["data"]["id"].as_str().map(String::from);
                let url = id.as_ref().map(|i| format!("https://x.com/i/web/status/{i}"));
                Outcome::Success { item_id: id, item_url: url, raw: v }
            } else if code.is_client_error() {
                // A 4xx is a definite rejection: nothing was posted.
                let detail = v["detail"].as_str().or(v["title"].as_str()).unwrap_or("").to_string();
                let c = match code.as_u16() {
                    401 => "AUTH_REQUIRED",
                    402 => "PAYMENT_REQUIRED",
                    429 => "RATE_LIMITED",
                    _ => "REJECTED",
                };
                Outcome::safe(c, format!("X API {code}：{detail}"))
            } else {
                Outcome::unsafe_("UPSTREAM", format!("X API {code}（可能已发布，请先确认）：{v}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_rfc7636_example() {
        assert_eq!(pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn extracts_tweet_id() {
        assert_eq!(tweet_id("https://x.com/user/status/123456?s=20"), "123456");
        assert_eq!(tweet_id("789"), "789");
    }
}
