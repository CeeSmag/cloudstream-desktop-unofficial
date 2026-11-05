//! Application state managed by Tauri and passed to command handlers.

use std::sync::{Arc, RwLock};
use ui_app::UiServices;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    services: Arc<RwLock<UiServices>>,
}

impl AppState {
    /// Wraps the application services.
    pub fn new(services: UiServices) -> Self {
        Self {
            services: Arc::new(RwLock::new(services)),
        }
    }

    /// Obtains a read lock on services.
    pub fn read(&self) -> std::sync::RwLockReadGuard<'_, UiServices> {
        self.services
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Runs blocking service work on a background thread pool without starving Tauri's async runtime.
    pub async fn run_blocking<T, F>(&self, work: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(&UiServices) -> Result<T, String> + Send + 'static,
    {
        let state = self.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let services = state.read();
            work(&services)
        })
        .await
        .map_err(|error| format!("Background task error: {error}"))?
    }

    /// Obtains a write lock on services.
    #[allow(dead_code)]
    pub fn write(&self) -> std::sync::RwLockWriteGuard<'_, UiServices> {
        self.services
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
