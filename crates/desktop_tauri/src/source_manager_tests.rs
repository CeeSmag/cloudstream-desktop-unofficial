use std::collections::HashMap;

use extension_repo::{HttpProbe, ProbeResponse, RepoError};

use super::*;

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

fn sources(urls: &[&str]) -> ActiveSources {
    let links = urls
        .iter()
        .map(|url| PlayableLink {
            url: (*url).to_owned(),
            link_type: "VIDEO".to_owned(),
            headers: HashMap::new(),
            ..PlayableLink::default()
        })
        .collect();
    ActiveSources::new(Default::default(), links, Vec::new())
}

/// A job over `urls` whose updates are collected in the returned list.
fn job(
    working: Vec<&'static str>,
    urls: &[&str],
) -> (BackgroundChecks, Vec<(usize, PlayableLink)>, Updates) {
    let session = Arc::new(SourceSession::default());
    let active = sources(urls);
    let links: Vec<(usize, PlayableLink)> = active.links.iter().cloned().enumerate().collect();
    let generation = session.set(active);
    let updates = Updates::default();
    let sink = updates.clone();
    let job = BackgroundChecks {
        checker: StreamChecker::new(Arc::new(VideoAt(working))),
        session,
        gate: Arc::new(CheckGate::default()),
        generation,
        started: Instant::now(),
        tell: Box::new(move |update| sink.push(update)),
    };
    (job, links, updates)
}

#[derive(Clone, Default)]
struct Updates(Arc<Mutex<Vec<CheckUpdate>>>);

impl Updates {
    fn push(&self, update: CheckUpdate) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(update);
    }

    fn all(&self) -> Vec<CheckUpdate> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[test]
fn every_stream_is_checked_and_the_best_working_one_is_chosen() {
    let urls = ["https://a/dead", "https://b/ok", "https://c/ok"];
    let (job, links, updates) = job(vec!["https://b/ok", "https://c/ok"], &urls);
    job.run(&links);
    assert_eq!(job.gate.wait_for_start(), Some(1));
    let sizes = job.session.snapshot().map(|active| active.sizes);
    assert_eq!(sizes, Some(vec![None, Some(1_000_000), Some(1_000_000)]));
    let health = job.session.snapshot().map(|active| active.health);
    assert_eq!(
        health,
        Some(vec![
            SourceHealth::Broken,
            SourceHealth::Working,
            SourceHealth::Working
        ])
    );
    let last_progress = updates.all().into_iter().rev().find_map(|u| match u {
        CheckUpdate::Progress(p) => Some(p),
        CheckUpdate::SourcesChanged => None,
    });
    assert_eq!(last_progress.map(|p| (p.checked, p.working)), Some((3, 2)));
}

#[test]
fn the_picker_is_told_when_the_checks_finish() {
    let (job, links, updates) = job(Vec::new(), &["https://a/dead"]);
    job.run(&links);
    assert_eq!(updates.all().last(), Some(&CheckUpdate::SourcesChanged));
}

#[test]
fn closing_the_video_stops_the_checks_and_ends_the_wait() {
    let urls: Vec<String> = (0..60).map(|n| format!("https://h/{n}")).collect();
    let refs: Vec<&str> = urls.iter().map(String::as_str).collect();
    let (job, links, _) = job(Vec::new(), &refs);
    job.session.clear();
    job.run(&links);
    assert_eq!(job.gate.progress().checked, 0, "nothing was asked");
    job.gate.cancel();
    assert_eq!(job.gate.wait_for_start(), None);
}

#[test]
fn beginning_a_new_video_cancels_the_previous_one() {
    let manager = SourceManager::new(StreamChecker::new(Arc::new(VideoAt(Vec::new()))));
    let first = manager.begin();
    let second = manager.begin();
    assert!(first.is_cancelled() && !second.is_cancelled());
    manager.close();
    assert!(second.is_cancelled());
    assert!(manager.session().snapshot().is_none());
}
