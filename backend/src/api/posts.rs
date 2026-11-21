use crate::{
    db,
    platforms::{self, PostShape},
    posts::{self, NewPost, Post},
    state::{ApiError, ApiResult, AppState},
};
use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

const MEDIA_LIMIT: usize = 1024 * 1024 * 1024;

pub async fn upload_media(State(s): State<AppState>, mut mp: Multipart) -> ApiResult<Json<Value>> {
    let mut saved = Vec::new();
    while let Some(mut field) = mp.next_field().await.map_err(|e| ApiError::bad_request(e.to_string()))? {
        let filename = field.file_name().unwrap_or("upload").to_string();
        let mime = field.content_type().unwrap_or("application/octet-stream").to_string();
        let kind = if mime.starts_with("image/") {
            "image"
        } else if mime.starts_with("video/") {
            "video"
        } else {
            return Err(ApiError::bad_request(format!("{filename}：只支持图片和视频")));
        };
        let ext: String = std::path::Path::new(&filename)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .filter(|e| e.len() <= 5 && e.chars().all(|c| c.is_ascii_alphanumeric()))
            .unwrap_or_else(|| mime.rsplit('/').next().unwrap_or("bin").to_string());
        let id = uuid::Uuid::new_v4().to_string();
        let stored = format!("{id}.{ext}");
        let path = s.config.media_dir().join(&stored);
        let size = match save_field(&mut field, &path).await {
            Ok(n) => n,
            Err(e) => {
                let _ = tokio::fs::remove_file(&path).await;
                return Err(e);
            }
        };
        sqlx::query("INSERT INTO media (id, filename, stored, mime, kind, size, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(&id)
            .bind(&filename)
            .bind(&stored)
            .bind(&mime)
            .bind(kind)
            .bind(size as i64)
            .bind(db::now())
            .execute(&s.db)
            .await?;
        saved.push(json!({ "id": id, "filename": filename, "kind": kind, "mime": mime, "size": size, "url": format!("/media/{stored}") }));
    }
    Ok(Json(json!(saved)))
}

/// Stream one multipart field to `path` chunk by chunk, so a large video never
/// sits in memory. Returns the byte count; the caller removes `path` on error.
async fn save_field(field: &mut axum::extract::multipart::Field<'_>, path: &std::path::Path) -> ApiResult<usize> {
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::File::create(path).await?;
    let mut size = 0usize;
    while let Some(chunk) = field.chunk().await.map_err(|e| ApiError::bad_request(format!("上传中断：{e}")))? {
        size += chunk.len();
        if size > MEDIA_LIMIT {
            return Err(ApiError::bad_request("文件超过 1GB"));
        }
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    Ok(size)
}

pub async fn create(State(s): State<AppState>, Json(b): Json<NewPost>) -> ApiResult<(StatusCode, Json<Post>)> {
    if b.targets.is_empty() {
        return Err(ApiError::bad_request("至少选择一个发布平台"));
    }
    let images = posts::media_by_ids(&s.db, &b.images).await?;
    if images.len() != b.images.len() || images.iter().any(|m| m.kind != "image") {
        return Err(ApiError::bad_request("图片无效，请重新上传"));
    }
    let video = posts::media_by_ids(&s.db, b.video.as_slice()).await?;
    if b.video.is_some() && video.first().is_none_or(|m| m.kind != "video") {
        return Err(ApiError::bad_request("视频无效，请重新上传"));
    }
    let cover = posts::media_by_ids(&s.db, b.cover.as_slice()).await?;
    if b.cover.is_some() && cover.first().is_none_or(|m| m.kind != "image") {
        return Err(ApiError::bad_request("封面无效，请重新上传"));
    }

    let scheduled_at = match b.scheduled_at.as_deref().filter(|x| !x.is_empty()) {
        Some(x) => Some(db::parse_ts(x).ok_or_else(|| ApiError::bad_request("定时时间格式错误"))?),
        None => None,
    };

    let mut details = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for t in &b.targets {
        let label = format!("{}/{}", t.platform, t.account);
        if !seen.insert(label.clone()) {
            details.push(format!("{label}：重复"));
            continue;
        }
        let Some(p) = platforms::get(&t.platform) else {
            details.push(format!("{label}：未知平台"));
            continue;
        };
        let shape = PostShape {
            title: t.title.as_deref().or(b.title.as_deref()),
            text: t.text.as_deref().unwrap_or(&b.text),
            image_count: images.len(),
            has_video: b.video.is_some(),
            has_cover: b.cover.is_some(),
            visibility: t.visibility.as_deref(),
            options: &t.options,
        };
        details.extend(platforms::validate(&p, &shape).into_iter().map(|e| format!("{}：{e}", p.name)));
    }
    if !details.is_empty() {
        let mut err = ApiError::bad_request("内容不符合平台要求");
        err.details = details;
        return Err(err);
    }

    let now = db::now();
    let post_id = uuid::Uuid::new_v4().to_string();
    let mut tx = s.db.begin().await?;
    let first_comment = b.first_comment.as_deref().map(str::trim).filter(|t| !t.is_empty());
    if first_comment.is_some_and(|t| t.chars().count() > 500) {
        return Err(ApiError::bad_request("首评不能超过 500 字"));
    }
    sqlx::query("INSERT INTO posts (id, title, text, images, video, cover, tags, topics, scheduled_at, created_at, first_comment, first_comment_delay) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(&post_id)
        .bind(b.title.as_deref().map(str::trim).filter(|t| !t.is_empty()))
        .bind(&b.text)
        .bind(serde_json::to_string(&b.images)?)
        .bind(&b.video)
        .bind(&b.cover)
        .bind(serde_json::to_string(&b.tags)?)
        .bind(serde_json::to_string(&b.topics)?)
        .bind(&scheduled_at)
        .bind(&now)
        .bind(first_comment)
        .bind(first_comment.map(|_| b.first_comment_delay.unwrap_or(10).clamp(1, 24 * 60)))
        .execute(&mut *tx)
        .await?;
    for t in &b.targets {
        sqlx::query(
            "INSERT INTO post_targets (id, post_id, platform, account, title, text, visibility, options, status, run_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&post_id)
        .bind(&t.platform)
        .bind(&t.account)
        .bind(&t.title)
        .bind(&t.text)
        .bind(&t.visibility)
        .bind(serde_json::to_string(&t.options)?)
        .bind(scheduled_at.as_deref().unwrap_or(&now))
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    let post = posts::load_post(&s.db, &post_id).await?.ok_or_else(ApiError::not_found)?;
    Ok((StatusCode::CREATED, Json(post)))
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<i64>,
}

pub async fn list(State(s): State<AppState>, Query(q): Query<ListQuery>) -> ApiResult<Json<Vec<Post>>> {
    let from = q.from.as_deref().and_then(db::parse_ts);
    let to = q.to.as_deref().and_then(db::parse_ts);
    Ok(Json(posts::list_posts(&s.db, from.as_deref(), to.as_deref(), q.limit.unwrap_or(100).clamp(1, 500)).await?))
}

pub async fn get(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Post>> {
    posts::load_post(&s.db, &id).await?.map(Json).ok_or_else(ApiError::not_found)
}

async fn has_running(s: &AppState, post_id: &str) -> ApiResult<bool> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM post_targets WHERE post_id = ? AND status = 'running'")
        .bind(post_id)
        .fetch_one(&s.db)
        .await?;
    Ok(n > 0)
}

/// Cancel every target that hasn't started yet.
pub async fn cancel(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Post>> {
    sqlx::query("UPDATE post_targets SET status = 'canceled', updated_at = ? WHERE post_id = ? AND status = 'pending'")
        .bind(db::now())
        .bind(&id)
        .execute(&s.db)
        .await?;
    get(State(s), Path(id)).await
}

pub async fn delete(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    if has_running(&s, &id).await? {
        return Err(ApiError::new(StatusCode::CONFLICT, "有任务正在发布，稍后再删"));
    }
    let mut tx = s.db.begin().await?;
    sqlx::query("DELETE FROM post_targets WHERE post_id = ?").bind(&id).execute(&mut *tx).await?;
    let n = sqlx::query("DELETE FROM posts WHERE id = ?").bind(&id).execute(&mut *tx).await?.rows_affected();
    tx.commit().await?;
    if n == 0 { Err(ApiError::not_found()) } else { Ok(StatusCode::NO_CONTENT) }
}

#[derive(Deserialize)]
pub struct Reschedule {
    pub scheduled_at: Option<String>,
}

/// Move a post on the calendar. Only targets that haven't started move with it.
pub async fn reschedule(State(s): State<AppState>, Path(id): Path<String>, Json(b): Json<Reschedule>) -> ApiResult<Json<Post>> {
    let at = match b.scheduled_at.as_deref().filter(|x| !x.is_empty()) {
        Some(x) => Some(db::parse_ts(x).ok_or_else(|| ApiError::bad_request("时间格式错误"))?),
        None => None,
    };
    let now = db::now();
    let n = sqlx::query("UPDATE posts SET scheduled_at = ? WHERE id = ?").bind(&at).bind(&id).execute(&s.db).await?.rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    sqlx::query("UPDATE post_targets SET run_at = ?, updated_at = ? WHERE post_id = ? AND status = 'pending'")
        .bind(at.as_deref().unwrap_or(&now))
        .bind(&now)
        .bind(&id)
        .execute(&s.db)
        .await?;
    get(State(s), Path(id)).await
}

#[derive(Deserialize)]
pub struct RetryQuery {
    /// Only try this provider (e.g. after confirming the first one didn't post).
    pub provider: Option<String>,
}

/// Manually re-queue a failed or canceled target.
pub async fn retry_target(State(s): State<AppState>, Path(id): Path<String>, Query(q): Query<RetryQuery>) -> ApiResult<Json<Value>> {
    let provider = q.provider.filter(|p| !p.is_empty());
    if let Some(p) = &provider {
        let t = posts::load_target(&s.db, &id).await?.ok_or_else(ApiError::not_found)?;
        let plat = platforms::get(&t.platform).ok_or_else(|| ApiError::bad_request("未知平台"))?;
        if !plat.providers.contains(&p.as_str()) || !crate::providers::bound(&s.settings.get().await, p, &t.platform, &t.account) {
            return Err(ApiError::bad_request(format!("这个账号没有配置通道 {p}")));
        }
    }
    let n = sqlx::query(
        "UPDATE post_targets SET status = 'pending', run_at = ?, error_code = NULL, last_error = NULL, force_provider = ?, updated_at = ?
         WHERE id = ? AND status IN ('failed', 'canceled')",
    )
    .bind(db::now())
    .bind(&provider)
    .bind(db::now())
    .bind(&id)
    .execute(&s.db)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(ApiError::new(StatusCode::CONFLICT, "只有失败或已取消的任务可以重试"));
    }
    Ok(Json(json!(posts::load_target(&s.db, &id).await?)))
}
