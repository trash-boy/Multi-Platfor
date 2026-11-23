//! QR-code login sessions. catbus draws the QR on stderr and also saves it to
//! `$CATBUS_HOME/cache/<platform>/qrcode.png`; we serve that PNG to the browser
//! and watch the process until the user confirms on their phone.
//!
//! Douyin often answers a scan with error 2046 ("second verification"): catbus
//! then sends an SMS code and reads it from stdin. We keep the child's stdin
//! open so the browser can submit that code (`submit_code`); with a closed stdin
//! catbus silently skips the SMS step and the login can never complete.
//!
//! Xiaohongshu does the same after the phone confirms (471 + verifyType 120):
//! catbus runs the web SmsCaptcha flow, sends a code to the bound phone and
//! reads it from stdin, allowing up to 3 tries; an empty line aborts the login.

use crate::catbus::{parse_envelope, stderr_tail, Catbus};
use serde::Serialize;
use std::{collections::HashMap, path::PathBuf, process::Stdio, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    io::AsyncWriteExt,
    process::ChildStdin,
    sync::Mutex,
    task::AbortHandle,
};

#[derive(Clone, Serialize)]
pub struct LoginStatus {
    pub id: String,
    pub platform: String,
    pub account: String,
    /// waiting | scanned | verify | need_code | success | failed
    ///
    /// `verify`: the platform demands a second verification (Douyin 2046);
    /// `need_code`: an SMS code was sent and catbus is waiting for it on stdin.
    pub state: String,
    pub message: Option<String>,
    pub qrcode_ready: bool,
    /// Bumped whenever catbus writes a new QR image, so the browser reloads it.
    pub qr_seq: u32,
    pub started_at: String,
}

struct Session {
    status: LoginStatus,
    abort: Option<AbortHandle>,
    stdin: Option<ChildStdin>,
}

/// What a catbus stderr line during QR login tells us.
#[derive(Debug, PartialEq)]
enum LoginEvent {
    NewQr,
    Scanned,
    /// Second verification required; message for the user.
    Verify(String),
    /// SMS code sent, catbus now reads it from stdin. Carries the masked
    /// phone number when the line names it.
    NeedCode(Option<String>),
    /// The code was wrong and catbus reads another one (Xiaohongshu); message.
    Retry(String),
    CodeOk,
    CodeBad,
}

