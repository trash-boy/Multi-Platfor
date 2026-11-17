use std::path::PathBuf;

/// Runtime configuration, read from environment variables with dev-friendly defaults
/// (paths default to siblings of the `backend/` directory).
#[derive(Clone, Debug)]
pub struct Config {
    pub bind: String,
    pub data_dir: PathBuf,
    pub catbus_bin: PathBuf,
    pub catbus_home: PathBuf,
    pub static_dir: PathBuf,
    /// When set, every request must carry HTTP Basic auth with this password.
    pub password: Option<String>,
}

impl Config {
    pub fn from_env() -> Self {
        let root = std::env::var("MPF_ROOT").map(PathBuf::from).unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".."))
        });
        let path = |key: &str, default: PathBuf| std::env::var(key).map(PathBuf::from).unwrap_or(default);

        let data_dir = path("MPF_DATA_DIR", root.join("data"));
        Self {
            bind: std::env::var("MPF_BIND").unwrap_or_else(|_| "127.0.0.1:8787".into()),
            catbus_bin: path("MPF_CATBUS_BIN", root.join("runtime/node_modules/.bin/catbus")),
            catbus_home: path("MPF_CATBUS_HOME", data_dir.join("catbus")),
            static_dir: path("MPF_STATIC_DIR", root.join("frontend/dist")),
            password: std::env::var("MPF_PASSWORD").ok().filter(|p| !p.is_empty()),
            data_dir,
        }
    }

    pub fn media_dir(&self) -> PathBuf {
        self.data_dir.join("media")
    }

    pub fn tmp_dir(&self) -> PathBuf {
        self.data_dir.join("tmp")
    }
}
