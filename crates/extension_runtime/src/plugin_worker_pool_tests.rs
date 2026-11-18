use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use core_model::{
    DiscoveredRepository, HomePage, LinkSearchCursor, LinkSearchUpdate, LoadDetails, MainPageEntry,
    PlayableLink, PlayableLinks, ProviderInfo, SearchPage, SearchResult,
};
use extension_api::{GatewayError, ProviderGateway};

use super::*;

/// Scripted per-plugin worker used by the pool tests.
struct ScriptedWorker {
    label: String,
    providers: Vec<String>,
    search_hits: Mutex<u32>,
    extract_reply: Mutex<Option<PlayableLinks>>,
    extract_calls: Mutex<u32>,
    repos: Mutex<Vec<DiscoveredRepository>>,
    repos_fail: bool,
    shutdowns: Arc<AtomicUsize>,
    load_fail: bool,
}

impl ScriptedWorker {
    fn ok(label: &str, providers: &[&str]) -> Arc<Self> {
        Arc::new(Self {
            label: label.to_owned(),
            providers: providers.iter().map(|name| (*name).to_owned()).collect(),
            search_hits: Mutex::new(0),
            extract_reply: Mutex::new(None),
            extract_calls: Mutex::new(0),
            repos: Mutex::new(Vec::new()),
            repos_fail: false,
            shutdowns: Arc::new(AtomicUsize::new(0)),
            load_fail: false,
        })
    }

    fn with_repos_fail(self: &Arc<Self>) -> Arc<Self> {
        Arc::new(Self {
            label: self.label.clone(),
            providers: self.providers.clone(),
            search_hits: Mutex::new(0),
            extract_reply: Mutex::new(None),
            extract_calls: Mutex::new(0),
            repos: Mutex::new(Vec::new()),
            repos_fail: true,
            shutdowns: Arc::clone(&self.shutdowns),
            load_fail: false,
        })
    }

    fn with_extract(self: &Arc<Self>, links: PlayableLinks) -> Arc<Self> {
        *self
            .extract_reply
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(links);
        Arc::clone(self)
    }

    fn with_repo(self: &Arc<Self>, name: &str, url: &str) -> Arc<Self> {
        self.repos
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(DiscoveredRepository {
                name: name.to_owned(),
                url: url.to_owned(),
                icon_url: None,
            });
        Arc::clone(self)
    }
}

impl ProviderGateway for ScriptedWorker {
    fn load_extension(
        &self,
        _path: &Path,
        _trusted: bool,
    ) -> Result<Vec<ProviderInfo>, GatewayError> {
        if self.load_fail {
            return Err(GatewayError::Remote {
                kind: "LoadFailed".into(),
                message: self.label.clone(),
            });
        }
        Ok(self
            .providers
            .iter()
            .map(|name| ProviderInfo {
                name: name.clone(),
                ..ProviderInfo::default()
            })
            .collect())
    }

    fn unload_extension(&self, _path: &Path) -> Result<(), GatewayError> {
        Ok(())
    }

    fn search(&self, provider: &str, query: &str, _page: u32) -> Result<SearchPage, GatewayError> {
        *self
            .search_hits
            .lock()
            .unwrap_or_else(PoisonError::into_inner) += 1;
        Ok(SearchPage {
            items: vec![SearchResult {
                name: format!("{}:{provider}:{query}", self.label),
                api_name: provider.to_owned(),
                ..SearchResult::default()
            }],
            has_next: false,
        })
    }

    fn main_page(
        &self,
        _: &str,
        _: u32,
        _: &MainPageEntry,
    ) -> Result<Option<HomePage>, GatewayError> {
        Ok(None)
    }

    fn load(&self, _: &str, _: &str) -> Result<Option<LoadDetails>, GatewayError> {
        Ok(None)
    }

    fn load_links(&self, _: &str, _: &str) -> Result<PlayableLinks, GatewayError> {
        Ok(PlayableLinks::default())
    }

    fn start_link_search(&self, provider: &str, _data: &str) -> Result<String, GatewayError> {
        Ok(format!("search-{}-{}", self.label, provider))
    }

