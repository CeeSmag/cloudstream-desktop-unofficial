//! A plugin with a call in flight is never parked; the live count never passes the limit when
//! nothing is in flight (plan step 1d, RULES.md section 9 race tests).

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use core_model::{
    DiscoveredRepository, HomePage, LinkSearchCursor, LinkSearchUpdate, LoadDetails, MainPageEntry,
    PlayableLinks, ProviderInfo, SearchPage,
};
use extension_api::{GatewayError, ProviderGateway};

use crate::plugin_worker_maps::Worker;
use crate::PluginWorkerPool;

/// Holds calls inside `search` until opened, and reports when one has entered.
#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    opened: Condvar,
    entered: AtomicBool,
}

impl Gate {
    fn wait_open(&self) {
        self.entered.store(true, Ordering::SeqCst);
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        while !*open {
            open = self
                .opened
                .wait(open)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn open(&self) {
        *self.open.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.opened.notify_all();
    }

    fn wait_until_entered(&self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.entered.load(Ordering::SeqCst) {
            assert!(Instant::now() < deadline, "no call entered the gate");
            thread::yield_now();
        }
    }
}

/// Counts live workers (spawned and not yet shut down) and the most there ever were.
#[derive(Default)]
struct Alive {
    now: AtomicUsize,
    peak: AtomicUsize,
}

/// A worker that serves whatever plugin it is asked to load (the provider is the file's stem) and
/// holds searches of `gated` until the gate opens. A search that finds the worker shut down fails.
struct TestWorker {
    provider: Mutex<String>,
    gated: String,
    gate: Arc<Gate>,
    alive: Arc<Alive>,
    closed: AtomicBool,
}

impl ProviderGateway for TestWorker {
    fn load_extension(&self, path: &Path, _: bool) -> Result<Vec<ProviderInfo>, GatewayError> {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        *self.provider.lock().unwrap_or_else(PoisonError::into_inner) = name.clone();
        Ok(vec![ProviderInfo {
            name,
            ..ProviderInfo::default()
        }])
    }

    fn unload_extension(&self, _: &Path) -> Result<(), GatewayError> {
        Ok(())
    }

    fn search(&self, provider: &str, _: &str, _: u32) -> Result<SearchPage, GatewayError> {
        if provider == self.gated {
            self.gate.wait_open();
        }
        thread::yield_now();
        if self.closed.load(Ordering::SeqCst) {
            return Err(GatewayError::Transport("the worker was closing".into()));
        }
        Ok(SearchPage::default())
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

    fn start_link_search(&self, _: &str, _: &str) -> Result<String, GatewayError> {
        Ok("search".to_owned())
    }

    fn poll_link_search(
        &self,
        _: &str,
        _: LinkSearchCursor,
        _: Duration,
    ) -> Result<LinkSearchUpdate, GatewayError> {
        Ok(LinkSearchUpdate::default())
    }

    fn cancel_link_search(&self, _: &str) -> Result<(), GatewayError> {
        Ok(())
    }

    fn extract(&self, _: &str, _: Option<&str>) -> Result<PlayableLinks, GatewayError> {
        Ok(PlayableLinks::default())
    }

    fn take_added_repositories(&self) -> Result<Vec<DiscoveredRepository>, GatewayError> {
        Ok(Vec::new())
    }

    fn shutdown(&self) {
        if !self.closed.swap(true, Ordering::SeqCst) {
            self.alive.now.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

struct Rig {
    pool: Arc<PluginWorkerPool>,
    gate: Arc<Gate>,
    alive: Arc<Alive>,
}

/// A pool of `TestWorker`s limited to `limit` resident plugins; searches of `gated` wait on the gate.
fn rig(limit: usize, gated: &str) -> Rig {
    let gate = Arc::new(Gate::default());
    let alive = Arc::new(Alive::default());
    let (gate_for_spawn, alive_for_spawn, gated_for_spawn) =
        (Arc::clone(&gate), Arc::clone(&alive), gated.to_owned());
    let pool = Arc::new(PluginWorkerPool::with_spawn(Arc::new(move |_trusted| {
        let now = alive_for_spawn.now.fetch_add(1, Ordering::SeqCst) + 1;
        alive_for_spawn.peak.fetch_max(now, Ordering::SeqCst);
        let worker: Worker = Arc::new(TestWorker {
            provider: Mutex::new(String::new()),
            gated: gated_for_spawn.clone(),
            gate: Arc::clone(&gate_for_spawn),
            alive: Arc::clone(&alive_for_spawn),
            closed: AtomicBool::new(false),
        });
        worker
    })));
    pool.set_resident_worker_limit(Some(limit));
    Rig { pool, gate, alive }
}

fn load(pool: &PluginWorkerPool, name: &str) {
    pool.load_extension(Path::new(&format!("/ext/{name}.cs3")), false)
        .expect("load");
}

#[test]
fn a_plugin_with_a_call_in_flight_is_not_parked_by_another_plugins_revive() {
    let rig = rig(1, "Alpha");
    load(&rig.pool, "Alpha");
    load(&rig.pool, "Beta"); // parks Alpha
    let slow = {
        let pool = Arc::clone(&rig.pool);
        thread::spawn(move || pool.search("Alpha", "q", 1)) // revives Alpha, then waits in the gate
    };
    rig.gate.wait_until_entered();
    // Beta is revived while Alpha's call is still running: it must not be Alpha that is parked.
    rig.pool.search("Beta", "q", 1).expect("Beta answers");
    rig.gate.open();
    let outcome = slow.join().expect("thread");
    assert!(outcome.is_ok(), "Alpha's call was cut short: {outcome:?}");
}

#[test]
fn the_excess_worker_is_parked_once_the_call_that_protected_it_ends() {
    let rig = rig(1, "Alpha");
    load(&rig.pool, "Alpha");
    load(&rig.pool, "Beta");
    let slow = {
        let pool = Arc::clone(&rig.pool);
        thread::spawn(move || pool.search("Alpha", "q", 1))
    };
    rig.gate.wait_until_entered();
    rig.pool.search("Beta", "q", 1).expect("Beta answers");
    rig.gate.open();
    slow.join().expect("thread").expect("Alpha answers");
    assert!(
        rig.alive.now.load(Ordering::SeqCst) <= 1,
        "two plugin processes stayed alive with a limit of one"
    );
}

#[test]
fn the_live_count_never_passes_the_limit_while_nothing_is_in_flight() {
    let rig = rig(1, "none");
    for name in ["Alpha", "Beta", "Gamma"] {
        load(&rig.pool, name);
    }
    for _ in 0..6 {
        for name in ["Alpha", "Beta", "Gamma"] {
            rig.pool.search(name, "q", 1).expect("answers");
        }
    }
    assert_eq!(
        rig.alive.peak.load(Ordering::SeqCst),
        1,
        "a plugin was started before the oldest was stopped"
    );
}

/// Four threads search three plugins in turn, with a limit of one; returns how many calls failed.
fn run_stress(rig: &Rig, names: [&'static str; 3]) -> usize {
    let failures = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicUsize::new(0));
    let threads: Vec<_> = (0..4usize)
        .map(|index| {
            let pool = Arc::clone(&rig.pool);
            let (failures, done) = (Arc::clone(&failures), Arc::clone(&done));
            thread::spawn(move || {
                for round in 0..300usize {
                    let name = names[(index + round) % names.len()];
                    if pool.search(name, "q", 1).is_err() {
                        failures.fetch_add(1, Ordering::SeqCst);
                    }
                }
                done.fetch_add(1, Ordering::SeqCst);
            })
        })
        .collect();
    let deadline = Instant::now() + Duration::from_secs(20);
    while done.load(Ordering::SeqCst) < threads.len() {
        assert!(
            Instant::now() < deadline,
            "the threads did not finish: a deadlock"
        );
        thread::sleep(Duration::from_millis(5));
    }
    for handle in threads {
        handle.join().expect("thread");
    }
    failures.load(Ordering::SeqCst)
}

#[test]
fn stress_no_call_fails_and_the_limit_holds_once_quiet() {
    let rig = rig(1, "none");
    let names = ["Alpha", "Beta", "Gamma"];
    for name in names {
        load(&rig.pool, name);
    }
    let failures = run_stress(&rig, names);
    assert_eq!(
        failures, 0,
        "calls failed while plugins were parked and revived"
    );
    assert!(
        rig.alive.now.load(Ordering::SeqCst) <= 1,
        "the limit holds once nothing is in flight"
    );
}
