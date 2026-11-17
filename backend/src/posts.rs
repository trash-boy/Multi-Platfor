//! Post model and persistence. A post is written once; each (platform, account)
//! it goes to is a `target` with its own status, schedule and result.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

#[derive(Serialize, Clone)]
pub struct Media {
    pub id: String,
    pub filename: String,
    pub stored: String,
    pub mime: String,
    pub kind: String,
    pub size: i64,
    pub url: String,
}

impl Media {
    pub fn from_row(r: &SqliteRow) -> Self {
        let stored: String = r.get("stored");
        Self {
            id: r.get("id"),
            filename: r.get("filename"),
            url: format!("/media/{stored}"),
            stored,
            mime: r.get("mime"),
            kind: r.get("kind"),
            size: r.get("size"),
        }
    }
}

#[derive(Serialize, Clone)]
pub struct Target {
    pub id: String,
    pub post_id: String,
    pub platform: String,
    pub account: String,
    pub title: Option<String>,
    pub text: Option<String>,
    pub visibility: Option<String>,
    pub options: Map<String, Value>,
    /// pending | running | success | failed | canceled
    pub status: String,
    pub run_at: String,
    pub attempts: i64,
    pub error_code: Option<String>,
    pub last_error: Option<String>,
    pub item_id: Option<String>,
    pub item_url: Option<String>,
    pub updated_at: String,
    /// Provider that produced the final result.
    pub provider: Option<String>,
    /// When set, only this provider is tried (manual "retry via ...").
    pub force_provider: Option<String>,
    pub attempt_log: Vec<Value>,
}

impl Target {
    pub fn from_row(r: &SqliteRow) -> Self {
        Self {
            id: r.get("id"),
            post_id: r.get("post_id"),
            platform: r.get("platform"),
            account: r.get("account"),
            title: r.get("title"),
            text: r.get("text"),
            visibility: r.get("visibility"),
            options: serde_json::from_str(r.get::<&str, _>("options")).unwrap_or_default(),
            status: r.get("status"),
            run_at: r.get("run_at"),
            attempts: r.get("attempts"),
            error_code: r.get("error_code"),
            last_error: r.get("last_error"),
            item_id: r.get("item_id"),
            item_url: r.get("item_url"),
            updated_at: r.get("updated_at"),
            provider: r.get("provider"),
            force_provider: r.get("force_provider"),
            attempt_log: serde_json::from_str(r.get::<&str, _>("attempt_log")).unwrap_or_default(),
        }
    }
}

#[derive(Serialize, Clone)]
pub struct Post {
    pub id: String,
    pub title: Option<String>,
    pub text: String,
    pub images: Vec<Media>,
    pub video: Option<Media>,
    pub cover: Option<Media>,
    pub tags: Vec<String>,
    pub topics: Vec<String>,
    pub scheduled_at: Option<String>,
    pub created_at: String,
    pub targets: Vec<Target>,
}

#[derive(Deserialize)]
pub struct NewTarget {
    pub platform: String,
    pub account: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub options: Map<String, Value>,
}

#[derive(Deserialize)]
pub struct NewPost {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub images: Vec<String>,
    #[serde(default)]
    pub video: Option<String>,
    #[serde(default)]
    pub cover: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub scheduled_at: Option<String>,
    pub targets: Vec<NewTarget>,
}

pub async fn media_by_ids(db: &SqlitePool, ids: &[String]) -> anyhow::Result<Vec<Media>> {
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(r) = sqlx::query("SELECT * FROM media WHERE id = ?").bind(id).fetch_optional(db).await? {
            out.push(Media::from_row(&r));
        }
    }
    Ok(out)
}

async fn one_media(db: &SqlitePool, id: Option<String>) -> anyhow::Result<Option<Media>> {
    Ok(match id {
        Some(id) => media_by_ids(db, &[id]).await?.into_iter().next(),
        None => None,
    })
}

pub async fn load_post(db: &SqlitePool, id: &str) -> anyhow::Result<Option<Post>> {
    let Some(r) = sqlx::query("SELECT * FROM posts WHERE id = ?").bind(id).fetch_optional(db).await? else {
        return Ok(None);
    };
    post_from_row(db, &r).await.map(Some)
}

async fn post_from_row(db: &SqlitePool, r: &SqliteRow) -> anyhow::Result<Post> {
    let id: String = r.get("id");
    let image_ids: Vec<String> = serde_json::from_str(r.get::<&str, _>("images")).unwrap_or_default();
    let targets = sqlx::query("SELECT * FROM post_targets WHERE post_id = ? ORDER BY platform, account")
        .bind(&id)
        .fetch_all(db)
        .await?
        .iter()
        .map(Target::from_row)
        .collect();
    Ok(Post {
        title: r.get("title"),
        text: r.get("text"),
        images: media_by_ids(db, &image_ids).await?,
        video: one_media(db, r.get("video")).await?,
        cover: one_media(db, r.get("cover")).await?,
        tags: serde_json::from_str(r.get::<&str, _>("tags")).unwrap_or_default(),
        topics: serde_json::from_str(r.get::<&str, _>("topics")).unwrap_or_default(),
        scheduled_at: r.get("scheduled_at"),
        created_at: r.get("created_at"),
        targets,
        id,
    })
}

/// Posts whose effective time (schedule, else creation) falls in [from, to).
pub async fn list_posts(db: &SqlitePool, from: Option<&str>, to: Option<&str>, limit: i64) -> anyhow::Result<Vec<Post>> {
    let rows = sqlx::query(
        "SELECT * FROM posts
         WHERE (?1 IS NULL OR COALESCE(scheduled_at, created_at) >= ?1)
           AND (?2 IS NULL OR COALESCE(scheduled_at, created_at) < ?2)
         ORDER BY COALESCE(scheduled_at, created_at) DESC LIMIT ?3",
    )
    .bind(from)
    .bind(to)
    .bind(limit)
    .fetch_all(db)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in &rows {
        out.push(post_from_row(db, r).await?);
    }
    Ok(out)
}

pub async fn load_target(db: &SqlitePool, id: &str) -> anyhow::Result<Option<Target>> {
    Ok(sqlx::query("SELECT * FROM post_targets WHERE id = ?").bind(id).fetch_optional(db).await?.as_ref().map(Target::from_row))
}