    fn poll_link_search(
        &self,
        _id: &str,
        _cursor: LinkSearchCursor,
        _wait: Duration,
    ) -> Result<LinkSearchUpdate, GatewayError> {
        Ok(LinkSearchUpdate {
            done: true,
            success: true,
            ..LinkSearchUpdate::default()
        })
    }

    fn cancel_link_search(&self, _id: &str) -> Result<(), GatewayError> {
        Ok(())
    }

    fn extract(&self, _url: &str, _referer: Option<&str>) -> Result<PlayableLinks, GatewayError> {
        *self
            .extract_calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner) += 1;
        match self
            .extract_reply
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
        {
            Some(links) => Ok(links),
            None => Ok(PlayableLinks::default()),
        }
    }

    fn take_added_repositories(&self) -> Result<Vec<DiscoveredRepository>, GatewayError> {
        if self.repos_fail {
            return Err(GatewayError::Transport("worker pipe broken".into()));
        }
        Ok(std::mem::take(
            &mut *self.repos.lock().unwrap_or_else(PoisonError::into_inner),
        ))
    }

    fn shutdown(&self) {
        self.shutdowns.fetch_add(1, Ordering::SeqCst);
    }
}

fn queue_spawn(workers: Vec<Arc<ScriptedWorker>>) -> SpawnFn {
    let queue = Arc::new(Mutex::new(VecDeque::from(workers)));
    Arc::new(move |_trusted| {
        let next = queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
            .unwrap_or_else(|| ScriptedWorker::ok("spare", &["Alpha", "Beta"]));
        let worker: Worker = next;
        worker
    })
}

fn useful_links(url: &str) -> PlayableLinks {
    PlayableLinks {
        success: true,
        links: vec![PlayableLink {
            url: url.to_owned(),
            ..PlayableLink::default()
        }],
        ..PlayableLinks::default()
    }
}

#[test]
fn load_two_plugins_routes_search_to_the_right_worker() {
    let a = ScriptedWorker::ok("A", &["Alpha"]);
    let b = ScriptedWorker::ok("B", &["Beta"]);
    let pool = PluginWorkerPool::with_spawn(queue_spawn(vec![Arc::clone(&a), Arc::clone(&b)]));

    pool.load_extension(Path::new("/ext/A.cs3"), false)
        .expect("load A");
    pool.load_extension(Path::new("/ext/B.cs3"), false)
        .expect("load B");

    let page = pool.search("Beta", "moon", 1).expect("search");
    assert_eq!(page.items[0].name, "B:Beta:moon");
    assert_eq!(
        *a.search_hits.lock().unwrap_or_else(PoisonError::into_inner),
        0
    );
    assert_eq!(
        *b.search_hits.lock().unwrap_or_else(PoisonError::into_inner),
        1
    );
}

#[test]
fn unload_removes_provider_route_and_shuts_the_worker_down() {
    let a = ScriptedWorker::ok("A", &["Alpha"]);
    let shutdowns = Arc::clone(&a.shutdowns);
    let pool = PluginWorkerPool::with_spawn(queue_spawn(vec![Arc::clone(&a)]));
    let path = PathBuf::from("/ext/A.cs3");
    pool.load_extension(&path, false).expect("load");
    pool.unload_extension(&path).expect("unload");
    assert!(pool.search("Alpha", "q", 1).is_err());
    assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
}

#[test]
fn extract_returns_first_useful_reply_across_workers() {
    let empty = ScriptedWorker::ok("empty", &["E"]).with_extract(PlayableLinks::default());
    let good = ScriptedWorker::ok("good", &["G"]).with_extract(useful_links("https://ok/v"));
    let pool =
        PluginWorkerPool::with_spawn(queue_spawn(vec![Arc::clone(&empty), Arc::clone(&good)]));
    pool.load_extension(Path::new("/e.cs3"), false).expect("e");
    pool.load_extension(Path::new("/g.cs3"), false).expect("g");

    let links = pool.extract("https://host/x", None).expect("extract");
    assert_eq!(links.links[0].url, "https://ok/v");
    assert!(
        *empty
            .extract_calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            >= 1
    );
}

