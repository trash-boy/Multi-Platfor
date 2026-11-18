use chrono::{DateTime, SecondsFormat, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::{path::Path, str::FromStr};

const SCHEMA: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS media (
        id TEXT PRIMARY KEY,
        filename TEXT NOT NULL,
        stored TEXT NOT NULL,
        mime TEXT NOT NULL,
        kind TEXT NOT NULL,
        size INTEGER NOT NULL,
        created_at TEXT NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS posts (
        id TEXT PRIMARY KEY,
        title TEXT,
        text TEXT NOT NULL,
        images TEXT NOT NULL,
        video TEXT,
        cover TEXT,
        tags TEXT NOT NULL,
        topics TEXT NOT NULL,
        scheduled_at TEXT,
        created_at TEXT NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS post_targets (
        id TEXT PRIMARY KEY,
        post_id TEXT NOT NULL,
        platform TEXT NOT NULL,
        account TEXT NOT NULL,
        title TEXT,
        text TEXT,
        visibility TEXT,
        options TEXT NOT NULL,
        status TEXT NOT NULL,
        run_at TEXT NOT NULL,
        attempts INTEGER NOT NULL DEFAULT 0,
        error_code TEXT,
        last_error TEXT,
        item_id TEXT,
        item_url TEXT,
        result TEXT,
        updated_at TEXT NOT NULL
    )",
    "CREATE INDEX IF NOT EXISTS idx_targets_due ON post_targets(status, run_at)",
    "CREATE INDEX IF NOT EXISTS idx_targets_post ON post_targets(post_id)",
    // ---- engagement: own works + stats snapshots (see engage/sync.rs) ----
    "CREATE TABLE IF NOT EXISTS items (
        platform TEXT NOT NULL,
        account TEXT NOT NULL,
        item_id TEXT NOT NULL,
        kind TEXT,
        title TEXT,
        url TEXT,
        cover TEXT,
        published_at TEXT,
        target_id TEXT,
        views INTEGER, likes INTEGER, comments INTEGER, collects INTEGER, shares INTEGER,
        comments_seen INTEGER,
        first_seen TEXT NOT NULL,
        synced_at TEXT NOT NULL,
        PRIMARY KEY (platform, account, item_id)
    )",
    "CREATE TABLE IF NOT EXISTS item_stats (
        platform TEXT NOT NULL,
        account TEXT NOT NULL,
        item_id TEXT NOT NULL,
        at TEXT NOT NULL,
        views INTEGER, likes INTEGER, comments INTEGER, collects INTEGER, shares INTEGER
    )",
    "CREATE INDEX IF NOT EXISTS idx_item_stats ON item_stats(at, platform, account, item_id)",
    // ---- engagement: unified inbox (comments, mentions, DM conversations) ----
    "CREATE TABLE IF NOT EXISTS inbox (
        id TEXT PRIMARY KEY,
        platform TEXT NOT NULL,
        account TEXT NOT NULL,
        kind TEXT NOT NULL,
        external_id TEXT NOT NULL,
        item_id TEXT,
        item_url TEXT,
        item_title TEXT,
        parent_id TEXT,
        author_id TEXT,
        author_name TEXT,
        author_url TEXT,
        author_avatar TEXT,
        text TEXT,
        created_at TEXT,
        fetched_at TEXT NOT NULL,
        is_read INTEGER NOT NULL DEFAULT 0,
        replied_at TEXT,
        reply_text TEXT
    )",
    "CREATE INDEX IF NOT EXISTS idx_inbox_time ON inbox(COALESCE(created_at, fetched_at))",
    // ---- engagement: automation rules and their action queue (engage/auto.rs) ----
    "CREATE TABLE IF NOT EXISTS auto_rules (
        id TEXT PRIMARY KEY,
        kind TEXT NOT NULL,
        enabled INTEGER NOT NULL DEFAULT 1,
        name TEXT NOT NULL,
        platform TEXT,
        account TEXT,
        keywords TEXT NOT NULL DEFAULT '[]',
        metric TEXT,
        threshold INTEGER,
        text TEXT NOT NULL,
        created_at TEXT NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS auto_actions (
        id TEXT PRIMARY KEY,
        source TEXT NOT NULL,
        rule_id TEXT,
        target_id TEXT,
        platform TEXT NOT NULL,
        account TEXT NOT NULL,
        item_id TEXT NOT NULL,
        reply_to TEXT,
        dedup TEXT NOT NULL UNIQUE,
        text TEXT NOT NULL,
        status TEXT NOT NULL,
        run_at TEXT NOT NULL,
        error_code TEXT,
        error TEXT,
        comment_id TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )",
    "CREATE INDEX IF NOT EXISTS idx_auto_due ON auto_actions(status, run_at)",
];

pub async fn connect(data_dir: &Path) -> anyhow::Result<SqlitePool> {
    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", data_dir.join("mpf.db").display()))?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(10));
    let pool = SqlitePoolOptions::new().max_connections(5).connect_with(opts).await?;
    for stmt in SCHEMA {
        sqlx::query(*stmt).execute(&pool).await?;
    }
    migrate(&pool).await?;
    Ok(pool)
}

/// Columns added after the first release; SQLite has no ADD COLUMN IF NOT EXISTS.
async fn migrate(pool: &SqlitePool) -> anyhow::Result<()> {
    use sqlx::Row;
    let cols: Vec<String> = sqlx::query("PRAGMA table_info(post_targets)").fetch_all(pool).await?.iter().map(|r| r.get::<String, _>("name")).collect();
    let add: &[(&str, &str)] = &[
        ("provider", "ALTER TABLE post_targets ADD COLUMN provider TEXT"),
        ("force_provider", "ALTER TABLE post_targets ADD COLUMN force_provider TEXT"),
        ("attempt_log", "ALTER TABLE post_targets ADD COLUMN attempt_log TEXT NOT NULL DEFAULT '[]'"),
    ];
    for (col, stmt) in add {
        if !cols.iter().any(|c| c == col) {
            sqlx::query(*stmt).execute(pool).await?;
        }
    }
    let cols: Vec<String> = sqlx::query("PRAGMA table_info(posts)").fetch_all(pool).await?.iter().map(|r| r.get::<String, _>("name")).collect();
    let add: &[(&str, &str)] = &[
        ("first_comment", "ALTER TABLE posts ADD COLUMN first_comment TEXT"),
        ("first_comment_delay", "ALTER TABLE posts ADD COLUMN first_comment_delay INTEGER"),
    ];
    for (col, stmt) in add {
        if !cols.iter().any(|c| c == col) {
            sqlx::query(*stmt).execute(pool).await?;
        }
    }
    Ok(())
}

/// All timestamps are stored as UTC RFC3339 with millis and a `Z` suffix,
/// so lexical order equals chronological order.
pub fn ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn now() -> String {
    ts(Utc::now())
}

pub fn parse_ts(s: &str) -> Option<String> {
    DateTime::parse_from_rfc3339(s).ok().map(|t| ts(t.with_timezone(&Utc)))
}
