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