#[test]
fn take_added_repositories_merges_every_worker() {
    let a = ScriptedWorker::ok("A", &["Alpha"]).with_repo("One", "https://one/repo.json");
    let b = ScriptedWorker::ok("B", &["Beta"]).with_repo("Two", "https://two/repo.json");
    let pool = PluginWorkerPool::with_spawn(queue_spawn(vec![a, b]));
    pool.load_extension(Path::new("/a.cs3"), false).expect("a");
    pool.load_extension(Path::new("/b.cs3"), false).expect("b");
    let found = pool.take_added_repositories().expect("repos");
    assert_eq!(found.len(), 2);
}

#[test]
fn take_added_repositories_skips_a_failed_worker() {
    let good = ScriptedWorker::ok("A", &["Alpha"]).with_repo("One", "https://one/repo.json");
    let bad = ScriptedWorker::ok("B", &["Beta"]).with_repos_fail();
    let pool = PluginWorkerPool::with_spawn(queue_spawn(vec![bad, good]));
    pool.load_extension(Path::new("/b.cs3"), false).expect("b");
    pool.load_extension(Path::new("/a.cs3"), false).expect("a");
    let found = pool.take_added_repositories().expect("repos");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "One");
}

#[test]
fn start_link_search_routes_poll_to_the_same_worker() {
    let a = ScriptedWorker::ok("A", &["Alpha"]);
    let pool = PluginWorkerPool::with_spawn(queue_spawn(vec![a]));
    pool.load_extension(Path::new("/a.cs3"), false).expect("a");
    let id = pool.start_link_search("Alpha", "data").expect("start");
    let update = pool
        .poll_link_search(&id, LinkSearchCursor::default(), Duration::from_millis(1))
        .expect("poll");
    assert!(update.done);
    // Done clears the search route.
    assert!(pool
        .poll_link_search(&id, LinkSearchCursor::default(), Duration::from_millis(1))
        .is_err());
}

fn race_pool() -> Arc<PluginWorkerPool> {
    let spawn_count = Arc::new(AtomicUsize::new(0));
    Arc::new(PluginWorkerPool::with_spawn(Arc::new(move |_trusted| {
        let n = spawn_count.fetch_add(1, Ordering::SeqCst);
        let name = format!("P{n}");
        let worker: Worker = ScriptedWorker::ok(&format!("W{n}"), &[name.as_str()]);
        worker
    })))
}

fn race_load_unload(pool: Arc<PluginWorkerPool>, barrier: Arc<Barrier>, slot: usize) {
    barrier.wait();
    for round in 0..40 {
        let path = PathBuf::from(format!("/ext/plugin-{slot}.cs3"));
        let _ = pool.load_extension(&path, false);
        let _ = pool.search(&format!("P{slot}"), "q", 1);
        if round % 2 == 0 {
            let _ = pool.unload_extension(&path);
        }
    }
}

fn race_query(pool: Arc<PluginWorkerPool>, barrier: Arc<Barrier>) {
    barrier.wait();
    for _ in 0..80 {
        let _ = pool.search("P0", "q", 1);
        let _ = pool.extract("https://x", None);
        let _ = pool.take_added_repositories();
    }
}

#[test]
fn concurrent_load_unload_and_search_keep_maps_consistent() {
    let pool = race_pool();
    let barrier = Arc::new(Barrier::new(8));
    let mut handles = Vec::new();
    for slot in 0..4 {
        let pool = Arc::clone(&pool);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || race_load_unload(pool, barrier, slot)));
    }
    for _ in 0..4 {
        let pool = Arc::clone(&pool);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || race_query(pool, barrier)));
    }
    for handle in handles {
        handle.join().expect("thread");
    }
    pool.shutdown();
    assert!(pool.search("P0", "q", 1).is_err());
}

#[test]
fn resident_limit_of_one_parks_the_older_worker_and_revives_on_search() {
    let a_first = ScriptedWorker::ok("A", &["Alpha"]);
    let a_shutdowns = Arc::clone(&a_first.shutdowns);
    let pool = PluginWorkerPool::with_spawn(queue_spawn(vec![
        a_first,
        ScriptedWorker::ok("B", &["Beta"]),
        ScriptedWorker::ok("A2", &["Alpha"]),
    ]));
    pool.set_resident_worker_limit(Some(1));
    pool.load_extension(Path::new("/ext/A.cs3"), false)
        .expect("load A");
    pool.load_extension(Path::new("/ext/B.cs3"), false)
        .expect("load B");
    assert_eq!(a_shutdowns.load(Ordering::SeqCst), 1, "A parked");
    assert_eq!(pool.lock_maps().live_count(), 1);
    let page = pool.search("Alpha", "moon", 1).expect("revive");
    assert!(page.items[0].name.contains("Alpha"));
    assert_eq!(pool.lock_maps().live_count(), 1);
}

