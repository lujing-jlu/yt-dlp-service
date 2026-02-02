use std::collections::VecDeque;
use actix_web::{web, HttpResponse, Responder};
use async_stream::stream;
use serde::Deserialize;
use tempfile::TempDir;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::OwnedSemaphorePermit;

use crate::{cookies, state::AppState, util};

async fn collect_stderr(
    stderr: tokio::process::ChildStderr,
    buf: std::sync::Arc<tokio::sync::Mutex<VecDeque<String>>>,
) {
    let mut r = BufReader::new(stderr);
    let mut line = String::new();
    loop {
        line.clear();
        match r.read_line(&mut line).await {
            Ok(0) => break,
            Ok(_) => {
                let l = line.trim_end().to_string();
                if !l.is_empty() {
                    eprintln!("{}", l);
                    let mut g = buf.lock().await;
                    if g.len() >= 50 {
                        g.pop_front();
                    }
                    g.push_back(l);
                }
            }
            Err(_) => break,
        }
    }
}

async fn render_tail(buf: &tokio::sync::Mutex<VecDeque<String>>) -> String {
    let g = buf.lock().await;
    if g.is_empty() {
        return "no stderr output captured".to_string();
    }
    g.iter().cloned().collect::<Vec<_>>().join("\n")
}

fn find_ffmpeg(cfg: &crate::config::AppConfig) -> Option<String> {
    if let Some(p) = &cfg.ffmpeg_bin {
        return Some(p.to_string_lossy().to_string());
    }
    // Common macOS/Homebrew locations.
    for p in ["/opt/homebrew/bin/ffmpeg", "/usr/local/bin/ffmpeg", "/usr/bin/ffmpeg"] {
        if std::path::Path::new(p).exists() {
            return Some(p.to_string());
        }
    }
    None
}

fn build_ytdlp_base_command(cfg: &crate::config::AppConfig) -> Command {
    let mut cmd = Command::new(&cfg.ytdlp_bin);
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

    cmd.arg("--cookies")
        .arg(cfg.cookies_file.to_string_lossy().as_ref())
        .arg("--js-runtimes")
        .arg("node")
        .arg("--no-playlist")
        .arg("--no-cache-dir")
        .arg("--no-part");

    cmd
}

fn build_ytdlp_command(
    cfg: &crate::config::AppConfig,
    mode: &str,
    url: &str,
    out_path: &str,
) -> Result<Command, String> {
    let mut cmd = build_ytdlp_base_command(cfg);
    cmd.arg("-o").arg(out_path);

    if mode == "best" {
        let ffmpeg = find_ffmpeg(cfg).ok_or_else(|| {
            "ffmpeg is required for mode=best. Install ffmpeg or set ffmpeg_bin in config.toml".to_string()
        })?;
        cmd.arg("--ffmpeg-location").arg(ffmpeg);
        cmd.arg("-f")
            .arg("bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]/best")
            .arg("--merge-output-format")
            .arg("mp4");
    } else {
        cmd.arg("-f").arg("best[ext=mp4]/best");
    }

    cmd.arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    Ok(cmd)
}

#[derive(Deserialize)]
pub struct StreamRequest {
    pub url: String,
    // "progressive" (default): best single-file mp4 if available (more likely to truly stream as it downloads)
    // "best": bestvideo+bestaudio with merge (may only start streaming after merge)
    pub mode: Option<String>,
}

#[derive(Deserialize)]
pub struct ThumbnailRequest {
    pub url: String,
}

#[derive(Deserialize)]
pub struct InfoRequest {
    pub url: String,
    // When false (default), we remove the usually-huge "formats" list from the response.
    pub include_formats: Option<bool>,
}

pub async fn index() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "service": "YouTube Download Service",
        "version": "0.2.0",
        "endpoints": {
            "GET /": "Health check",
            "POST /download": "Download video then return the final mp4 (body: {url, mode})",
            "POST /thumbnail": "Download thumbnail then return the image (body: {url})",
            "POST /info": "Get video info JSON (body: {url, include_formats})"
        }
    }))
}

