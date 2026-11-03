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
use crate::source_manager::CheckUpdate;
use crate::source_session::ActiveSources;

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
    let gateway = Arc::new(ScriptedSearch::default());
    *gateway.updates.lock().expect("lock") = VecDeque::from(script);
    let search = LinkSearch::start(
        Arc::clone(&gateway) as Arc<dyn ProviderGateway>,
        "P",
        "data",
    )
    .expect("search starts");
    let session = Arc::new(SourceSession::default());
    let generation = session.set(ActiveSources::new(
        Default::default(),
        Vec::new(),
        Vec::new(),
    ));
    let updates = Updates::default();
    let sink = updates.clone();
    let job = Discovery {
        checker: StreamChecker::new(Arc::new(VideoAt(working))),
        session,
        gate: Arc::new(CheckGate::new(Grace {
            high_quality: Duration::from_millis(10),
            other: Duration::from_millis(50),
            max_wait: Duration::from_secs(30),
        })),
        generation,
        queue: Arc::new(CheckQueue::default()),
        started: Instant::now(),
        tell: Arc::new(move |update| sink.0.lock().expect("lock").push(update)),
        give_subtitles: Arc::new(|_| {}),
        hosts: Mutex::default(),
        failures: FailureTally::default(),
    };
    (job, search, gateway, updates)
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

#[test]
fn subtitles_found_after_playback_started_reach_the_player() {
    let given = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&given);
    let first = update(vec![link("https://a/ok", 720)], false);
    let late = LinkSearchUpdate {
        subtitles: vec![SubtitleTrack {
            lang: "fr".to_owned(),
            ..SubtitleTrack::default()
        }],
        done: true,
        success: true,
        ..LinkSearchUpdate::default()
    };
    let (mut job, search, _, _) = job(vec![first, late], Vec::new());
    job.give_subtitles = Arc::new(move |subtitles| {
        sink.lock().expect("lock").extend(subtitles.to_vec());
    });
    job.session.mark_selected(0);
    job.run_search(search);
    assert_eq!(given.lock().expect("lock").len(), 1);
}

#[test]
fn every_failed_check_is_counted_by_reason_and_host() {
    let script = vec![update(
        vec![
            link("https://dead.test/a", 720),
            link("https://dead.test/b", 480),
            link("https://alive.test/c", 1080),
        ],
        true,
    )];
    let (job, search, _, _) = job(script, vec!["https://alive.test/c"]);
    job.run_search(search);
    job.run_checks();
    assert_eq!(job.failures.total(), 2);
    assert!(job.failures.hosts().starts_with("dead.test=2"));
    assert!(job.failures.reasons().contains("cannot download <url>"));
}

/// The viewer closes the video at every point of its search and checks: both threads must stop, the
/// search must be told to stop at most once, and nothing may bring the closed video's list back.
#[test]
fn closing_at_any_moment_stops_the_search_and_the_checks_and_never_revives_the_session() {
    for delay_micros in [0u64, 20, 100, 400, 1_500, 6_000, 25_000, 90_000] {
        let script = (0..6)
            .map(|n| update(vec![link(&format!("https://h{n}.test/v"), 720)], n == 5))
            .collect();
        let (job, search, gateway, _) = job(script, vec!["https://h1.test/v"]);
        let job = Arc::new(job);
        let (searching, checking) = (Arc::clone(&job), Arc::clone(&job));
        let handles = vec![
            std::thread::spawn(move || searching.run_search(search)),
            std::thread::spawn(move || checking.run_checks()),
        ];

        std::thread::sleep(Duration::from_micros(delay_micros));
        job.session.clear();
        job.gate.cancel();

        crate::race_tests::join_within(handles, crate::race_tests::TIME_LIMIT);
        assert!(
            job.session.snapshot().is_none(),
            "closing after {delay_micros} us: the search brought the closed video's list back"
        );
        assert!(
            *gateway.cancelled.lock().expect("lock") <= 1,
            "closing after {delay_micros} us: the search was stopped more than once"
        );
    }
}
