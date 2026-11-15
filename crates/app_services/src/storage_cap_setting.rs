//! The size limit of the app's data folder (Settings > General > "Storage limit").

use crate::{ServiceError, SettingsService};

const STORAGE_CAP_KEY: &str = "storage_cap_gb";
/// The least size (in GB) the app's data folder may be limited to.
pub(crate) const MIN_STORAGE_CAP_GB: u32 = 1;
/// The most size (in GB) the app's data folder may be limited to.
pub(crate) const MAX_STORAGE_CAP_GB: u32 = 10;
/// The limit (in GB) until the viewer chooses one.
pub(crate) const DEFAULT_STORAGE_CAP_GB: u32 = 2;

impl SettingsService {
    /// How many GB the app's data folder may use before old cache is deleted: 2 until the viewer
    /// chooses 1 to 10. A stored value out of range (edited by hand) counts as the default.
    pub fn storage_cap_gb(&self) -> Result<u32, ServiceError> {
        let stored = self.stored(STORAGE_CAP_KEY)?;
        Ok(stored
            .and_then(|text| text.parse::<u32>().ok())
            .filter(|gb| (MIN_STORAGE_CAP_GB..=MAX_STORAGE_CAP_GB).contains(gb))
            .unwrap_or(DEFAULT_STORAGE_CAP_GB))
    }

    /// Remembers the data folder's size limit; only 1 to 10 GB are accepted.
    pub fn set_storage_cap_gb(&self, gb: u32) -> Result<(), ServiceError> {
        if !(MIN_STORAGE_CAP_GB..=MAX_STORAGE_CAP_GB).contains(&gb) {
            return Err(ServiceError::NotFound(format!(
                "the storage limit must be {MIN_STORAGE_CAP_GB} to {MAX_STORAGE_CAP_GB} GB, not {gb}"
            )));
        }
        self.store_value(STORAGE_CAP_KEY, &gb.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use storage::{Database, SettingsStore};

    use super::*;

    fn service() -> SettingsService {
        let database = Arc::new(Database::open_in_memory().expect("memory db"));
        SettingsService::new(SettingsStore::new(database))
    }

    #[test]
    fn the_storage_limit_is_two_gb_until_chosen_and_only_one_to_ten_are_accepted() {
        let service = service();
        assert_eq!(
            service.storage_cap_gb().expect("default"),
            DEFAULT_STORAGE_CAP_GB
        );
        service.set_storage_cap_gb(7).expect("seven");
        assert_eq!(service.storage_cap_gb().expect("read"), 7);
        assert!(service.set_storage_cap_gb(0).is_err());
        assert!(service.set_storage_cap_gb(11).is_err());
        assert_eq!(service.storage_cap_gb().expect("unchanged"), 7);
    }

    #[test]
    fn a_stored_limit_out_of_range_counts_as_the_default() {
        let service = service();
        service.store_value(STORAGE_CAP_KEY, "50").expect("write");
        assert_eq!(
            service.storage_cap_gb().expect("read"),
            DEFAULT_STORAGE_CAP_GB
        );
    }
}
