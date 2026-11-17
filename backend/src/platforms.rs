//! Platform registry: what each platform supports and how a generic post maps
//! onto its `catbus <p> item publish` flags. Shared with the frontend via
//! `GET /api/platforms` so validation rules live in one place.

use serde::Serialize;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone)]
pub struct OptionSpec {
    pub key: &'static str,
    pub label: &'static str,
    /// number | text | bool | select
    pub kind: &'static str,
    pub choices: &'static [&'static str],
    pub required: bool,
    pub help: &'static str,
}

#[derive(Serialize, Clone)]
pub struct PublishSpec {
    pub title: bool,
    pub title_required: bool,
    pub title_max: Option<usize>,
    pub text_max: Option<usize>,
    pub images: bool,
    pub max_images: usize,
    pub video: bool,
    pub video_requires_cover: bool,
    pub cover: bool,
    pub requires_media: bool,
    pub tags: bool,
    pub topics: bool,
    pub visibility: &'static [&'static str],
    pub options: Vec<OptionSpec>,
    pub notes: &'static [&'static str],
}

#[derive(Serialize, Clone)]
pub struct Platform {
    pub id: &'static str,
    pub name: &'static str,
    pub color: &'static str,
    /// Login methods the web UI offers (qrcode / cookie).
    pub login_methods: &'static [&'static str],
    pub cookie_site: &'static str,
    pub cookie_keys: &'static str,
    pub can_search: bool,
    pub can_feed: bool,
    pub can_comments: bool,
    pub publish: Option<PublishSpec>,
    /// Publishing providers that can serve this platform (see providers/mod.rs).
    pub providers: &'static [&'static str],
}

const fn opt(
    key: &'static str,
    label: &'static str,
    kind: &'static str,
    choices: &'static [&'static str],
    required: bool,
    help: &'static str,
) -> OptionSpec {
    OptionSpec { key, label, kind, choices, required, help }
}