pub async fn stream_direct(req: web::Json<StreamRequest>, state: web::Data<AppState>) -> impl Responder {
    let url = req.url.clone();
    if url.trim().is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Missing url"
        }));
    }

    let mode = req.mode.clone().unwrap_or_else(|| "progressive".to_string());
    if mode != "progressive" && mode != "best" {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Invalid mode (expected: progressive|best)"
        }));
    }

    eprintln!("[STREAM] Request: mode={} url={}", mode, url);

    let permit = match state.limiter.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            return HttpResponse::TooManyRequests().json(serde_json::json!({
                "error": format!("Too many concurrent downloads (max: {})", state.config.max_concurrent_downloads)
            }));
        }
    };

    if let Err(e) = cookies::ensure_cookies(state.config.as_ref(), state.cookie_lock.as_ref()).await {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to refresh cookies: {}", e)
        }));
    }

    let temp_dir = match tempfile::Builder::new().prefix("yt-dlp-stream-").tempdir() {
        Ok(d) => d,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": e.to_string()
            }));
        }
    };

    // New behavior: finish server-side download first, then stream the final file back (single request).
    // We still keep cleanup on request end by capturing TempDir inside the response body stream.
    let out_path = temp_dir.path().join("video.mp4");

    let cfg = state.config.as_ref();
    let mut cmd = match build_ytdlp_command(
        cfg,
        mode.as_str(),
        url.as_str(),
        out_path.to_string_lossy().as_ref(),
    ) {
        Ok(c) => c,
        Err(msg) => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": msg
            }));
        }
    };

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Failed to start yt-dlp: {}", e)
            }));
        }
    };
    if let Some(pid) = child.id() {
        eprintln!("[STREAM] yt-dlp started (pid={})", pid);
    }

    // Capture stderr so we can return a useful error if yt-dlp fails.
    let tail_buf: std::sync::Arc<tokio::sync::Mutex<VecDeque<String>>> =
        std::sync::Arc::new(tokio::sync::Mutex::new(VecDeque::new()));
    let stderr = match child.stderr.take() {
        Some(s) => s,
        None => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to capture yt-dlp stderr"
            }));
        }
    };
    let tail_buf_clone = tail_buf.clone();
    let stderr_task = tokio::spawn(async move { collect_stderr(stderr, tail_buf_clone).await });

    // Wait for download completion. If the client disconnects during this wait, Actix will drop the handler future,
    // which drops `child` (kill_on_drop) and `temp_dir` so we don't leak disk usage.
    let status = match child.wait().await {
        Ok(s) => s,
        Err(e) => {
            let tail = render_tail(&tail_buf).await;
            return HttpResponse::BadGateway().json(serde_json::json!({
                "error": format!("Failed waiting for yt-dlp: {}", e),
                "stderr_tail": tail
            }));
        }
    };
    let _ = stderr_task.await;

    if !status.success() {
        let tail = render_tail(&tail_buf).await;
        return HttpResponse::BadGateway().json(serde_json::json!({
            "error": format!("yt-dlp exited with error (status={})", status),
            "stderr_tail": tail
        }));
    }

    let meta = match tokio::fs::metadata(&out_path).await {
        Ok(m) => m,
        Err(e) => {
            let tail = render_tail(&tail_buf).await;
            return HttpResponse::BadGateway().json(serde_json::json!({
                "error": format!("Download succeeded but output file missing: {}", e),
                "stderr_tail": tail
            }));
        }
    };
    if meta.len() == 0 {
        let tail = render_tail(&tail_buf).await;
        return HttpResponse::BadGateway().json(serde_json::json!({
            "error": "Download succeeded but output file is empty",
            "stderr_tail": tail
        }));
    }

    eprintln!("[STREAM] Download completed; streaming {} bytes", meta.len());

    // Now stream the finished file back to the client. Capture TempDir so it is deleted when the response ends.
    let body = stream! {
        let _permit: OwnedSemaphorePermit = permit;
        let _temp_dir: TempDir = temp_dir;

        let mut file = match File::open(&out_path).await {
            Ok(f) => f,
            Err(e) => {
                yield Err(e);
                return;
            }
        };

        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            match file.read(&mut buffer).await {
                Ok(0) => break,
                Ok(n) => yield Ok(bytes::Bytes::copy_from_slice(&buffer[..n])),
                Err(e) => {
                    yield Err(e);
                    break;
                }
            }
        }
    };

    let filename = util::video_id_from_url(&url).unwrap_or_else(|| "video".to_string());
    HttpResponse::Ok()
        .content_type("video/mp4")
        .append_header((actix_web::http::header::CONTENT_LENGTH, meta.len().to_string()))
        .append_header((
            actix_web::http::header::CONTENT_DISPOSITION,
            format!(r#"attachment; filename="{}.mp4""#, filename),
        ))
        .append_header((actix_web::http::header::CACHE_CONTROL, "no-store"))
        .streaming(body)
}

