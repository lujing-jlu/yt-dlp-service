use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use tokio::process::Command;
use tokio::sync::Mutex as AsyncMutex;

use crate::config::AppConfig;

/// Automatically refresh cookies (export from browser) into cookies file.
pub async fn refresh_cookies(cfg: &AppConfig) -> Result<()> {
    eprintln!(
        "[COOKIES] Refreshing from browser: {}...",
        cfg.cookies_browser
    );

    // `--cookies FILE` reads from and dumps cookie jar in that file.
    // We hit an arbitrary video URL but skip download; goal is just to populate/update cookies file.
    let ytdlp_bin = &cfg.ytdlp_bin;
    let browser = &cfg.cookies_browser;
    let cookie_file = &cfg.cookies_file;

    let mut cmd = Command::new(ytdlp_bin);
    cmd.env("PATH", &cfg.ytdlp_path);
    if !cfg.inherit_proxy_env {
        // Avoid being accidentally bound to a dead local proxy (common in shell env).
        cmd.env_remove("http_proxy")
            .env_remove("https_proxy")
            .env_remove("HTTP_PROXY")
            .env_remove("HTTPS_PROXY")
            .env_remove("no_proxy")
            .env_remove("NO_PROXY");
    }

    if let Some(p) = &cfg.ytdlp_proxy {
        cmd.arg("--proxy").arg(p);
    }

    let output = cmd
        .args(&[
            "--cookies-from-browser",
            browser.as_str(),
            "--cookies",
            cookie_file.to_string_lossy().as_ref(),
            "--skip-download",
            "--quiet",
            "--no-warnings",
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        ])
        .output()
        .await
        .context("Failed to run yt-dlp for cookies")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("yt-dlp failed: {}", stderr));
    }

    eprintln!("[COOKIES] Refreshed successfully");
    Ok(())
}

/// Check if cookies need refresh (missing or older than configured max age).
pub fn needs_refresh(cfg: &AppConfig) -> bool {
    if !PathBuf::from(&cfg.cookies_file).exists() {
        return true;
    }

    if let Ok(metadata) = std::fs::metadata(&cfg.cookies_file) {
        if let Ok(modified) = metadata.modified() {
            if let Ok(elapsed) = modified.elapsed() {
                return elapsed.as_secs() > cfg.cookies_refresh_max_age_secs;
            }
        }
    }
    false
}

pub async fn ensure_cookies(cfg: &AppConfig, cookie_lock: &AsyncMutex<()>) -> Result<()> {
    if cfg.cookies_source != "file" {
        // In browser mode, we rely on `--cookies-from-browser` at runtime.
        return Ok(());
    }

    // Avoid multiple concurrent refreshes under load (and avoid writing cookies file concurrently).
    let _guard = cookie_lock.lock().await;
    if needs_refresh(cfg) {
        refresh_cookies(cfg).await?;
    }
    Ok(())
}