pub fn all() -> Vec<Platform> {
    vec![
        Platform {
            id: "xhs",
            name: "小红书",
            color: "#FF2442",
            login_methods: &["qrcode", "cookie"],
            cookie_site: "https://www.xiaohongshu.com",
            cookie_keys: "a1、web_session",
            can_search: true,
            can_feed: true,
            can_comments: true,
            publish: Some(PublishSpec {
                title: true,
                title_required: false,
                title_max: Some(20),
                text_max: Some(1000),
                images: true,
                max_images: 18,
                video: true,
                video_requires_cover: true,
                cover: true,
                requires_media: true,
                tags: true,
                topics: true,
                visibility: &["public", "private"],
                options: vec![opt("poi-name", "地点名称", "text", &[], false, "按名称搜索地点，取名称完全相同的")],
                notes: &["图文或视频二选一，必须至少一张图或一个视频", "视频笔记需要同时上传封面"],
            }),
            providers: &["catbus", "xhs-mcp", "sau"],
        },
        Platform {
            id: "xianyu",
            name: "闲鱼",
            color: "#FFDA00",
            login_methods: &["qrcode", "cookie"],
            cookie_site: "https://www.goofish.com",
            cookie_keys: "unb、_m_h5_tk、cookie2",
            can_search: false,
            can_feed: false,
            can_comments: false,
            publish: Some(PublishSpec {
                title: true,
                title_required: false,
                title_max: Some(30),
                text_max: Some(5000),
                images: true,
                max_images: 9,
                video: false,
                video_requires_cover: false,
                cover: false,
                requires_media: true,
                tags: false,
                topics: false,
                visibility: &["public"],
                options: vec![
                    opt("price", "售价（元）", "number", &[], true, ""),
                    opt("original-price", "原价（元）", "number", &[], false, ""),
                    opt("shipping", "运费方式", "select", &["free", "distance", "fixed", "none"], false, "free 包邮 / distance 按距离 / fixed 一口价 / none 无需邮寄"),
                    opt("postage", "一口价运费（元）", "number", &[], false, "配合运费方式 fixed"),
                    opt("pickup", "支持自提", "bool", &[], false, ""),
                ],
                notes: &["发布的是闲置商品，正文即商品描述", "只支持图片；类目由闲鱼自动推荐，地点取账号第一个常用地址"],
            }),
            providers: &["catbus"],
        },
        Platform {
            id: "x",
            name: "X (Twitter)",
            color: "#000000",
            login_methods: &["cookie"],
            cookie_site: "https://x.com",
            cookie_keys: "auth_token、ct0",
            can_search: true,
            can_feed: true,
            can_comments: true,
            publish: Some(PublishSpec {
                title: false,
                title_required: false,
                title_max: None,
                text_max: Some(280),
                images: true,
                max_images: 4,
                video: true,
                video_requires_cover: false,
                cover: false,
                requires_media: false,
                tags: false,
                topics: false,
                visibility: &["public"],
                options: vec![opt("quote", "引用推文", "text", &[], false, "推文 ID 或链接")],
                notes: &["超过 280 权重（中文算 2）会自动发长推，需要 Premium", "最多 4 张图，图片和视频不能同时用"],
            }),
            providers: &["x-api", "catbus"],
        },
        Platform {
            id: "weibo",
            name: "微博",
            color: "#E6162D",
            login_methods: &["cookie"],
            cookie_site: "https://weibo.com",
            cookie_keys: "SUB",
            can_search: true,
            can_feed: true,
            can_comments: true,
            publish: Some(PublishSpec {
                title: false,
                title_required: false,
                title_max: None,
                text_max: Some(5000),
                images: true,
                max_images: 9,
                video: true,
                video_requires_cover: false,
                cover: false,
                requires_media: false,
                tags: false,
                topics: true,
                visibility: &["public", "private", "friends", "fans"],
                options: vec![opt("poi-name", "地点名称", "text", &[], false, "")],
                notes: &[],
            }),
            providers: &["catbus", "sau"],
        },
        Platform {
            id: "douyin",
            name: "抖音",
            color: "#111111",
            login_methods: &["qrcode"],
            cookie_site: "https://www.douyin.com",
            cookie_keys: "sessionid（写操作还需要设备素材，推荐扫码）",
            can_search: true,
            can_feed: true,
            can_comments: true,
            publish: Some(PublishSpec {
                title: true,
                title_required: false,
                title_max: Some(30),
                text_max: Some(1000),
                images: true,
                max_images: 35,
                video: true,
                video_requires_cover: false,
                cover: true,
                requires_media: true,
                tags: true,
                topics: true,
                visibility: &["public", "private", "friends"],
                options: vec![opt("hotspot", "关联热点", "text", &[], false, "")],
                notes: &["图文或视频，必须带媒体"],
            }),
            providers: &["catbus", "sau"],
        },
        Platform {
            id: "bilibili",
            name: "B站",
            color: "#00A1D6",
            login_methods: &["qrcode", "cookie"],
            cookie_site: "https://www.bilibili.com",
            cookie_keys: "SESSDATA、bili_jct",
            can_search: true,
            can_feed: true,
            can_comments: true,
            publish: Some(PublishSpec {
                title: true,
                title_required: true,
                title_max: Some(80),
                text_max: Some(2000),
                images: false,
                max_images: 0,
                video: true,
                video_requires_cover: false,
                cover: true,
                requires_media: true,
                tags: true,
                topics: false,
                visibility: &["public", "private"],
                options: vec![
                    opt("category", "分区 ID", "text", &[], false, "B站分区 tid，例如 249；走 sau 通道时必填"),
                    opt("source", "转载来源", "text", &[], false, "填写后按转载投稿，不填为自制"),
                    opt("dynamic", "同步动态文案", "text", &[], false, ""),
                ],
                notes: &["只能投稿视频，标题必填"],
            }),
            providers: &["catbus", "sau"],
        },
        Platform {
            id: "kuaishou",
            name: "快手",
            color: "#FF4906",
            login_methods: &["qrcode", "cookie"],
            cookie_site: "https://www.kuaishou.com",
            cookie_keys: "完整 Cookie",
            can_search: true,
            can_feed: true,
            can_comments: true,
            publish: Some(PublishSpec {
                title: true,
                title_required: false,
                title_max: Some(30),
                text_max: Some(1000),
                images: true,
                max_images: 31,
                video: true,
                video_requires_cover: false,
                cover: true,
                requires_media: true,
                tags: true,
                topics: true,
                visibility: &["public", "private"],
                options: vec![],
                notes: &["图文或视频，必须带媒体"],
            }),
            providers: &["catbus", "sau"],
        },
        Platform {
            id: "tencent",
            name: "视频号",
            color: "#FA9D3B",
            login_methods: &[],
            cookie_site: "https://channels.weixin.qq.com",
            cookie_keys: "",
            can_search: false,
            can_feed: false,
            can_comments: false,
            publish: Some(PublishSpec {
                title: true,
                title_required: false,
                title_max: Some(16),
                text_max: Some(1000),
                images: false,
                max_images: 0,
                video: true,
                video_requires_cover: false,
                cover: false,
                requires_media: true,
                tags: true,
                topics: true,
                visibility: &["public"],
                options: vec![],
                notes: &["只支持视频，通过 social-auto-upload 发布", "先在终端执行 sau tencent login --account <名字>，再到「通道设置」绑定"],
            }),
            providers: &["sau"],
        },
    ]
}

