//! Thin wrapper around the `catbus` CLI, which is our platform adapter layer.
//! Every catbus command prints exactly one JSON envelope on stdout; logs and
//! QR codes go to stderr.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{io::AsyncWriteExt, process::Command};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatbusError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub hint: Option<String>,
    #[serde(default)]
    pub detail: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub ok: bool,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default)]
    pub data: Value,
    #[serde(default)]
    pub page: Option<Value>,
    #[serde(default)]
    pub error: Option<CatbusError>,
}

impl Envelope {
    pub fn failure(code: &str, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            platform: None,
            account: None,
            data: Value::Null,
            page: None,
            error: Some(CatbusError { code: code.into(), message: message.into(), hint: None, detail: None }),
        }
    }

    pub fn error_code(&self) -> Option<&str> {
        self.error.as_ref().map(|e| e.code.as_str())
    }

    pub fn error_text(&self) -> String {
        match &self.error {
            Some(e) => match &e.hint {
                Some(h) if !h.is_empty() => format!("[{}] {}（{}）", e.code, e.message, h),
                _ => format!("[{}] {}", e.code, e.message),
            },
            None => String::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Catbus {
    pub bin: PathBuf,
    pub home: PathBuf,
}

impl Catbus {
    /// Base command with an isolated CATBUS_HOME so logins live in our data dir.
    pub fn command<I, S>(&self, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let mut cmd = Command::new(&self.bin);
        cmd.args(args)
            .env("CATBUS_HOME", &self.home)
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        cmd
    }

    pub async fn run(&self, args: Vec<String>, stdin: Option<String>, timeout: Duration) -> Envelope {
        tracing::debug!(?args, "catbus");
        let mut cmd = self.command(&args);
        if stdin.is_some() {
            cmd.stdin(Stdio::piped());
        }
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return Envelope::failure(
                    "SPAWN",
                    format!("无法启动 catbus（{}）：{e}。请先在 runtime/ 下执行 npm install", self.bin.display()),
                );
            }
        };
        if let Some(input) = stdin {
            if let Some(mut pipe) = child.stdin.take() {
                let _ = pipe.write_all(input.as_bytes()).await;
            }
        }
        // On timeout the future (and the child) is dropped; kill_on_drop terminates it.
        match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Err(_) => Envelope::failure("TIMEOUT", format!("catbus 超过 {} 秒没有返回", timeout.as_secs())),
            Ok(Err(e)) => Envelope::failure("SPAWN", format!("catbus 执行失败：{e}")),
            Ok(Ok(out)) => parse_envelope(&out.stdout, &out.stderr),
        }
    }
}

pub fn parse_envelope(stdout: &[u8], stderr: &[u8]) -> Envelope {
    let text = String::from_utf8_lossy(stdout);
    for line in text.lines().rev() {
        let line = line.trim();
        if line.starts_with('{') {
            if let Ok(env) = serde_json::from_str::<Envelope>(line) {
                return env;
            }
        }
    }
    if let Ok(env) = serde_json::from_str::<Envelope>(text.trim()) {
        return env;
    }
    Envelope::failure("INTERNAL", format!("catbus 没有返回 JSON：{}", stderr_tail(stderr, 400)))
}

/// Last `max` chars of stderr with ANSI escapes removed (QR codes are drawn there).
pub fn stderr_tail(stderr: &[u8], max: usize) -> String {
    let clean = strip_ansi(&String::from_utf8_lossy(stderr));
    let chars: Vec<char> = clean.trim().chars().collect();
    chars[chars.len().saturating_sub(max)..].iter().collect()
}

pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for c2 in chars.by_ref() {
                    if c2.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_last_json_line() {
        let out = b"noise\n{\"ok\":true,\"data\":[1],\"error\":null}\n";
        let env = parse_envelope(out, b"");
        assert!(env.ok);
        assert_eq!(env.data, serde_json::json!([1]));
    }

    #[test]
    fn non_json_becomes_internal_error() {
        let env = parse_envelope(b"", b"\x1b[31mboom\x1b[0m");
        assert!(!env.ok);
        assert_eq!(env.error_code(), Some("INTERNAL"));
        assert!(env.error_text().contains("boom"));
    }
}
