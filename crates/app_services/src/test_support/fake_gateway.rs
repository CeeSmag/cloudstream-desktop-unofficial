use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use core_model::{
    DiscoveredRepository, HomePage, LinkSearchCursor, LinkSearchUpdate, LoadDetails, MainPageEntry,
    PlayableLinks, ProviderInfo, SearchPage, SearchResult,
};
use extension_api::{GatewayError, ProviderGateway};

/// A provider a scripted plugin registers: its name and the types it reports.
pub(crate) type ScriptedProvider = (String, Vec<String>);

/// A gateway with scripted answers and call counters.
#[derive(Default)]
pub(crate) struct FakeGateway {
    pub(crate) link_replies: Mutex<VecDeque<Result<PlayableLinks, GatewayError>>>,
    pub(crate) extract_reply: Mutex<Option<PlayableLinks>>,
    pub(crate) search_replies: Mutex<Vec<(String, Result<SearchPage, GatewayError>)>>,
    pub(crate) loaded_paths: Mutex<Vec<(String, bool)>>,
    pub(crate) unloaded_paths: Mutex<Vec<String>>,
    pub(crate) load_links_calls: Mutex<u32>,
    pub(crate) searched: Mutex<Vec<String>>,
    pub(crate) load_reply: Mutex<Option<Result<Option<LoadDetails>, GatewayError>>>,
    pub(crate) main_page_calls: Mutex<Vec<(String, u32, String)>>,
    pub(crate) main_page_reply: Mutex<Option<Result<Option<HomePage>, GatewayError>>>,
    /// Answers to link-search polls; `None` means the runtime cannot stream links.
    pub(crate) search_updates: Mutex<Option<VecDeque<Result<LinkSearchUpdate, GatewayError>>>>,
    pub(crate) searches_started: Mutex<u32>,
    pub(crate) searches_cancelled: Mutex<u32>,
    /// Repositories the plugins asked for; handed out once.
    pub(crate) added_repositories: Mutex<Vec<DiscoveredRepository>>,
    /// Runtime controls, in order, as `debug true` / `extend 30s` / `shutdown`.
    pub(crate) controls: Mutex<Vec<String>>,
    /// When true, the runtime refuses the debug-logging choice.
    pub(crate) refuses_debug_logging: std::sync::atomic::AtomicBool,
    /// Providers (name, types) a plugin file registers, by file stem; otherwise one provider named
    /// after the file with no types.
    pub(crate) providers_by_file: Mutex<HashMap<String, Vec<ScriptedProvider>>>,
    /// Runs inside `unload_extension`, before it returns (to look at the service mid-call).
    pub(crate) on_unload: Mutex<Option<Box<dyn Fn() + Send>>>,
}

impl FakeGateway {
    fn control(&self, call: String) {
        self.controls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(call);
    }
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
        if let Some(scripted) = self
            .providers_by_file
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&name)
        {
            return Ok(scripted
                .iter()
                .map(|(provider, types)| ProviderInfo {
                    name: provider.clone(),
                    types: types.clone(),
                    ..ProviderInfo::default()
                })
                .collect());
        }
        Ok(vec![ProviderInfo {
            name,
            ..ProviderInfo::default()
        }])
    }

    fn unload_extension(&self, path: &Path) -> Result<(), GatewayError> {
        self.unloaded_paths
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(path.to_string_lossy().into_owned());
        if let Some(hook) = self
            .on_unload
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            hook();
        }
        Ok(())
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
        provider: &str,
        page: u32,
        entry: &MainPageEntry,
    ) -> Result<Option<HomePage>, GatewayError> {
        self.main_page_calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((provider.to_owned(), page, entry.name.clone()));
        self.main_page_reply
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .unwrap_or_else(unavailable)
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

    fn start_link_search(&self, _provider: &str, _data: &str) -> Result<String, GatewayError> {
        let scripted = self
            .search_updates
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some();
        if !scripted {
            return unavailable();
        }
        let mut started = self
            .searches_started
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *started += 1;
        Ok(format!("search-{started}"))
    }

    fn poll_link_search(
        &self,
        _id: &str,
        _cursor: LinkSearchCursor,
        _wait: Duration,
    ) -> Result<LinkSearchUpdate, GatewayError> {
        self.search_updates
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
            .and_then(VecDeque::pop_front)
            .unwrap_or_else(unavailable)
    }

    fn cancel_link_search(&self, _id: &str) -> Result<(), GatewayError> {
        *self
            .searches_cancelled
            .lock()
            .unwrap_or_else(PoisonError::into_inner) += 1;
        Ok(())
    }

    fn extract(&self, _url: &str, _referer: Option<&str>) -> Result<PlayableLinks, GatewayError> {
        self.extract_reply
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .map_or_else(unavailable, Ok)
    }

    fn take_added_repositories(&self) -> Result<Vec<DiscoveredRepository>, GatewayError> {
        Ok(std::mem::take(
            &mut *self
                .added_repositories
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        ))
    }

    fn extend_call_deadline(&self, extra: Duration) {
        self.control(format!("extend {}s", extra.as_secs()));
    }

    fn shutdown(&self) {
        self.control("shutdown".to_owned());
    }

    fn set_debug_logging(&self, enabled: bool) -> Result<(), GatewayError> {
        self.control(format!("debug {enabled}"));
        if self
            .refuses_debug_logging
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(GatewayError::Unavailable("not started".to_owned()));
        }
        Ok(())
    }
}