pub fn get(id: &str) -> Option<Platform> {
    all().into_iter().find(|p| p.id == id)
}

/// Everything needed to publish to one platform/account.
pub struct PublishInput<'a> {
    pub account: &'a str,
    pub title: Option<&'a str>,
    pub text: &'a str,
    pub images: Vec<PathBuf>,
    pub video: Option<PathBuf>,
    pub cover: Option<PathBuf>,
    pub tags: &'a [String],
    pub topics: &'a [String],
    pub visibility: Option<&'a str>,
    pub options: &'a Map<String, Value>,
}

/// Shape-only description used for validation before files exist on disk.
pub struct PostShape<'a> {
    pub title: Option<&'a str>,
    pub text: &'a str,
    pub image_count: usize,
    pub has_video: bool,
    pub has_cover: bool,
    pub visibility: Option<&'a str>,
    pub options: &'a Map<String, Value>,
}

/// Tags/topics are silently dropped on platforms without them, so they are not validated.
pub fn validate(p: &Platform, s: &PostShape) -> Vec<String> {
    let mut errs = Vec::new();
    let Some(spec) = &p.publish else {
        return vec![format!("{} 不支持发布", p.name)];
    };
    let title = s.title.map(str::trim).filter(|t| !t.is_empty());
    if spec.title_required && title.is_none() {
        errs.push("标题必填".into());
    }
    if let (Some(t), Some(max)) = (title, spec.title_max) {
        if t.chars().count() > max {
            errs.push(format!("标题超过 {max} 字"));
        }
    }
    // X counts weight, and catbus turns long text into a Premium long post, so don't block it.
    if p.id != "x" {
        if let Some(max) = spec.text_max {
            if s.text.chars().count() > max {
                errs.push(format!("正文超过 {max} 字"));
            }
        }
    }
    if title.is_none() && s.text.trim().is_empty() && s.image_count == 0 && !s.has_video {
        errs.push("内容为空".into());
    }
    if s.image_count > 0 && !spec.images {
        errs.push("不支持图片".into());
    }
    if s.image_count > spec.max_images && spec.images {
        errs.push(format!("最多 {} 张图片", spec.max_images));
    }
    if s.has_video && !spec.video {
        errs.push("不支持视频".into());
    }
    if s.has_video && s.image_count > 0 {
        errs.push("图片和视频不能同时发".into());
    }
    if s.has_video && spec.video_requires_cover && !s.has_cover {
        errs.push("视频需要封面".into());
    }
    if spec.requires_media && s.image_count == 0 && !s.has_video {
        errs.push(if spec.images { "必须至少一张图片或一个视频".into() } else { "必须上传视频".into() });
    }
    if let Some(v) = s.visibility {
        if !v.is_empty() && !spec.visibility.contains(&v) {
            errs.push(format!("不支持可见范围 {v}"));
        }
    }
    for o in &spec.options {
        let v = s.options.get(o.key);
        let empty = matches!(v, None | Some(Value::Null)) || v.and_then(Value::as_str).is_some_and(|x| x.trim().is_empty());
        if o.required && empty {
            errs.push(format!("{} 必填", o.label));
        }
        if let (Some(v), "number") = (v, o.kind) {
            if !empty && value_to_string(v).parse::<f64>().is_err() {
                errs.push(format!("{} 必须是数字", o.label));
            }
        }
        if let (Some(Value::String(v)), "select") = (v, o.kind) {
            if !v.is_empty() && !o.choices.contains(&v.as_str()) {
                errs.push(format!("{} 取值无效", o.label));
            }
        }
    }
    for k in s.options.keys() {
        if !spec.options.iter().any(|o| o.key == k) {
            errs.push(format!("未知选项 {k}"));
        }
    }
    errs
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.trim().to_string(),
        other => other.to_string(),
    }
}