pub async fn thumbnail(req: web::Json<ThumbnailRequest>, state: web::Data<AppState>) -> impl Responder {
    let url = req.url.clone();
    if url.trim().is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Missing url"
        }));
    }

    eprintln!("[THUMBNAIL] Request: url={}", url);

    let permit = match state.limiter.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            return HttpResponse::TooManyRequests().json(serde_json::json!({
                "error": format!("Too many concurrent downloads (max: {})", state.config.max_concurrent_downloads)
            }));
        }
    };

    if let Err(e) = cookies::ensure_cookies(state.config.as_ref(), state.cookie_lock.as_ref()).await {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to refresh cookies: {}", e)
        }));
    }

    let temp_dir = match tempfile::Builder::new().prefix("yt-dlp-thumb-").tempdir() {
        Ok(d) => d,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": e.to_string()
            }));
        }
    };

    let out_template = temp_dir.path().join("thumbnail.%(ext)s");

    let cfg = state.config.as_ref();
    let mut cmd = build_ytdlp_base_command(cfg);
    cmd.arg("--skip-download")
        .arg("--write-thumbnail")
        .arg("-o")
        .arg(out_template.to_string_lossy().as_ref());

    // If ffmpeg is available, ask yt-dlp to convert to jpg for consistent output.
    if let Some(ffmpeg) = find_ffmpeg(cfg) {
        cmd.arg("--ffmpeg-location").arg(ffmpeg);
        cmd.arg("--convert-thumbnails").arg("jpg");
    }

    cmd.arg(url.as_str())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Failed to start yt-dlp: {}", e)
            }));
        }
    };
    if let Some(pid) = child.id() {
        eprintln!("[THUMBNAIL] yt-dlp started (pid={})", pid);
    }

    let tail_buf: std::sync::Arc<tokio::sync::Mutex<VecDeque<String>>> =
        std::sync::Arc::new(tokio::sync::Mutex::new(VecDeque::new()));
    let stderr = match child.stderr.take() {
        Some(s) => s,
        None => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to capture yt-dlp stderr"
            }));
        }
    };
    let tail_buf_clone = tail_buf.clone();
    let stderr_task = tokio::spawn(async move { collect_stderr(stderr, tail_buf_clone).await });

    let status = match child.wait().await {
        Ok(s) => s,
        Err(e) => {
            let tail = render_tail(&tail_buf).await;
            return HttpResponse::BadGateway().json(serde_json::json!({
                "error": format!("Failed waiting for yt-dlp: {}", e),
                "stderr_tail": tail
            }));
        }
    };
    let _ = stderr_task.await;

    if !status.success() {
        let tail = render_tail(&tail_buf).await;
        return HttpResponse::BadGateway().json(serde_json::json!({
            "error": format!("yt-dlp exited with error (status={})", status),
            "stderr_tail": tail
        }));
    }

    // Find produced thumbnail file; prefer jpg/jpeg, then png, then webp.
    let mut jpg: Option<std::path::PathBuf> = None;
    let mut png: Option<std::path::PathBuf> = None;
    let mut webp: Option<std::path::PathBuf> = None;

    let mut entries = match tokio::fs::read_dir(temp_dir.path()).await {
        Ok(e) => e,
        Err(e) => {
            return HttpResponse::BadGateway().json(serde_json::json!({
                "error": format!("Failed to read thumbnail dir: {}", e)
            }));
        }
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let p = entry.path();
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "jpg" | "jpeg" => jpg = Some(p),
            "png" => png = Some(p),
            "webp" => webp = Some(p),
            _ => {}
        }
    }

    let path = jpg.or(png).or(webp);
    let path = match path {
        Some(p) => p,
        None => {
            return HttpResponse::BadGateway().json(serde_json::json!({
                "error": "No thumbnail file found"
            }));
        }
    };

    let meta = match tokio::fs::metadata(&path).await {
        Ok(m) => m,
        Err(e) => {
            return HttpResponse::BadGateway().json(serde_json::json!({
                "error": format!("Thumbnail file missing: {}", e)
            }));
        }
    };

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpg")
        .to_string();
    let ct = match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    };

    let filename = util::video_id_from_url(&url).unwrap_or_else(|| "thumbnail".to_string());
    let thumb_path = path.clone();

    let body = stream! {
        let _permit: OwnedSemaphorePermit = permit;
        let _temp_dir: TempDir = temp_dir;

        let mut file = match File::open(&thumb_path).await {
            Ok(f) => f,
            Err(e) => {
                yield Err(e);
                return;
            }
        };

        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            match file.read(&mut buffer).await {
                Ok(0) => break,
                Ok(n) => yield Ok(bytes::Bytes::copy_from_slice(&buffer[..n])),
                Err(e) => {
                    yield Err(e);
                    break;
                }
            }
        }
    };

    HttpResponse::Ok()
        .content_type(ct)
        .append_header((actix_web::http::header::CONTENT_LENGTH, meta.len().to_string()))
        .append_header((
            actix_web::http::header::CONTENT_DISPOSITION,
            format!(r#"attachment; filename="{}.{}""#, filename, ext),
        ))
        .append_header((actix_web::http::header::CACHE_CONTROL, "no-store"))
        .streaming(body)
}

