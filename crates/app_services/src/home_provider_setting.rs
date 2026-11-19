//! The provider whose shelves Home shows.

use crate::{ServiceError, SettingsService};

const HOME_PROVIDER_KEY: &str = "home_provider";

impl SettingsService {
    /// The provider the viewer picked for Home, if any (it may no longer be installed).
    pub fn home_provider(&self) -> Result<Option<String>, ServiceError> {
        Ok(self
            .stored(HOME_PROVIDER_KEY)?
            .filter(|name| !name.is_empty()))
    }

    /// Remembers the provider for Home, so it is the same after a restart.
    pub fn set_home_provider(&self, name: &str) -> Result<(), ServiceError> {
        self.store_value(HOME_PROVIDER_KEY, name)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use storage::{Database, SettingsStore};

    use super::*;

    #[test]
    fn home_provider_is_none_until_picked_then_kept() {
        let database = Arc::new(Database::open_in_memory().expect("memory db"));
        let service = SettingsService::new(SettingsStore::new(database));
        assert_eq!(service.home_provider().expect("read"), None);
        service.set_home_provider("CineStream").expect("set");
        assert_eq!(
            service.home_provider().expect("read").as_deref(),
            Some("CineStream")
        );
    }
}
