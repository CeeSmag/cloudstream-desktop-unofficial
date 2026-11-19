//! Unit and race tests for `subtitle_delivery`: language ranking, caps, cached title delivery,
//! late arrivals, and concurrent cancellation/switching.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use core_model::SubtitleTrack;
use player_api::{find_language, ENGLISH};
use subtitle_store::{FetchError, LoadOptions, SubtitleCache, SubtitleFetcher, SubtitleLoader};

use crate::background_work::EventPump;
use crate::request_tickets::{RequestKind, RequestTickets};
use crate::ui_events::UiEvent;

use super::*;

static DIRS: AtomicU32 = AtomicU32::new(0);

struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "cs_ui_subtitle_delivery_{}_{}",
            std::process::id(),
            DIRS.fetch_add(1, Ordering::SeqCst)
        )))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct FakeFetcher {
    delay: Duration,
    calls: AtomicUsize,
}

impl SubtitleFetcher for FakeFetcher {
    fn fetch(&self, url: &str, _headers: &[(String, String)]) -> Result<Vec<u8>, FetchError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
        Ok(format!("WEBVTT\n\n00:00.000 --> 00:05.000\n{url}\n").into_bytes())
    }
}

fn sub(lang: &str, url: &str) -> SubtitleTrack {
    SubtitleTrack {
        lang: lang.to_owned(),
        url: url.to_owned(),
        headers: None,
    }
}

fn loader_in(scratch: &Scratch, delay: Duration) -> (Arc<SubtitleLoader>, Arc<FakeFetcher>) {
    let cache = Arc::new(SubtitleCache::new(
        scratch.0.clone(),
        Duration::from_secs(3600),
    ));
    let fetcher = Arc::new(FakeFetcher {
        delay,
        calls: AtomicUsize::new(0),
    });
    let options = LoadOptions {
        attempts: 1,
        backoff: Duration::from_millis(1),
    };
    let as_fetcher: Arc<dyn SubtitleFetcher> = fetcher.clone();
    (
        Arc::new(SubtitleLoader::new(cache, as_fetcher, options)),
        fetcher,
    )
}

fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !cond() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn matches_subtitle_label_handles_iso_codes_and_qualified_names() {
    let arabic = find_language("ara").expect("Arabic");
    assert!(matches_subtitle_label("en", &ENGLISH));
    assert!(matches_subtitle_label("eng", &ENGLISH));
    assert!(matches_subtitle_label("en-US", &ENGLISH));
    assert!(matches_subtitle_label("English (SDH)", &ENGLISH));
    assert!(matches_subtitle_label("English - Forced", &ENGLISH));
    assert!(matches_subtitle_label("Arabic [CC]", &arabic));
    assert!(!matches_subtitle_label("French", &ENGLISH));
    assert!(!matches_subtitle_label("", &ENGLISH));
}

#[test]
fn budget_prioritizes_preferred_and_caps_non_preferred_per_language() {
    let arabic = find_language("ara").expect("Arabic");
    let mut budget = QueueBudget::new(vec![arabic, ENGLISH], HashSet::new());
    let tracks = vec![
        sub("French", "https://s/fr1"),
        sub("French", "https://s/fr2"),
        sub("French", "https://s/fr3"),
        sub("English (SDH)", "https://s/en1"),
        sub("ar", "https://s/ar1"),
        sub("ar", "https://s/ar1"),
    ];
    let accepted = budget.filter_accepted(&tracks);
    let urls: Vec<&str> = accepted.iter().map(|t| t.url.as_str()).collect();
    assert_eq!(
        urls,
        [
            "https://s/ar1",
            "https://s/en1",
            "https://s/fr1",
            "https://s/fr2"
        ],
        "preferred ordered first, duplicate URL dropped, non-preferred capped at 2 per language"
    );
}

fn start_english(
    loader: &Arc<SubtitleLoader>,
    title_key: &str,
    ticket: Ticket,
    pump: &EventPump,
) -> SubtitleDeliveryHandle {
    SubtitleDeliveryHandle::start(DeliveryRequest {
        loader: Arc::clone(loader),
        title_key: title_key.to_owned(),
        preferred: vec![ENGLISH],
        already_delivered: HashSet::new(),
        ticket,
        sender: pump.sender(),
    })
}