#[test]
fn race_park_revive_under_limit_stays_consistent() {
    use std::sync::atomic::AtomicBool;
    let pool = Arc::new(PluginWorkerPool::with_spawn(queue_spawn(vec![
        ScriptedWorker::ok("A", &["Alpha"]),
        ScriptedWorker::ok("B", &["Beta"]),
        ScriptedWorker::ok("A2", &["Alpha"]),
        ScriptedWorker::ok("B2", &["Beta"]),
        ScriptedWorker::ok("A3", &["Alpha"]),
        ScriptedWorker::ok("B3", &["Beta"]),
        ScriptedWorker::ok("A4", &["Alpha"]),
        ScriptedWorker::ok("B4", &["Beta"]),
    ])));
    pool.set_resident_worker_limit(None);
    pool.load_extension(Path::new("/a.cs3"), false).expect("a");
    pool.load_extension(Path::new("/b.cs3"), false).expect("b");
    pool.set_resident_worker_limit(Some(1));
    let stop = Arc::new(AtomicBool::new(false));
    let threads: Vec<_> = [0, 1]
        .into_iter()
        .map(|slot| {
            let pool = Arc::clone(&pool);
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let name = if slot == 0 { "Alpha" } else { "Beta" };
                    let _ = pool.search(name, "q", 1);
                }
            })
        })
        .collect();
    thread::sleep(std::time::Duration::from_millis(100));
    stop.store(true, Ordering::Relaxed);
    for t in threads {
        t.join().expect("thread");
    }
    assert!(pool.lock_maps().live_count() <= 1);
    pool.shutdown();
}

#[test]
fn trusted_load_asks_spawn_for_trusted_true() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_spawn = Arc::clone(&seen);
    let pool = PluginWorkerPool::with_spawn(Arc::new(move |trusted| {
        seen_spawn
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(trusted);
        let worker: Worker = ScriptedWorker::ok("T", &["TrustedProv"]);
        worker
    }));
    pool.load_extension(Path::new("/t.cs3"), true)
        .expect("trusted load");
    pool.load_extension(Path::new("/u.cs3"), false)
        .expect("untrusted load");
    assert_eq!(
        *seen.lock().unwrap_or_else(PoisonError::into_inner),
        vec![true, false]
    );
}

#[test]
fn race_trusted_and_untrusted_loads_interleave_without_panic() {
    let pool = Arc::new(PluginWorkerPool::with_spawn(Arc::new(|trusted| {
        let name = if trusted { "T" } else { "U" };
        let worker: Worker = ScriptedWorker::ok(name, &[name]);
        worker
    })));
    let barrier = Arc::new(Barrier::new(8));
    let mut handles = Vec::new();
    for slot in 0..8 {
        let pool = Arc::clone(&pool);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            for round in 0..30 {
                let path = PathBuf::from(format!("/ext/r-{slot}-{round}.cs3"));
                let trusted = round % 2 == 0;
                let _ = pool.load_extension(&path, trusted);
                let _ = pool.unload_extension(&path);
            }
        }));
    }
    for handle in handles {
        handle.join().expect("thread");
    }
    pool.shutdown();
}

#[test]
fn failed_load_does_not_leave_a_routed_worker() {
    let bad = Arc::new(ScriptedWorker {
        label: "bad".into(),
        providers: vec!["Bad".into()],
        search_hits: Mutex::new(0),
        extract_reply: Mutex::new(None),
        extract_calls: Mutex::new(0),
        repos: Mutex::new(Vec::new()),
        repos_fail: false,
        shutdowns: Arc::new(AtomicUsize::new(0)),
        load_fail: true,
    });
    let shutdowns = Arc::clone(&bad.shutdowns);
    let pool = PluginWorkerPool::with_spawn(queue_spawn(vec![bad]));
    assert!(pool.load_extension(Path::new("/bad.cs3"), false).is_err());
    assert!(pool.search("Bad", "q", 1).is_err());
    assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
}
