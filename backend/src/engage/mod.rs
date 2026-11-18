//! Post-publish engagement: pulling stats / comments / DMs back from the
//! platforms (`sync`), and automatic comments (`auto`).
//!
//! Everything here goes through catbus with the user's own logins, so every
//! request counts against the account's risk budget. The sync runs on a slow
//! cadence (default 2h), only looks at recent works, and spaces calls with
//! random gaps.

pub mod auto;
pub mod sync;

use serde::Serialize;
use std::time::Duration;

/// What catbus can do per platform for engagement (from `catbus platforms <p>`).
#[derive(Serialize, Clone, Copy, Debug)]
pub struct Caps {
    pub platform: &'static str,
    /// `user items me` returns own works with stats.
    pub stats: bool,
    /// `comment list <item>` per work (used for the inbox).
    pub comments: bool,
    /// `notice list` – types we ingest into the inbox.
    pub notices: &'static [&'static str],
    /// `comment add <item> <text> [--reply-to]`.
    pub comment_add: bool,
    /// `msg list` / `msg history`.
    pub dm_read: bool,
    /// `msg send --conversation`.
    pub dm_send: bool,
}

const fn caps(platform: &'static str, stats: bool, comments: bool, notices: &'static [&'static str], comment_add: bool, dm_read: bool, dm_send: bool) -> Caps {
    Caps { platform, stats, comments, notices, comment_add, dm_read, dm_send }
}

/// 小红书 comments come from notices (one call covers every note, and catbus
/// cannot reply on 小红书 yet anyway). 抖音 comments are pulled per work so they
/// carry a comment id we can reply to; its notices only add @mentions.
pub const CAPS: &[Caps] = &[
    caps("xhs", true, false, &["comment", "mention"], false, true, true),
    caps("douyin", true, true, &["mention"], true, false, false),
    caps("bilibili", true, true, &[], true, false, false),
    caps("x", true, true, &[], true, true, false),
    caps("weibo", true, true, &[], false, false, false),
    caps("kuaishou", true, true, &[], false, false, false),
    caps("xianyu", false, false, &[], false, false, false),
];

pub fn caps_of(platform: &str) -> Option<Caps> {
    CAPS.iter().copied().find(|c| c.platform == platform)
}

pub fn can_comment(platform: &str) -> bool {
    caps_of(platform).is_some_and(|c| c.comment_add)
}

/// Cheap randomness without a dependency (uuid v4 is backed by the OS RNG).
pub fn rand_u64() -> u64 {
    uuid::Uuid::new_v4().as_u128() as u64
}

/// Uniform random duration in [lo, hi].
pub fn jitter(lo: Duration, hi: Duration) -> Duration {
    let span = hi.saturating_sub(lo).as_millis() as u64;
    lo + Duration::from_millis(if span == 0 { 0 } else { rand_u64() % (span + 1) })
}

pub fn account_args(account: &str) -> Vec<String> {
    vec!["-a".into(), account.into()]
}