pub async fn info(req: web::Json<InfoRequest>, state: web::Data<AppState>) -> impl Responder {
    let url = req.url.clone();
    if url.trim().is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Missing url"
        }));
    }

    let include_formats = req.include_formats.unwrap_or(false);

    eprintln!("[INFO] Request: url={} include_formats={}", url, include_formats);

    let permit = match state.limiter.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            return HttpResponse::TooManyRequests().json(serde_json::json!({
                "error": format!("Too many concurrent downloads (max: {})", state.config.max_concurrent_downloads)
            }));
        }
    };

    if let Err(e) = cookies::ensure_cookies(state.config.as_ref(), state.cookie_lock.as_ref()).await {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "error": format!("Failed to refresh cookies: {}", e)
        }));
    }

    let cfg = state.config.as_ref();
    let mut cmd = build_ytdlp_base_command(cfg);
    cmd.arg("--no-playlist")
        .arg("-J")
        .arg(url.as_str())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    // Keep the concurrency slot held while we run yt-dlp.
    let _permit: OwnedSemaphorePermit = permit;

    let out = match cmd.output().await {
        Ok(o) => o,
        Err(e) => {
            return HttpResponse::BadGateway().json(serde_json::json!({
                "error": format!("Failed to run yt-dlp: {}", e),
            }));
        }
    };

    if !out.status.success() {
        let stderr_tail = String::from_utf8_lossy(&out.stderr).to_string();
        return HttpResponse::BadGateway().json(serde_json::json!({
            "error": format!("yt-dlp exited with error (status={})", out.status),
            "stderr_tail": stderr_tail
        }));
    }

    let mut v: serde_json::Value = match serde_json::from_slice(&out.stdout) {
        Ok(v) => v,
        Err(e) => {
            return HttpResponse::BadGateway().json(serde_json::json!({
                "error": format!("Failed to parse yt-dlp JSON: {}", e),
            }));
        }
    };

    if !include_formats {
        if let Some(obj) = v.as_object_mut() {
            obj.remove("formats");
            obj.remove("requested_formats");
        }
    }

    HttpResponse::Ok()
        .content_type("application/json")
        .append_header((actix_web::http::header::CACHE_CONTROL, "no-store"))
        .json(v)
}
