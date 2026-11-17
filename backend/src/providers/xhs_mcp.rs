//! xpzouying/xiaohongshu-mcp HTTP API (default :18060). It drives a real
//! headless browser, so it keeps working when web signatures change.

use super::{Job, Outcome};
use crate::{settings::XhsMcpInstance, state::AppState};
use serde_json::{json, Value};
use std::time::Duration;

fn req(state: &AppState, inst: &XhsMcpInstance, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
    let url = format!("{}{}", inst.url.trim_end_matches('/'), path);
    let mut r = state.http.request(method, url);
    if !inst.token.is_empty() {
        r = r.bearer_auth(&inst.token);
    }
    r
}

pub async fn instance(state: &AppState, account: &str) -> Option<XhsMcpInstance> {
    state.settings.get().await.xhs_mcp.instances.into_iter().find(|i| i.account == account)
}

/// `{"reachable", "logged_in", "username", "error"}`
pub async fn status(state: &AppState, inst: &XhsMcpInstance) -> Value {
    let r = req(state, inst, reqwest::Method::GET, "/api/v1/login/status").timeout(Duration::from_secs(20)).send().await;
    match r {
        Err(e) => json!({ "reachable": false, "logged_in": false, "username": null, "error": format!("连接失败：{e}") }),
        Ok(resp) => {
            let code = resp.status();
            let body: Value = resp.json().await.unwrap_or(Value::Null);
            if !code.is_success() {
                return json!({ "reachable": true, "logged_in": false, "username": null, "error": format!("HTTP {code}：{}", body["error"].as_str().unwrap_or("")) });
            }
            json!({
                "reachable": true,
                "logged_in": body["data"]["is_logged_in"].as_bool().unwrap_or(false),
                "username": body["data"]["username"],
                "error": null,
            })
        }
    }
}

/// Returns the data-URL QR image, or `already_logged_in`.
pub async fn qrcode(state: &AppState, inst: &XhsMcpInstance) -> anyhow::Result<Value> {
    let body: Value = req(state, inst, reqwest::Method::GET, "/api/v1/login/qrcode")
        .timeout(Duration::from_secs(60))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(json!({ "img": body["data"]["img"], "logged_in": body["data"]["is_logged_in"], "timeout": body["data"]["timeout"] }))
}

fn visibility(v: Option<&str>) -> &'static str {
    match v {
        Some("private") => "仅自己可见",
        Some("friends") => "仅互关好友可见",
        _ => "公开可见",
    }
}

pub async fn publish(state: &AppState, job: &Job) -> Outcome {
    let Some(inst) = instance(state, &job.account).await else {
        return Outcome::safe("UNSUPPORTED", "该账号没有绑定 xiaohongshu-mcp 实例");
    };
    let title = job.title.as_deref().map(str::trim).unwrap_or("");
    if title.is_empty() {
        return Outcome::safe("USAGE", "xiaohongshu-mcp 要求填写标题");
    }
    // Pre-flight: unreachable or logged out means nothing was sent.
    let st = status(state, &inst).await;
    if !st["reachable"].as_bool().unwrap_or(false) {
        return Outcome::safe("UNAVAILABLE", st["error"].as_str().unwrap_or("服务不可用").to_string());
    }
    if !st["logged_in"].as_bool().unwrap_or(false) {
        return Outcome::safe("AUTH_REQUIRED", "xiaohongshu-mcp 未登录，请在「通道设置」扫码");
    }

    let mut tags = job.tags.clone();
    tags.extend(job.topics.iter().cloned());
    let path_str = |p: &std::path::PathBuf| p.display().to_string();
    let (path, body) = if let Some(video) = &job.video {
        ("/api/v1/publish_video", json!({ "title": title, "content": job.text, "video": path_str(video), "tags": tags, "visibility": visibility(job.visibility.as_deref()) }))
    } else if !job.images.is_empty() {
        ("/api/v1/publish", json!({ "title": title, "content": job.text, "images": job.images.iter().map(path_str).collect::<Vec<_>>(), "tags": tags, "visibility": visibility(job.visibility.as_deref()) }))
    } else {
        return Outcome::safe("USAGE", "小红书必须带图片或视频");
    };

    // From here on the browser may already have clicked "publish"; never fall back.
    let resp = req(state, &inst, reqwest::Method::POST, path).json(&body).timeout(Duration::from_secs(20 * 60)).send().await;
    match resp {
        Err(e) if e.is_connect() => Outcome::safe("UNAVAILABLE", format!("连接失败：{e}")),
        Err(e) => Outcome::unsafe_("UNKNOWN", format!("请求中断，可能已发布，请先到小红书确认：{e}")),
        Ok(r) => {
            let code = r.status();
            let v: Value = r.json().await.unwrap_or(Value::Null);
            if code.is_success() && v["success"].as_bool().unwrap_or(false) {
                Outcome::Success { item_id: None, item_url: None, raw: v }
            } else if code == reqwest::StatusCode::UNAUTHORIZED {
                Outcome::safe("AUTH_REQUIRED", "xiaohongshu-mcp 访问令牌错误")
            } else {
                let msg = v["error"].as_str().or(v["message"].as_str()).unwrap_or("未知错误");
                let detail = v["details"].as_str().unwrap_or("");
                Outcome::unsafe_("UPSTREAM", format!("HTTP {code}：{msg} {detail}（可能已部分完成，请先到小红书确认）"))
            }
        }
    }
}

