//! How many plugin JVMs stay resident (Settings > General > "Plugin processes").

use memory_budget::ResidentPlugins;

use crate::{ServiceError, SettingsService};

pub(crate) const KEY: &str = "plugin_worker_limit";

/// Choices offered in Settings (`None` = unlimited).
pub const PLUGIN_WORKER_LIMIT_CHOICES: &[Option<usize>] =
    &[Some(1), Some(2), Some(3), Some(4), Some(6), None];

impl SettingsService {
    /// How many plugin processes stay alive (`None` = unlimited): what the viewer chose, else the
    /// profile's value (1 on a low-end PC, more on bigger ones).
    pub fn plugin_worker_limit(&self) -> Result<Option<usize>, ServiceError> {
        Ok(self.performance_budget()?.resident_plugins.as_limit())
    }

    /// Remembers the viewer's choice of resident plugin processes (`None` = unlimited).
    pub fn set_plugin_worker_limit(&self, limit: Option<usize>) -> Result<(), ServiceError> {
        self.store_value(KEY, &format_limit(limit))?;
        Ok(())
    }

    /// The viewer's own choice, if one is stored and readable.
    pub(crate) fn stored_worker_choice(&self) -> Result<Option<ResidentPlugins>, ServiceError> {
        Ok(self.stored(KEY)?.as_deref().and_then(parse_stored))
    }
}

/// `0` is how an unlimited choice is written; anything else must be a positive count.
fn parse_stored(text: &str) -> Option<ResidentPlugins> {
    match text.trim() {
        "0" => Some(ResidentPlugins::Unlimited),
        other => ResidentPlugins::parse(other),
    }
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
    fn until_chosen_the_limit_follows_the_profile() {
        assert_eq!(service().plugin_worker_limit().expect("r"), Some(2));
    }

    #[test]
    fn set_round_trips_and_unlimited_is_a_choice_of_its_own() {
        let service = service();
        service.set_plugin_worker_limit(Some(3)).expect("3");
        assert_eq!(service.plugin_worker_limit().expect("r"), Some(3));
        service.set_plugin_worker_limit(None).expect("u");
        assert_eq!(service.plugin_worker_limit().expect("r"), None);
    }

    #[test]
    fn a_stored_value_that_is_not_offered_is_ignored() {
        let service = service();
        service.set_plugin_worker_limit(Some(5)).expect("5");
        assert_eq!(service.plugin_worker_limit().expect("r"), Some(2));
        service.store_value(KEY, "many").expect("w");
        assert_eq!(service.plugin_worker_limit().expect("r"), Some(2));
    }
}
