//! How many plugin JVMs stay resident (Settings > General > "Plugin processes").

use crate::{ServiceError, SettingsService};

const KEY: &str = "plugin_worker_limit";

/// Choices offered in Settings (1 / 2 / unlimited).
pub const PLUGIN_WORKER_LIMIT_CHOICES: &[Option<usize>] = &[Some(1), Some(2), None];

impl SettingsService {
    /// Caps how many plugin processes stay alive. `None` = unlimited.
    ///
    /// Missing key: writes **1** on a fresh install, or **unlimited** when `had_prior_data` so
    /// existing installs are not surprised by parking workers.
    pub fn plugin_worker_limit_migrating(
        &self,
        had_prior_data: bool,
    ) -> Result<Option<usize>, ServiceError> {
        if let Some(stored) = self.stored(KEY)? {
            return Ok(parse_limit(&stored));
        }
        let limit = if had_prior_data { None } else { Some(1) };
        self.set_plugin_worker_limit(limit)?;
        Ok(limit)
    }

    /// The stored cap (`None` = unlimited). Missing key means unlimited until migrating runs.
    pub fn plugin_worker_limit(&self) -> Result<Option<usize>, ServiceError> {
        Ok(self.stored(KEY)?.as_deref().and_then(parse_limit))
    }

    /// Remembers the resident-worker cap and returns it.
    pub fn set_plugin_worker_limit(&self, limit: Option<usize>) -> Result<(), ServiceError> {
        self.store_value(KEY, &format_limit(limit))?;
        Ok(())
    }
}

fn parse_limit(text: &str) -> Option<usize> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("unlimited") || trimmed == "0" {
        return None;
    }
    trimmed.parse::<usize>().ok().filter(|n| *n > 0)
}

fn format_limit(limit: Option<usize>) -> String {
    match limit {
        None => "0".to_owned(),
        Some(n) => n.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use storage::{Database, SettingsStore};

    use super::*;

    fn service() -> SettingsService {
        SettingsService::new(SettingsStore::new(Arc::new(
            Database::open_in_memory().expect("memory db"),
        )))
    }

    #[test]
    fn fresh_install_defaults_to_one_and_persists() {
        let service = service();
        assert_eq!(
            service.plugin_worker_limit_migrating(false).expect("m"),
            Some(1)
        );
        assert_eq!(service.plugin_worker_limit().expect("r"), Some(1));
    }

    #[test]
    fn existing_install_keeps_unlimited_until_chosen() {
        let service = service();
        assert_eq!(
            service.plugin_worker_limit_migrating(true).expect("m"),
            None
        );
        assert_eq!(service.plugin_worker_limit().expect("r"), None);
    }

    #[test]
    fn set_round_trips_and_rejects_nothing_extra() {
        let service = service();
        service.set_plugin_worker_limit(Some(2)).expect("2");
        assert_eq!(service.plugin_worker_limit().expect("r"), Some(2));
        service.set_plugin_worker_limit(None).expect("u");
        assert_eq!(service.plugin_worker_limit().expect("r"), None);
    }
}