// ---------- read side: normalize feeds into catbus' Item shape ----------

fn count(v: &Value) -> Value {
    let s = match v {
        Value::String(s) => s.trim().trim_end_matches('+').to_string(),
        Value::Number(n) => return Value::Number(n.clone()),
        _ => return Value::Null,
    };
    let (num, mul) = if let Some(x) = s.strip_suffix('万') { (x, 10_000.0) } else if let Some(x) = s.strip_suffix('亿') { (x, 1e8) } else { (s.as_str(), 1.0) };
    num.parse::<f64>().map(|n| json!((n * mul).round() as i64)).unwrap_or(Value::Null)
}

pub fn feed_to_item(f: &Value) -> Value {
    let id = f["id"].as_str().unwrap_or_default();
    let tok = f["xsecToken"].as_str().unwrap_or_default();
    let card = &f["noteCard"];
    let user = &card["user"];
    let cover = card["cover"]["urlDefault"].as_str().filter(|s| !s.is_empty()).or(card["cover"]["url"].as_str());
    let ii = &card["interactInfo"];
    json!({
        "id": id,
        "kind": if card["video"].is_null() { "image" } else { "video" },
        "url": format!("https://www.xiaohongshu.com/explore/{id}?xsec_token={tok}&xsec_source=pc_search"),
        "title": card["displayTitle"],
        "text": null,
        "author": { "id": user["userId"], "name": user["nickname"].as_str().or(user["nickName"].as_str()), "url": null, "avatar": user["avatar"] },
        "created_at": null,
        "cover": cover,
        "media": [],
        "stats": { "views": null, "likes": count(&ii["likedCount"]), "comments": count(&ii["commentCount"]), "collects": count(&ii["collectedCount"]), "shares": count(&ii["sharedCount"]) },
        "price": null,
        "status": null,
        "source": "xhs-mcp",
    })
}

/// Search or list via xiaohongshu-mcp; returns a catbus-style envelope.
pub async fn read(state: &AppState, inst: &XhsMcpInstance, keyword: Option<&str>) -> crate::catbus::Envelope {
    let r = match keyword {
        Some(k) => req(state, inst, reqwest::Method::POST, "/api/v1/feeds/search").json(&json!({ "keyword": k })),
        None => req(state, inst, reqwest::Method::GET, "/api/v1/feeds/list"),
    };
    let res = async {
        let v: Value = r.timeout(Duration::from_secs(120)).send().await?.error_for_status()?.json().await?;
        anyhow::Ok(v)
    }
    .await;
    match res {
        Ok(v) => crate::catbus::Envelope {
            ok: true,
            platform: Some("xhs".into()),
            account: Some(inst.account.clone()),
            data: Value::Array(v["data"]["feeds"].as_array().map(|a| a.iter().map(feed_to_item).collect()).unwrap_or_default()),
            page: None,
            error: None,
        },
        Err(e) => crate::catbus::Envelope::failure("UPSTREAM", format!("xiaohongshu-mcp：{e:#}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_counts() {
        assert_eq!(count(&json!("1.7万")), json!(17000));
        assert_eq!(count(&json!("10万+")), json!(100000));
        assert_eq!(count(&json!("35")), json!(35));
        assert_eq!(count(&json!("")), Value::Null);
    }

    #[test]
    fn normalizes_feed() {
        let f = json!({"id":"abc","xsecToken":"T","noteCard":{"displayTitle":"标题","user":{"userId":"u","nickname":"昵称","avatar":"a"},"interactInfo":{"likedCount":"2万"},"cover":{"url":"c"},"video":null}});
        let it = feed_to_item(&f);
        assert_eq!(it["url"], "https://www.xiaohongshu.com/explore/abc?xsec_token=T&xsec_source=pc_search");
        assert_eq!(it["stats"]["likes"], 20000);
        assert_eq!(it["author"]["name"], "昵称");
        assert_eq!(it["cover"], "c");
    }
}
