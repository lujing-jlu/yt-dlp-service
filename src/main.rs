use std::sync::Arc;
use std::time::Duration;

use actix_web::{web, App, HttpServer};
use tokio::sync::{Mutex as AsyncMutex, Semaphore};
use tokio::time;

mod config;
mod cookies;
mod handlers;
mod state;
mod util;

use crate::state::AppState;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let cfg_path = std::env::args()
        .skip_while(|a| a != "--config")
        .skip(1)
        .next()
        .unwrap_or_else(|| "config.toml".to_string());

    let cfg = match config::AppConfig::load(&cfg_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[CONFIG] Failed to load {}: {:#}", cfg_path, e);
            std::process::exit(1);
        }
    };

    println!("========================================");
    println!("  YouTube Download Service");
    println!("  http://{}", cfg.listen_addr);
    println!("========================================");
    println!();

    let state = web::Data::new(AppState {
        limiter: Arc::new(Semaphore::new(cfg.max_concurrent_downloads)),
        cookie_lock: Arc::new(AsyncMutex::new(())),
        config: Arc::new(cfg),
    });

    // Keep cookies warm in the background.
    {
        let cookie_lock = state.cookie_lock.clone();
        let cfg = state.config.clone();
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(300)); // check every 5 minutes
            loop {
                interval.tick().await;
                if let Err(e) = cookies::ensure_cookies(cfg.as_ref(), cookie_lock.as_ref()).await {
                    eprintln!("[COOKIES] Background refresh failed: {}", e);
                }
            }
        });
    }

    let bind_addr = state.config.listen_addr.clone();
    HttpServer::new(move || {
        App::new()
            .wrap(actix_web::middleware::Logger::default())
            .app_data(state.clone())
            .service(web::resource("/").route(web::get().to(handlers::index)))
            .service(web::resource("/download").route(web::post().to(handlers::stream_direct)))
            .service(web::resource("/thumbnail").route(web::post().to(handlers::thumbnail)))
            .service(web::resource("/info").route(web::post().to(handlers::info)))
    })
    .bind(bind_addr.as_str())?
    .run()
    .await
}
