//! Application state managed by Tauri and passed to command handlers.

use std::sync::{Arc, RwLock};
use ui_app::UiServices;

/// Shared application state.
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

    /// Obtains a write lock on services.
    #[allow(dead_code)]
    pub fn write(&self) -> std::sync::RwLockWriteGuard<'_, UiServices> {
        self.services
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
