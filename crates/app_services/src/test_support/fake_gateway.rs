use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use core_model::{
    HomePage, LoadDetails, MainPageEntry, PlayableLinks, ProviderInfo, SearchPage, SearchResult,
};
use extension_api::{GatewayError, ProviderGateway};

/// A gateway with scripted answers and call counters.
#[derive(Default)]
pub(crate) struct FakeGateway {
    pub(crate) link_replies: Mutex<VecDeque<Result<PlayableLinks, GatewayError>>>,
    pub(crate) extract_reply: Mutex<Option<PlayableLinks>>,
    pub(crate) search_replies: Mutex<Vec<(String, Result<SearchPage, GatewayError>)>>,
    pub(crate) loaded_paths: Mutex<Vec<(String, bool)>>,
    pub(crate) load_links_calls: Mutex<u32>,
    pub(crate) searched: Mutex<Vec<String>>,
    pub(crate) load_reply: Mutex<Option<Result<Option<LoadDetails>, GatewayError>>>,
}

fn unavailable<T>() -> Result<T, GatewayError> {
    Err(GatewayError::Unavailable("not scripted".to_owned()))
}

impl FakeGateway {
    pub(crate) fn with_search(provider: &str, names: &[&str]) -> Self {
        let page = SearchPage {
            items: names
                .iter()
                .map(|name| SearchResult {
                    name: (*name).to_owned(),
                    api_name: provider.to_owned(),
                    ..SearchResult::default()
                })
                .collect(),
            has_next: false,
        };
        let gateway = Self::default();
        gateway
            .search_replies
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((provider.to_owned(), Ok(page)));
        gateway
    }
}

impl ProviderGateway for FakeGateway {
    fn load_extension(
        &self,
        path: &Path,
        trusted: bool,
    ) -> Result<Vec<ProviderInfo>, GatewayError> {
        self.loaded_paths
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((path.to_string_lossy().into_owned(), trusted));
        let name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.starts_with("Broken") {
            return Err(GatewayError::Remote {
                kind: "ClassNotFoundException".into(),
                message: name,
            });
        }
        Ok(vec![ProviderInfo {
            name,
            ..ProviderInfo::default()
        }])
    }

    fn search(&self, provider: &str, _query: &str, _page: u32) -> Result<SearchPage, GatewayError> {
        self.searched
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(provider.to_owned());
        let replies = self
            .search_replies
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        replies
            .iter()
            .find(|(name, _)| name == provider)
            .map_or_else(unavailable, |(_, reply)| reply.clone())
    }

    fn main_page(
        &self,
        _p: &str,
        _page: u32,
        _e: &MainPageEntry,
    ) -> Result<Option<HomePage>, GatewayError> {
        unavailable()
    }

    fn load(&self, _provider: &str, _url: &str) -> Result<Option<LoadDetails>, GatewayError> {
        self.load_reply
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .unwrap_or_else(unavailable)
    }

    fn load_links(&self, _provider: &str, _data: &str) -> Result<PlayableLinks, GatewayError> {
        *self
            .load_links_calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner) += 1;
        self.link_replies
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
            .unwrap_or_else(unavailable)
    }

    fn extract(&self, _url: &str, _referer: Option<&str>) -> Result<PlayableLinks, GatewayError> {
        self.extract_reply
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .map_or_else(unavailable, Ok)
    }
}
