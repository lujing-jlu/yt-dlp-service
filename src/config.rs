use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub listen_addr: String,
    pub max_concurrent_downloads: usize,

    // "browser" (default) or "file"
    pub cookies_source: String,
    pub cookies_file: PathBuf,
    pub cookies_browser: String,
    pub cookies_refresh_max_age_secs: u64,

    pub ytdlp_bin: PathBuf,
    pub ytdlp_path: String,
    pub ffmpeg_bin: Option<PathBuf>,
    // Preferred: explicit yt-dlp proxy (e.g. socks5://127.0.0.1:7890).
    pub ytdlp_proxy: Option<String>,
    // Whether to let yt-dlp inherit http_proxy/https_proxy from the service environment.
    pub inherit_proxy_env: bool,
}

#[derive(Debug, Deserialize)]
struct AppConfigFile {
    listen_addr: Option<String>,
    max_concurrent_downloads: Option<usize>,

    cookies_source: Option<String>,
    cookies_file: Option<String>,
    cookies_browser: Option<String>,
    cookies_refresh_max_age_secs: Option<u64>,

    ytdlp_bin: Option<String>,
    ytdlp_path: Option<String>,
    ffmpeg_bin: Option<String>,
    ytdlp_proxy: Option<String>,
    inherit_proxy_env: Option<bool>,
}

fn default_ytdlp_path() -> String {
    // Prefer inheriting PATH from the service process; override via config.toml when needed
    // (e.g. to include Homebrew, ffmpeg, node from nvm, etc).
    std::env::var("PATH").unwrap_or_else(|_| {
        "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin".to_string()
    })
}

impl AppConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path).with_context(|| {
            format!(
                "Failed to read config file: {}",
                path.to_string_lossy().as_ref()
            )
        })?;

        let file: AppConfigFile = toml::from_str(&raw).context("Failed to parse config.toml")?;

        let cfg = Self {
            listen_addr: file.listen_addr.unwrap_or_else(|| "0.0.0.0:8080".to_string()),
            max_concurrent_downloads: file.max_concurrent_downloads.unwrap_or(5),

            cookies_source: file
                .cookies_source
                .unwrap_or_else(|| "browser".to_string())
                .to_ascii_lowercase(),
            cookies_file: PathBuf::from(file.cookies_file.unwrap_or_else(|| "cookies.txt".to_string())),
            cookies_browser: file.cookies_browser.unwrap_or_else(|| "edge".to_string()),
            cookies_refresh_max_age_secs: file.cookies_refresh_max_age_secs.unwrap_or(1800),

            ytdlp_bin: PathBuf::from(file.ytdlp_bin.unwrap_or_else(|| "yt-dlp".to_string())),
            ytdlp_path: file.ytdlp_path.unwrap_or_else(default_ytdlp_path),
            ffmpeg_bin: file.ffmpeg_bin.and_then(|s| {
                let s = s.trim().to_string();
                if s.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(s))
                }
            }),
            ytdlp_proxy: file
                .ytdlp_proxy
                .and_then(|s| {
                    let s = s.trim().to_string();
                    if s.is_empty() { None } else { Some(s) }
                }),
            inherit_proxy_env: file.inherit_proxy_env.unwrap_or(false),
        };

        if cfg.cookies_source != "browser" && cfg.cookies_source != "file" {
            return Err(anyhow!(
                "Invalid cookies_source: {} (expected: browser|file)",
                cfg.cookies_source
            ));
        }

        Ok(cfg)
    }
}
