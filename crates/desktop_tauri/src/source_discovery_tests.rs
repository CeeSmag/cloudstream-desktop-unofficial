use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Mutex;

use core_model::{
    HomePage, LinkSearchCursor, LinkSearchUpdate, LoadDetails, MainPageEntry, PlayableLink,
    PlayableLinks, ProviderInfo, SearchPage,
};
use extension_api::{GatewayError, ProviderGateway};
use extension_repo::{HttpProbe, ProbeResponse, RepoError};

use super::*;
use crate::check_gate::{Grace, StartDecision};
use crate::failure_tally::FailureTally;
use crate::source_manager::{CheckUpdate, Tell};
use crate::source_session::ActiveSources;
use crate::subtitle_delivery::{Delivery, DeliveryParts, SubtitleWork};
use crate::test_subtitles::TestSubtitles;

/// A runtime whose search answers with scripted updates.
#[derive(Default)]
struct ScriptedSearch {
    updates: Mutex<VecDeque<LinkSearchUpdate>>,
    cancelled: Mutex<u32>,
}

fn unsupported<T>() -> Result<T, GatewayError> {
    Err(GatewayError::Unavailable("not scripted".to_owned()))
}

impl ProviderGateway for ScriptedSearch {
    fn load_extension(&self, _: &Path, _: bool) -> Result<Vec<ProviderInfo>, GatewayError> {
        unsupported()
    }
    fn search(&self, _: &str, _: &str, _: u32) -> Result<SearchPage, GatewayError> {
        unsupported()
    }
    fn main_page(
        &self,
        _: &str,
        _: u32,
        _: &MainPageEntry,
    ) -> Result<Option<HomePage>, GatewayError> {
        unsupported()
    }
    fn load(&self, _: &str, _: &str) -> Result<Option<LoadDetails>, GatewayError> {
        unsupported()
    }
    fn load_links(&self, _: &str, _: &str) -> Result<PlayableLinks, GatewayError> {
        unsupported()
    }
    fn extract(&self, _: &str, _: Option<&str>) -> Result<PlayableLinks, GatewayError> {
        unsupported()
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
        Ok(self
            .updates
            .lock()
            .expect("lock")
            .pop_front()
            // A script that runs out ends the search instead of leaving it waiting forever.
            .unwrap_or_else(|| update(Vec::new(), true)))
    }
    fn cancel_link_search(&self, _: &str) -> Result<(), GatewayError> {
        *self.cancelled.lock().expect("lock") += 1;
        Ok(())
    }
}

