//! QR-code login sessions. catbus draws the QR on stderr and also saves it to
//! `$CATBUS_HOME/cache/<platform>/qrcode.png`; we serve that PNG to the browser
//! and watch the process until the user confirms on their phone.

use crate::catbus::{parse_envelope, stderr_tail, Catbus};
use serde::Serialize;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    sync::Mutex,
    task::AbortHandle,
};

#[derive(Clone, Serialize)]
pub struct LoginStatus {
    pub id: String,
    pub platform: String,
    pub account: String,
    /// waiting | scanned | success | failed
    pub state: String,
    pub message: Option<String>,
    pub qrcode_ready: bool,
    pub started_at: String,
}

struct Session {
    status: LoginStatus,
    abort: Option<AbortHandle>,
}

#[derive(Clone)]
pub struct LoginManager {
    catbus: Catbus,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
}

impl LoginManager {
    pub fn new(catbus: Catbus) -> Self {
        Self { catbus, sessions: Arc::default() }
    }

    pub fn qrcode_path(&self, platform: &str) -> PathBuf {
        self.catbus.home.join("cache").join(platform).join("qrcode.png")
    }

    /// Start (or restart) a QR login for `platform`. catbus keeps one QR file per
    /// platform, so only one login per platform can run at a time.
    pub async fn start(&self, platform: &str, account: &str) -> LoginStatus {
        let mut sessions = self.sessions.lock().await;
        if let Some(old) = sessions.remove(platform) {
            if let Some(a) = old.abort {
                a.abort();
            }
        }
        let _ = tokio::fs::remove_file(self.qrcode_path(platform)).await;

        let status = LoginStatus {
            id: uuid::Uuid::new_v4().to_string(),
            platform: platform.into(),
            account: account.into(),
            state: "waiting".into(),
            message: None,
            qrcode_ready: false,
            started_at: crate::db::now(),
        };
        let args = vec![platform.to_string(), "auth".into(), "login".into(), "--method".into(), "qrcode".into(), "-a".into(), account.into()];
        let mut cmd = self.catbus.command(&args);

        let handle = match cmd.spawn() {
            Err(e) => {
                let mut st = status.clone();
                st.state = "failed".into();
                st.message = Some(format!("无法启动 catbus：{e}"));
                sessions.insert(platform.into(), Session { status: st.clone(), abort: None });
                return st;
            }
            Ok(mut child) => {
                let mgr = self.clone();
                let key = platform.to_string();
                let id = status.id.clone();
                tokio::spawn(async move {
                    let mut stdout = child.stdout.take().expect("piped");
                    let stderr = child.stderr.take().expect("piped");
                    let mgr2 = mgr.clone();
                    let (key2, id2) = (key.clone(), id.clone());
                    let err_task = tokio::spawn(async move {
                        let mut buf = Vec::new();
                        let mut lines = BufReader::new(stderr).lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            if line.contains("已扫码") {
                                mgr2.update(&key2, &id2, |s| s.state = "scanned".into()).await;
                            }
                            buf.extend_from_slice(line.as_bytes());
                            buf.push(b'\n');
                        }
                        buf
                    });
                    let mut out = Vec::new();
                    let waited = tokio::time::timeout(Duration::from_secs(330), async {
                        let _ = stdout.read_to_end(&mut out).await;
                        child.wait().await
                    })
                    .await;
                    let err = if waited.is_ok() { err_task.await.unwrap_or_default() } else { err_task.abort(); Vec::new() };
                    let (state, message) = match waited {
                        Err(_) => ("failed", Some("二维码已过期，请重新获取".to_string())),
                        Ok(_) => {
                            let env = parse_envelope(&out, &err);
                            if env.ok {
                                let name = env.data.pointer("/user/name").and_then(|v| v.as_str()).map(|n| format!("已登录：{n}"));
                                ("success", name)
                            } else {
                                let mut msg = env.error_text();
                                if env.error_code() == Some("INTERNAL") {
                                    msg = stderr_tail(&err, 300);
                                }
                                ("failed", Some(msg))
                            }
                        }
                    };
                    mgr.update(&key, &id, |s| {
                        s.state = state.into();
                        s.message = message;
                    })
                    .await;
                })
            }
        };
        sessions.insert(platform.into(), Session { status: status.clone(), abort: Some(handle.abort_handle()) });
        status
    }

    async fn update(&self, platform: &str, id: &str, f: impl FnOnce(&mut LoginStatus)) {
        if let Some(s) = self.sessions.lock().await.get_mut(platform) {
            if s.status.id == id {
                f(&mut s.status);
            }
        }
    }

    pub async fn status(&self, platform: &str) -> Option<LoginStatus> {
        let mut st = self.sessions.lock().await.get(platform)?.status.clone();
        st.qrcode_ready = matches!(st.state.as_str(), "waiting" | "scanned") && self.qrcode_path(platform).exists();
        Some(st)
    }

    pub async fn cancel(&self, platform: &str) {
        if let Some(s) = self.sessions.lock().await.remove(platform) {
            if let Some(a) = s.abort {
                a.abort();
            }
        }
    }
}
