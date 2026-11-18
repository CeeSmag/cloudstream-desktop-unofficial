//! Searches the enabled providers for a title (the Search screen and opening a Browse title).

use app_services::UiServices;
use core_model::{ProviderInfo, SearchResult};

/// What one search across the providers found.
#[derive(Debug, Clone, Default)]
pub(crate) struct SearchSummary {
    pub(crate) query: String,
    pub(crate) items: Vec<SearchResult>,
    /// Providers whose search failed.
    pub(crate) failed: Vec<String>,
    /// How many providers were asked.
    pub(crate) asked: usize,
}

/// The providers the viewer did not switch off, `preferred` first.
pub(crate) fn enabled_providers(services: &UiServices, preferred: &str) -> Vec<ProviderInfo> {
    let disabled = services
        .settings
        .disabled_providers()
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "could not read the switched-off providers; searching all");
            Vec::new()
        });
    let mut providers: Vec<ProviderInfo> = services
        .extensions
        .providers()
        .into_iter()
        .filter(|info| !disabled.contains(&info.name))
        .collect();
    providers.sort_by_key(|info| info.name != preferred);
    providers
}

/// Searches `providers` for `query`; results keep the providers' order.
pub(crate) fn search_providers(
    services: &UiServices,
    providers: &[ProviderInfo],
    query: &str,
) -> SearchSummary {
    let mut summary = SearchSummary {
        query: query.to_owned(),
        asked: providers.len(),
        ..SearchSummary::default()
    };
    for outcome in services.catalog.search(providers, query) {
        match outcome.result {
            Ok(page) => summary.items.extend(page.items),
            Err(error) => {
                tracing::warn!(provider = %outcome.provider, %error, "a provider's search failed");
                summary.failed.push(outcome.provider);
            }
        }
    }
    summary
}

/// The line under the search box once a search finished ("" when there is nothing to say).
pub(crate) fn search_message(summary: &SearchSummary) -> String {
    if summary.asked == 0 {
        return "No provider to search. Add a plugin in Settings → Extensions.".to_owned();
    }
    let failed = match summary.failed.as_slice() {
        [] => String::new(),
        [one] => format!("{one} did not answer."),
        many => format!(
            "{} of {} providers did not answer.",
            many.len(),
            summary.asked
        ),
    };
    match (summary.items.is_empty(), failed.is_empty()) {
        (true, true) => format!("Nothing found for “{}”.", summary.query),
        (true, false) => format!("Nothing found for “{}”. {failed}", summary.query),
        (false, _) => failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(items: usize, failed: &[&str], asked: usize) -> SearchSummary {
        let item = SearchResult {
            name: "x".into(),
            url: "u".into(),
            api_name: "P".into(),
            media_type: None,
            poster_url: None,
            year: None,
            poster_headers: None,
        };
        SearchSummary {
            query: "dune".into(),
            items: vec![item; items],
            failed: failed.iter().map(|name| (*name).to_owned()).collect(),
            asked,
        }
    }

    #[test]
    fn search_message_results_without_failures_say_nothing() {
        assert_eq!(search_message(&summary(3, &[], 2)), "");
    }

    #[test]
    fn search_message_no_results_names_the_query() {
        assert_eq!(
            search_message(&summary(0, &[], 2)),
            "Nothing found for “dune”."
        );
    }

    #[test]
    fn search_message_reports_failed_providers() {
        assert_eq!(search_message(&summary(2, &["A"], 3)), "A did not answer.");
        assert_eq!(
            search_message(&summary(0, &["A", "B"], 3)),
            "Nothing found for “dune”. 2 of 3 providers did not answer."
        );
    }

    #[test]
    fn search_message_without_providers_points_to_extensions() {
        assert!(search_message(&summary(0, &[], 0)).contains("Add a plugin"));
    }
}