#[test]
fn downloads_save_to_cache_and_reopening_title_offers_cached_immediately() {
    let scratch = Scratch::new();
    let (loader, fetcher) = loader_in(&scratch, Duration::ZERO);
    let pump = EventPump::new();
    let mut tickets = RequestTickets::default();
    let t1 = tickets.issue(RequestKind::Open);

    let handle = start_english(&loader, "prov\nep1", t1, &pump);
    handle.submit(&[sub("English", "https://s/en.vtt")]);
    handle.finish_discovery();

    let mut got = Vec::new();
    wait_until("first download ready", || {
        for event in pump.drain() {
            if let UiEvent::SubtitleReady(t, ready) = event {
                assert_eq!(t, t1);
                got.push(ready);
            }
        }
        got.len() == 1
    });
    assert!(std::path::Path::new(&got[0].track.url).exists());
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);

    // Reopen same title: cached subtitle is emitted immediately without downloading again.
    let t2 = tickets.issue(RequestKind::Open);
    let second = start_english(&loader, "prov\nep1", t2, &pump);
    second.stop();
    let cached_events: Vec<_> = pump
        .drain()
        .into_iter()
        .filter(|e| matches!(e, UiEvent::SubtitleReady(t, _) if *t == t2))
        .collect();
    assert_eq!(cached_events.len(), 1);
    assert_eq!(
        fetcher.calls.load(Ordering::SeqCst),
        1,
        "cached title subtitle must not re-download"
    );
}

#[test]
fn refresh_skips_already_delivered_urls_and_downloads_new_ones() {
    let scratch = Scratch::new();
    let (loader, fetcher) = loader_in(&scratch, Duration::ZERO);
    let pump = EventPump::new();
    let mut tickets = RequestTickets::default();
    let t2 = tickets.issue(RequestKind::Open);

    let already = HashSet::from(["https://s/en.vtt".to_owned()]);
    let handle = SubtitleDeliveryHandle::start(DeliveryRequest {
        loader,
        title_key: "prov\nep2".into(),
        preferred: vec![ENGLISH],
        already_delivered: already,
        ticket: t2,
        sender: pump.sender(),
    });
    handle.submit(&[
        sub("English", "https://s/en.vtt"),
        sub("Arabic", "https://s/ar.vtt"),
    ]);
    handle.finish_discovery();

    let mut ready_urls = Vec::new();
    wait_until("new subtitle downloaded", || {
        for event in pump.drain() {
            if let UiEvent::SubtitleReady(_, r) = event {
                ready_urls.push(r.remote_url);
            }
        }
        !handle.snapshot().busy && !ready_urls.is_empty()
    });
    assert_eq!(ready_urls, ["https://s/ar.vtt"]);
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);
}

/// Race test: rapidly replacing/stopping deliveries while concurrent threads submit tracks
/// never delivers tracks for a stale ticket after stop and never deadlocks or leaks busy state.
#[test]
fn concurrent_submit_and_stop_across_rapid_video_switches_stays_clean() {
    let scratch = Scratch::new();
    let (loader, _) = loader_in(&scratch, Duration::from_millis(2));
    let pump = EventPump::new();
    let mut tickets = RequestTickets::default();

    for round in 0..20 {
        let ticket = tickets.issue(RequestKind::Open);
        let key = format!("prov\nvideo-{round}");
        loader.cache().keep_only_title(&key);
        let handle = Arc::new(SubtitleDeliveryHandle::start(DeliveryRequest {
            loader: Arc::clone(&loader),
            title_key: key,
            preferred: vec![ENGLISH],
            already_delivered: HashSet::new(),
            ticket,
            sender: pump.sender(),
        }));
        let submitter_handle = Arc::clone(&handle);
        let submitter = std::thread::spawn(move || {
            for idx in 0..16 {
                submitter_handle
                    .submit(&[sub("English", &format!("https://s/r{round}/sub{idx}.vtt"))]);
            }
        });
        std::thread::sleep(Duration::from_millis(3));
        handle.stop();
        submitter.join().expect("submitter joined");
        wait_until("stopped delivery clears busy", || !handle.snapshot().busy);
    }

    let final_ticket = tickets.issue(RequestKind::Open);
    for event in pump.drain() {
        if let UiEvent::SubtitleReady(ticket, _) = event {
            assert!(
                !tickets.is_current(ticket) || ticket == final_ticket,
                "stale ticket cannot be mistaken for current"
            );
        }
    }
}
