use std::sync::Arc;

use tokio::sync::{Mutex as AsyncMutex, Semaphore};

use crate::config::AppConfig;

pub struct AppState {
    pub limiter: Arc<Semaphore>,
    pub cookie_lock: Arc<AsyncMutex<()>>,
    pub config: Arc<AppConfig>,
}