fn classify(line: &str) -> Option<LoginEvent> {
    use LoginEvent::*;
    let l = line.trim_start_matches("[catbus]").trim();
    if l.contains("二维码图片：") {
        Some(NewQr)
    } else if l.contains("请重新输入") {
        // xhs: "小红书短信验证失败：…，请重新输入（还剩 2 次）："
        let reason = l.split_once("，请重新输入").map(|(r, _)| r).unwrap_or(l);
        let left = l.split_once("还剩 ").and_then(|(_, r)| r.split_once(' ')).map(|(n, _)| format!("，还能再试 {n} 次")).unwrap_or_default();
        Some(Retry(format!("{reason}{left}。请重新输入 6 位短信验证码")))
    } else if l.contains("请输入") && l.contains("短信验证码") {
        // douyin: "验证码已发送，请输入短信验证码（6 位；…）"
        // xhs:    "验证码已发送到 138****0000，请输入 6 位短信验证码（空行放弃）："
        let mobile = l.split_once("已发送到 ").and_then(|(_, r)| r.split_once('，')).map(|(m, _)| m.trim().to_string());
        Some(NeedCode(mobile))
    } else if l.contains("小红书要求短信验证") {
        Some(Verify("小红书风控要求短信验证，正在向账号绑定的手机发送验证码…".into()))
    } else if l.contains("验证码校验通过") || l.contains("短信验证通过") {
        Some(CodeOk)
    } else if l.contains("验证码校验未通过") {
        Some(CodeBad)
    } else if l.contains("没有短信验证选项") {
        Some(Verify("抖音要求二次验证，但这次没有提供短信方式。请在抖音 App 里完成安全验证后再点确认登录；如果一直没反应，请重新获取二维码或改用「导入 Cookie」。".into()))
    } else if l.contains("二次验证") && l.contains("短信验证") {
        let mobile = l.split_once("发往 ").and_then(|(_, r)| r.split_once('）')).map(|(m, _)| format!("（{m}）")).unwrap_or_default();
        Some(Verify(format!("抖音风控要求二次验证，正在发送短信验证码{mobile}…")))
    } else if l.contains("已扫码") {
        Some(Scanned)
    } else {
        None
    }
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
            qr_seq: 0,
            started_at: crate::db::now(),
        };
        let args = vec![platform.to_string(), "auth".into(), "login".into(), "--method".into(), "qrcode".into(), "-a".into(), account.into()];
        let mut cmd = self.catbus.command(&args);
        cmd.stdin(Stdio::piped());

        let stdin;
        let handle = match cmd.spawn() {
            Err(e) => {
                let mut st = status.clone();
                st.state = "failed".into();
                st.message = Some(format!("无法启动 catbus：{e}"));
                sessions.insert(platform.into(), Session { status: st.clone(), abort: None, stdin: None });
                return st;
            }
            Ok(mut child) => {
                stdin = child.stdin.take();
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
                            if let Some(ev) = classify(&line) {
                                mgr2.update(&key2, &id2, |s| apply(s, ev)).await;
                            }
                            buf.extend_from_slice(line.as_bytes());
                            buf.push(b'\n');
                        }
                        buf
                    });
                    let mut out = Vec::new();
                    let waited = tokio::time::timeout(Duration::from_secs(600), async {
                        let _ = stdout.read_to_end(&mut out).await;
                        child.wait().await
                    })
                    .await;
                    let err = if waited.is_ok() { err_task.await.unwrap_or_default() } else { err_task.abort(); Vec::new() };
                    let (state, message) = match waited {
                        Err(_) => ("failed", Some("登录超时（二维码或验证码已过期），请重新获取".to_string())),
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
        sessions.insert(platform.into(), Session { status: status.clone(), abort: Some(handle.abort_handle()), stdin });
        status
    }

    /// Feed the SMS code catbus is waiting for. An empty code skips SMS
    /// verification (catbus then keeps waiting for in-app verification).
    pub async fn submit_code(&self, platform: &str, code: &str) -> Result<LoginStatus, String> {
        let mut sessions = self.sessions.lock().await;
        let s = sessions.get_mut(platform).ok_or("没有进行中的登录")?;
        if s.status.state != "need_code" {
            return Err("当前不需要输入验证码".into());
        }
        let pipe = s.stdin.as_mut().ok_or("登录进程已结束")?;
        pipe.write_all(format!("{code}\n").as_bytes()).await.map_err(|e| format!("提交失败：{e}"))?;
        let _ = pipe.flush().await;
        s.status.state = "verify".into();
        s.status.message = Some(match (code.is_empty(), s.status.platform == "douyin") {
            (false, _) => "正在校验验证码…".into(),
            (true, true) => "已跳过短信验证，请在抖音 App 里完成安全验证".into(),
            // xhs: an empty line makes catbus give up the login.
            (true, false) => "已放弃短信验证，正在结束登录…".into(),
        });
        Ok(s.status.clone())
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
        st.qrcode_ready = matches!(st.state.as_str(), "waiting" | "scanned" | "verify" | "need_code") && self.qrcode_path(platform).exists();
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

fn platform_name(id: &str) -> &str {
    match id {
        "douyin" => "抖音",
        "xhs" => "小红书",
        other => other,
    }
}

fn apply(s: &mut LoginStatus, ev: LoginEvent) {
    match ev {
        LoginEvent::NewQr => {
            s.qr_seq += 1;
            // A fresh QR (first one or after expiry) resets the flow.
            s.state = "waiting".into();
            s.message = None;
        }
        LoginEvent::Scanned => {
            if s.state == "waiting" {
                s.state = "scanned".into();
            }
        }
        LoginEvent::Verify(m) => {
            s.state = "verify".into();
            s.message = Some(m);
        }
        LoginEvent::NeedCode(mobile) => {
            s.state = "need_code".into();
            let mobile = mobile
                .map(|m| format!("（{m}）"))
                .or_else(|| s.message.as_deref().and_then(|m| m.split_once('（')).and_then(|(_, r)| r.split_once('）')).map(|(m, _)| format!("（{m}）")))
                .unwrap_or_default();
            let what = if s.platform == "douyin" { "二次验证" } else { "短信验证" };
            s.message = Some(format!("{}风控要求{what}，验证码已发送到手机{mobile}，请输入 6 位短信验证码", platform_name(&s.platform)));
        }
        LoginEvent::Retry(m) => {
            s.state = "need_code".into();
            s.message = Some(m);
        }
        LoginEvent::CodeOk => {
            s.state = "verify".into();
            s.message = Some(if s.platform == "douyin" {
                "验证码校验通过，正在等待登录放行；如果手机上还有确认按钮，请点确认".into()
            } else {
                "短信验证通过，正在完成登录…".into()
            });
        }
        LoginEvent::CodeBad => {
            s.state = "verify".into();
            s.message = Some("验证码校验未通过，本次无法再次输入。可以在抖音 App 里完成验证，或重新获取二维码".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st() -> LoginStatus {
        st_for("douyin")
    }

    fn st_for(platform: &str) -> LoginStatus {
        LoginStatus { id: "1".into(), platform: platform.into(), account: "a".into(), state: "waiting".into(), message: None, qrcode_ready: false, qr_seq: 0, started_at: String::new() }
    }

    #[test]
    fn douyin_2046_sms_flow() {
        let mut s = st();
        for line in [
            "[catbus] 请用抖音 App 扫码。二维码图片：/data/catbus/cache/douyin/qrcode.png",
            "[catbus] 已扫码，请在手机上确认",
            "[catbus] 已扫码，抖音风控要求二次验证。catbus 选择短信验证（发往 138****0000），正在发送验证码…",
            "[catbus] 验证码已发送，请输入短信验证码（6 位；直接回车跳过，改为等待 App 端验证）：",
        ] {
            if let Some(ev) = classify(line) {
                apply(&mut s, ev);
            }
        }
        assert_eq!(s.qr_seq, 1);
        assert_eq!(s.state, "need_code");
        assert!(s.message.unwrap().contains("138****0000"));
        assert_eq!(classify("[catbus] 验证码校验通过，等待登录放行…"), Some(LoginEvent::CodeOk));
        assert_eq!(classify("[catbus] 验证码校验未通过，将继续等待（也可在 App 上完成验证）"), Some(LoginEvent::CodeBad));
    }

    #[test]
    fn xhs_sms_verify_flow() {
        let mut s = st_for("xhs");
        let feed = |s: &mut LoginStatus, line: &str| {
            if let Some(ev) = classify(line) {
                apply(s, ev);
            }
        };
        feed(&mut s, "[catbus] 请用小红书 App 扫码并确认。二维码图片：/data/catbus/cache/xhs/qrcode.png");
        feed(&mut s, "[catbus] 已扫码，请在手机上确认");
        assert_eq!(s.state, "scanned");
        feed(&mut s, "[catbus] 小红书要求短信验证（verifyType 120），正在准备验证…");
        assert_eq!(s.state, "verify");
        feed(&mut s, "[catbus] 验证码已发送到 138****0000，请输入 6 位短信验证码（空行放弃）：");
        assert_eq!(s.state, "need_code");
        let m = s.message.clone().unwrap();
        assert!(m.contains("小红书") && m.contains("138****0000"), "{m}");
        s.state = "verify".into();
        feed(&mut s, "[catbus] 小红书短信验证失败：验证码错误，请重新输入（还剩 2 次）：");
        assert_eq!(s.state, "need_code");
        assert!(s.message.clone().unwrap().contains("还能再试 2 次"));
        feed(&mut s, "[catbus] 短信验证通过，继续登录");
        assert_eq!(s.state, "verify");
    }

    #[test]
    fn no_sms_way_and_qr_refresh() {
        let mut s = st();
        apply(&mut s, classify("[catbus] 已扫码，抖音风控要求二次验证，但本会话没有短信验证选项。请在抖音 App 上完成验证；或用浏览器加载验证组件（脚本地址已落盘 x）").unwrap());
        assert_eq!(s.state, "verify");
        assert_eq!(classify("[catbus] 二维码已过期，换一张继续等"), None);
        apply(&mut s, classify("[catbus] 请扫码。二维码图片：/x/qrcode.png").unwrap());
        assert_eq!((s.state.as_str(), s.qr_seq, s.message.is_none()), ("waiting", 1, true));
        // A late "已扫码" must not downgrade a verify state.
        apply(&mut s, LoginEvent::Verify("x".into()));
        apply(&mut s, LoginEvent::Scanned);
        assert_eq!(s.state, "verify");
    }
}