/// Answers like a video for the listed URLs and like an unreachable host for the rest.
struct VideoAt(Vec<&'static str>);

impl HttpProbe for VideoAt {
    fn probe(&self, url: &str, _: &[(String, String)]) -> Result<ProbeResponse, RepoError> {
        if self.0.contains(&url) {
            return Ok(ProbeResponse {
                status: 206,
                content_type: "video/mp4".to_owned(),
                head: b"\0\0\0\x18ftypmp42".to_vec(),
                total_bytes: Some(1_000_000),
            });
        }
        Err(RepoError::Download {
            url: url.to_owned(),
            reason: "unreachable".to_owned(),
        })
    }
}

fn link(url: &str, quality: i32) -> PlayableLink {
    PlayableLink {
        url: url.to_owned(),
        quality,
        link_type: "VIDEO".to_owned(),
        headers: HashMap::new(),
        ..PlayableLink::default()
    }
}

fn update(links: Vec<PlayableLink>, done: bool) -> LinkSearchUpdate {
    LinkSearchUpdate {
        links,
        done,
        success: true,
        ..LinkSearchUpdate::default()
    }
}

#[derive(Clone, Default)]
struct Updates(Arc<Mutex<Vec<CheckUpdate>>>);

/// A job whose search plays `script`; the probe says `working` URLs are videos.
fn job(
    script: Vec<LinkSearchUpdate>,
    working: Vec<&'static str>,
) -> (Discovery, LinkSearch, Arc<ScriptedSearch>, Updates) {
    let (job, search, gateway, updates, _) = job_with_subtitles(script, working, Arc::new(|_| {}));
    (job, search, gateway, updates)
}

/// A session with one video opened, and that video's generation.
fn fresh_session() -> (Arc<SourceSession>, u64) {
    let session = Arc::new(SourceSession::default());
    let generation = session.set(ActiveSources::new(
        Default::default(),
        Vec::new(),
        Vec::new(),
    ));
    (session, generation)
}

/// A gate that decides quickly and waits a long time at most.
fn quick_gate() -> Arc<CheckGate> {
    Arc::new(CheckGate::new(Grace {
        high_quality: Duration::from_millis(10),
        other: Duration::from_millis(50),
        max_wait: Duration::from_secs(30),
    }))
}

/// A search that plays `script`, and the provider that plays it.
fn scripted_search(script: Vec<LinkSearchUpdate>) -> (LinkSearch, Arc<ScriptedSearch>) {
    let gateway = Arc::new(ScriptedSearch::default());
    *gateway.updates.lock().expect("lock") = VecDeque::from(script);
    let search = LinkSearch::start(
        Arc::clone(&gateway) as Arc<dyn ProviderGateway>,
        "P",
        "data",
    )
    .expect("search starts");
    (search, gateway)
}

/// Like [`job`], with `give_subtitles` as where subtitles that become ready go; also returns the
/// fake subtitle host.
fn job_with_subtitles(
    script: Vec<LinkSearchUpdate>,
    working: Vec<&'static str>,
    give_subtitles: SubtitleSink,
) -> (
    Discovery,
    LinkSearch,
    Arc<ScriptedSearch>,
    Updates,
    TestSubtitles,
) {
    let (search, gateway) = scripted_search(script);
    let (session, generation) = fresh_session();
    let updates = Updates::default();
    let sink = updates.clone();
    let tell: Tell = Arc::new(move |update| sink.0.lock().expect("lock").push(update));
    let gate = quick_gate();
    let subtitles = TestSubtitles::new();
    let delivery = Delivery::new(DeliveryParts {
        session: Arc::clone(&session),
        generation,
        loader: Arc::clone(&subtitles.loader),
        work: SubtitleWork {
            title_key: "P
data"
                .to_owned(),
            rank_of: Arc::new(|_| 0),
        },
        tell: Arc::clone(&tell),
        give_subtitles,
    });
    let (wanted_session, wanted_gate) = (Arc::clone(&session), Arc::clone(&gate));
    let (prefetch, _threads) = delivery.start_downloads(Arc::new(move || {
        wanted_session.is_current(generation) && !wanted_gate.is_cancelled()
    }));
    let job = Discovery {
        checker: StreamChecker::new(Arc::new(VideoAt(working))),
        session,
        gate,
        generation,
        queue: Arc::new(CheckQueue::default()),
        started: Instant::now(),
        tell,
        prefetch,
        hosts: Mutex::default(),
        failures: FailureTally::default(),
    };
    (job, search, gateway, updates, subtitles)
}

#[test]
fn streams_are_checked_as_they_arrive_and_the_best_working_one_is_chosen() {
    let script = vec![
        update(vec![link("https://a/dead", 1080)], false),
        update(
            vec![link("https://b/ok", 720), link("https://c/ok", 480)],
            true,
        ),
    ];
    let (job, search, _, updates) = job(script, vec!["https://b/ok", "https://c/ok"]);
    job.run_search(search);
    job.run_checks();
    assert_eq!(job.gate.wait_for_start(), StartDecision::Start(1));
    let active = job.session.snapshot().expect("open");
    assert_eq!(active.links.len(), 3);
    assert_eq!(
        active.health,
        [
            SourceHealth::Broken,
            SourceHealth::Working,
            SourceHealth::Working
        ]
    );
    assert_eq!(active.sizes[1], Some(1_000_000));
    let last = updates
        .0
        .lock()
        .expect("lock")
        .iter()
        .rev()
        .find_map(|u| match u {
            CheckUpdate::Progress(p) => Some(*p),
            CheckUpdate::SourcesChanged => None,
        });
    let last = last.expect("progress was reported");
    assert_eq!((last.checked, last.working, last.total), (3, 2, 3));
    assert!(!last.discovering);
}

#[test]
fn a_search_that_finds_nothing_fails_the_wait() {
    let (job, search, _, _) = job(vec![update(Vec::new(), true); 3], Vec::new());
    job.run_search(search);
    job.run_checks();
    assert!(matches!(
        job.gate.wait_for_start(),
        StartDecision::Failed(_)
    ));
}

#[test]
fn closing_the_video_stops_the_search_in_the_runtime_and_the_checks() {
    let (job, search, gateway, _) = job(
        vec![update(vec![link("https://a/x", 720)], false)],
        Vec::new(),
    );
    job.session.clear();
    job.run_search(search);
    job.run_checks();
    assert_eq!(*gateway.cancelled.lock().expect("lock"), 1);
    assert_eq!(job.gate.progress().checked, 0, "nothing was asked");
    job.gate.cancel();
    assert_eq!(job.gate.wait_for_start(), StartDecision::Cancelled);
}

#[path = "source_discovery_outcome_tests.rs"]
mod outcome;
