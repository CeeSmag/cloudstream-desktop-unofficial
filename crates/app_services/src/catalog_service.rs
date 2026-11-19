use std::sync::Arc;

use core_model::{HomePage, LoadDetails, MainPageEntry, ProviderInfo, SearchPage};
use extension_api::{GatewayError, ProviderGateway};

use crate::ServiceError;

/// The outcome of searching one provider; a failing provider never hides the others.
#[derive(Debug)]
pub struct ProviderSearchOutcome {
    /// Provider that was searched.
    pub provider: String,
    /// Its results, or why it failed.
    pub result: Result<SearchPage, GatewayError>,
}

/// Browsing: search, home pages and title details.
#[derive(Clone)]
pub struct CatalogService {
    gateway: Arc<dyn ProviderGateway>,
}

impl CatalogService {
    /// Creates the service over the extension runtime.
    pub fn new(gateway: Arc<dyn ProviderGateway>) -> Self {
        Self { gateway }
    }

    /// Searches every given provider (page 1) and reports each outcome separately.
    #[tracing::instrument(level = "debug", skip_all, fields(query = ?query))]
    pub fn search(&self, providers: &[ProviderInfo], query: &str) -> Vec<ProviderSearchOutcome> {
        providers
            .iter()
            .map(|provider| {
                let result = self.gateway.search(&provider.name, query, 1);
                if let Err(error) = &result {
                    tracing::warn!(provider = %provider.name, %error, "search failed");
                }
                ProviderSearchOutcome {
                    provider: provider.name.clone(),
                    result,
                }
            })
            .collect()
    }

    /// Loads the first page of one home page category of a provider.
    #[tracing::instrument(level = "debug", skip_all, fields(provider = ?provider), err)]
    pub fn home(
        &self,
        provider: &str,
        entry: &MainPageEntry,
    ) -> Result<Option<HomePage>, ServiceError> {
        self.home_page(provider, 1, entry)
    }

    /// Loads `page` (1-based) of one home page category of a provider.
    #[tracing::instrument(level = "debug", skip_all, fields(provider = ?provider, page), err)]
    pub fn home_page(
        &self,
        provider: &str,
        page: u32,
        entry: &MainPageEntry,
    ) -> Result<Option<HomePage>, ServiceError> {
        Ok(self.gateway.main_page(provider, page, entry)?)
    }

    /// Loads the details of a title.
    #[tracing::instrument(level = "debug", skip_all, fields(provider = ?provider, url = ?url), err)]
    pub fn details(&self, provider: &str, url: &str) -> Result<LoadDetails, ServiceError> {
        self.gateway
            .load(provider, url)?
            .ok_or_else(|| ServiceError::NotFound(format!("{provider} returned nothing for {url}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeGateway;

    fn provider(name: &str) -> ProviderInfo {
        ProviderInfo {
            name: name.to_owned(),
            ..ProviderInfo::default()
        }
    }

    #[test]
    fn one_failing_provider_does_not_hide_the_others() {
        let gateway = FakeGateway::with_search("Good", &["Apollo 11"]);
        let service = CatalogService::new(Arc::new(gateway));
        let outcomes = service.search(&[provider("Broken"), provider("Good")], "apollo");
        assert!(outcomes[0].result.is_err());
        assert_eq!(
            outcomes[1].result.as_ref().expect("good works").items[0].name,
            "Apollo 11"
        );
    }

    #[test]
    fn details_reports_not_found_when_provider_returns_nothing() {
        let gateway = FakeGateway::default();
        *gateway.load_reply.lock().expect("lock") = Some(Ok(None));
        let service = CatalogService::new(Arc::new(gateway));
        assert!(matches!(
            service.details("P", "u"),
            Err(ServiceError::NotFound(_))
        ));
    }

    #[test]
    fn details_returns_what_the_provider_loaded() {
        let gateway = FakeGateway::default();
        let details = LoadDetails {
            name: "Apollo".into(),
            ..LoadDetails::default()
        };
        *gateway.load_reply.lock().expect("lock") = Some(Ok(Some(details)));
        let service = CatalogService::new(Arc::new(gateway));
        assert_eq!(service.details("P", "u").expect("loads").name, "Apollo");
    }

    #[test]
    fn home_page_forwards_requested_page_number_to_gateway() {
        let gateway = Arc::new(FakeGateway::default());
        *gateway.main_page_reply.lock().expect("lock") = Some(Ok(Some(HomePage {
            items: Vec::new(),
            has_next: true,
        })));
        let service = CatalogService::new(gateway.clone());
        let entry = MainPageEntry {
            name: "Popular Movies".into(),
            data: "popular".into(),
            horizontal_images: false,
        };
        let page = service
            .home_page("StreamFlix", 3, &entry)
            .expect("loads")
            .expect("has page");
        assert!(page.has_next);
        assert_eq!(
            *gateway.main_page_calls.lock().expect("lock"),
            vec![("StreamFlix".to_owned(), 3, "Popular Movies".to_owned())]
        );
    }
}