/// Build `catbus <p> item publish ...` argv. Text is passed through a file
/// (`--text @path`) so content starting with `-` or `@` is never misparsed.
pub fn publish_args(p: &Platform, input: &PublishInput, text_file: &Path) -> Vec<String> {
    let spec = p.publish.as_ref().expect("validated");
    let mut a: Vec<String> = vec![p.id.into(), "item".into(), "publish".into(), "-a".into(), input.account.into()];
    let mut push = |flag: &str, val: String| {
        a.push(flag.into());
        a.push(val);
    };
    if spec.title {
        if let Some(t) = input.title.map(str::trim).filter(|t| !t.is_empty()) {
            push("--title", t.into());
        }
    }
    if !input.text.trim().is_empty() {
        push("--text", format!("@{}", text_file.display()));
    }
    for img in &input.images {
        push("--image", img.display().to_string());
    }
    if let Some(v) = &input.video {
        push("--video", v.display().to_string());
    }
    if spec.cover {
        if let Some(c) = &input.cover {
            push("--cover", c.display().to_string());
        }
    }
    if spec.tags {
        for t in input.tags.iter().filter(|t| !t.trim().is_empty()) {
            push("--tag", t.trim().into());
        }
    }
    if spec.topics {
        for t in input.topics.iter().filter(|t| !t.trim().is_empty()) {
            push("--topic", t.trim().into());
        }
    }
    if let Some(v) = input.visibility.filter(|v| !v.is_empty()) {
        push("--visibility", v.into());
    }
    for o in &spec.options {
        match (o.kind, input.options.get(o.key)) {
            ("bool", Some(Value::Bool(true))) => a.push(format!("--{}", o.key)),
            ("bool", _) | (_, None) | (_, Some(Value::Null)) => {}
            (_, Some(v)) => {
                let s = value_to_string(v);
                if !s.is_empty() {
                    a.push(format!("--{}", o.key));
                    a.push(s);
                }
            }
        }
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn shape<'a>(opts: &'a Map<String, Value>) -> PostShape<'a> {
        PostShape {
            title: Some("标题"),
            text: "正文",
            image_count: 1,
            has_video: false,
            has_cover: false,
            visibility: None,
            options: opts,
        }
    }

    #[test]
    fn xianyu_requires_price() {
        let p = get("xianyu").unwrap();
        let empty = Map::new();
        assert!(validate(&p, &shape(&empty)).iter().any(|e| e.contains("售价")));
        let ok = json!({"price": "88"}).as_object().unwrap().clone();
        assert!(validate(&p, &shape(&ok)).is_empty());
    }

    #[test]
    fn xhs_requires_media() {
        let p = get("xhs").unwrap();
        let o = Map::new();
        let mut s = shape(&o);
        s.image_count = 0;
        assert!(!validate(&p, &s).is_empty());
    }

    #[test]
    fn x_too_many_images() {
        let p = get("x").unwrap();
        let o = Map::new();
        let mut s = shape(&o);
        s.image_count = 5;
        assert!(validate(&p, &s).iter().any(|e| e.contains("4")));
    }

    #[test]
    fn builds_xianyu_args() {
        let p = get("xianyu").unwrap();
        let opts = json!({"price": 88, "pickup": true, "shipping": "free"}).as_object().unwrap().clone();
        let input = PublishInput {
            account: "default",
            title: Some("键盘"),
            text: "九成新",
            images: vec![PathBuf::from("/m/a.jpg")],
            video: None,
            cover: None,
            tags: &[],
            topics: &[],
            visibility: None,
            options: &opts,
        };
        let args = publish_args(&p, &input, Path::new("/tmp/t.txt"));
        let joined = args.join(" ");
        assert!(joined.starts_with("xianyu item publish -a default --title 键盘 --text @/tmp/t.txt --image /m/a.jpg"));
        assert!(joined.contains("--price 88"));
        assert!(joined.contains("--pickup"));
        assert!(joined.contains("--shipping free"));
    }

    #[test]
    fn x_drops_title() {
        let p = get("x").unwrap();
        let opts = Map::new();
        let input = PublishInput {
            account: "me",
            title: Some("ignored"),
            text: "hello",
            images: vec![],
            video: None,
            cover: None,
            tags: &["t".into()],
            topics: &[],
            visibility: None,
            options: &opts,
        };
        let args = publish_args(&p, &input, Path::new("/tmp/t.txt"));
        assert!(!args.contains(&"--title".to_string()));
        assert!(!args.contains(&"--tag".to_string()));
    }
}
