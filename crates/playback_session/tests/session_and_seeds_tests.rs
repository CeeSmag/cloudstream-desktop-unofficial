//! Integration and race tests for `playback_session`: `run_open`, `refresh_sources`,
//! `check_and_pick`, fresh/stale `TitleCache` seeds, `OpenControl`, and `LiveSources`.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::too_many_lines)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use app_services::StreamResolver;
use core_model::{
    HomePage, LinkSearchCursor, LinkSearchUpdate, LoadDetails, MainPageEntry, PlayableLink,
    PlayableLinks, ProviderInfo, SearchPage, SubtitleTrack,
};
use extension_api::{GatewayError, ProviderGateway};
use playback_session::{
    cached_title_from_open, check_and_pick, refresh_sources, run_open, save_open_kept, title_key,
    CheckedOpen, LiveSources, OpenControl, OpenRequest, RefreshRequest,
};
use title_cache::{CacheLimits, CachedLink, CachedTitle, Clock, LinkHealth, TitleCache};

fn fixed_clock(now: Arc<Mutex<u64>>) -> Clock {
    Arc::new(move || UNIX_EPOCH + Duration::from_secs(*now.lock().expect("clock lock")))
}

struct MockHttpServer {
    base_url: String,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl MockHttpServer {
    fn start(routes: Vec<(String, u16, String, String)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock http");
        let addr = listener.local_addr().expect("local addr");
        listener.set_nonblocking(true).expect("nonblocking");
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);

        let handle = thread::spawn(move || {
            while !stop_flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // Windows: accepted sockets inherit the listener's non-blocking mode.
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                        let mut buf = [0u8; 2048];
                        let n = stream.read(&mut buf).unwrap_or(0);
                        let req = String::from_utf8_lossy(&buf[..n]);
                        let path = req
                            .lines()
                            .next()
                            .and_then(|l| l.split_whitespace().nth(1))
                            .unwrap_or("/");
                        let matched = routes.iter().find(|(p, _, _, _)| p == path);
                        let (status, ct, body) = match matched {
                            Some((_, s, c, b)) => (*s, c.as_str(), b.as_str()),
                            None => (404, "text/plain", "not found"),
                        };
                        let reason = if status == 200 { "OK" } else { "Not Found" };
                        let resp = format!(
                            "HTTP/1.1 {status} {reason}\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        let _ = stream.write_all(resp.as_bytes());
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            base_url: format!("http://{addr}"),
            stop,
            handle: Some(handle),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

impl Drop for MockHttpServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

struct ScriptedGateway {
    batches: Mutex<Vec<PlayableLinks>>,
    calls: AtomicUsize,
}

impl ScriptedGateway {
    fn single(links: Vec<PlayableLink>, subtitles: Vec<SubtitleTrack>) -> Self {
        Self {
            batches: Mutex::new(vec![PlayableLinks {
                success: true,
                links,
                subtitles,
            }]),
            calls: AtomicUsize::new(0),
        }
    }
}

impl ProviderGateway for ScriptedGateway {
    fn load_extension(&self, _: &Path, _: bool) -> Result<Vec<ProviderInfo>, GatewayError> {
        Ok(Vec::new())
    }
    fn search(&self, _: &str, _: &str, _: u32) -> Result<SearchPage, GatewayError> {
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
        self.calls.fetch_add(1, Ordering::SeqCst);
        let guard = self.batches.lock().expect("batches");
        Ok(guard.first().cloned().unwrap_or_default())
    }
    fn start_link_search(&self, _: &str, _: &str) -> Result<String, GatewayError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok("search-1".into())
    }
    fn poll_link_search(
        &self,
        _: &str,
        cursor: LinkSearchCursor,
        _: Duration,
    ) -> Result<LinkSearchUpdate, GatewayError> {
        let batch = self
            .batches
            .lock()
            .expect("batches")
            .first()
            .cloned()
            .unwrap_or_default();
        let links = batch.links.get(cursor.links..).unwrap_or(&[]).to_vec();
        let subtitles = batch
            .subtitles
            .get(cursor.subtitles..)
            .unwrap_or(&[])
            .to_vec();
        Ok(LinkSearchUpdate {
            next_link: batch.links.len(),
            next_subtitle: batch.subtitles.len(),
            links,
            subtitles,
            done: true,
            success: true,
            failure: None,
        })
    }
    fn extract(&self, _: &str, _: Option<&str>) -> Result<PlayableLinks, GatewayError> {
        Ok(PlayableLinks::default())
    }
}

fn temp_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("cs_pb_sess_{tag}_{}_{}", std::process::id(), nanos));
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

fn make_link(name: &str, url: String, quality: i32, kind: &str) -> PlayableLink {
    PlayableLink {
        source: "Demo".into(),
        name: name.into(),
        url,
        quality,
        link_type: kind.into(),
        ..PlayableLink::default()
    }
}

#[test]
fn run_open_falls_back_from_broken_high_quality_to_working_stream() {
    let server = MockHttpServer::start(vec![
        (
            "/broken.mp4".into(),
            404,
            "text/plain".into(),
            "gone".into(),
        ),
        (
            "/good.mp4".into(),
            200,
            "video/mp4".into(),
            "00000000ftypisom".into(),
        ),
    ]);
    let gw = Arc::new(ScriptedGateway::single(
        vec![
            make_link("Broken 1080p", server.url("/broken.mp4"), 1080, "VIDEO"),
            make_link("Good 720p", server.url("/good.mp4"), 720, "VIDEO"),
        ],
        vec![SubtitleTrack {
            lang: "English".into(),
            url: server.url("/en.vtt"),
            headers: None,
        }],
    ));
    let resolver = StreamResolver::new(gw);
    let control = OpenControl::default();
    let ready_idx = Mutex::new(None);
    let updates = AtomicUsize::new(0);

    let opened = run_open(OpenRequest {
        streams: &resolver,
        provider: "Demo",
        data: "ep1",
        media_key: "ep1",
        cache: None,
        on_progress: &|_| {},
        on_update: &|_| {
            updates.fetch_add(1, Ordering::SeqCst);
        },
        on_ready: &|idx| {
            *ready_idx.lock().expect("ready") = Some(idx);
        },
        control: &control,
    })
    .expect("run_open succeeds");

    let play_idx = opened.play_index.expect("picked working link");
    assert_eq!(opened.links[play_idx].0.url, server.url("/good.mp4"));
    assert_eq!(*ready_idx.lock().expect("ready"), Some(play_idx));
    assert!(updates.load(Ordering::SeqCst) >= 1);
    assert_eq!(opened.subtitles.len(), 1);
}

#[test]
fn run_open_uses_fresh_and_stale_title_cache_seeds() {
    let server = MockHttpServer::start(vec![
        (
            "/seed.mp4".into(),
            200,
            "video/mp4".into(),
            "00000000ftypisom".into(),
        ),
        (
            "/fresh.mp4".into(),
            200,
            "video/mp4".into(),
            "00000000ftypisom".into(),
        ),
    ]);
    let dir = temp_dir("seeds");
    let now = Arc::new(Mutex::new(10_000u64));
    let cache = Arc::new(TitleCache::with_clock(
        dir.clone(),
        CacheLimits::default(),
        fixed_clock(Arc::clone(&now)),
    ));
    let key = title_key("Demo", "movie1");
    let seed_link = make_link("Cached 720p", server.url("/seed.mp4"), 720, "VIDEO");
    let kept = cache.keep_only(&key);
    save_open_kept(
        &cache,
        &kept,
        &CheckedOpen {
            links: vec![(
                seed_link.clone(),
                Some(app_services::LinkHealth::Working {
                    size_bytes: Some(16),
                    subtitles: Vec::new(),
                }),
            )],
            play_index: Some(0),
            subtitles: Vec::new(),
        },
    );

    // 1. Fresh seed: announced ready immediately while search still runs.
    let gw = Arc::new(ScriptedGateway::single(
        vec![make_link(
            "Live 1080p",
            server.url("/fresh.mp4"),
            1080,
            "VIDEO",
        )],
        Vec::new(),
    ));
    let resolver = StreamResolver::new(gw);
    let control = OpenControl::default();
    let ready_seen = AtomicUsize::new(0);
    let fresh_open = run_open(OpenRequest {
        streams: &resolver,
        provider: "Demo",
        data: "movie1",
        media_key: "movie1",
        cache: Some(&cache),
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &|_| {
            ready_seen.fetch_add(1, Ordering::SeqCst);
        },
        control: &control,
    })
    .expect("fresh seeded open succeeds");
    assert_eq!(ready_seen.load(Ordering::SeqCst), 1);
    assert!(fresh_open.play_index.is_some());

    // 2. Stale seed (advance clock by 2 hours, within 24h keep_for but past 20m fresh_for).
    *now.lock().expect("clock") += 7_200;
    let stale_open = run_open(OpenRequest {
        streams: &resolver,
        provider: "Demo",
        data: "movie1",
        media_key: "movie1",
        cache: Some(&cache),
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &|_| {},
        control: &control,
    })
    .expect("stale seeded open succeeds");
    assert!(stale_open.play_index.is_some());

    // Also test cached_title_from_open helper directly.
    let cached = cached_title_from_open(&stale_open);
    assert!(!cached.links.is_empty());
    assert!(cached.last_good.is_some());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn refresh_sources_pins_currently_playing_link_and_forgets_old_cache() {
    let server = MockHttpServer::start(vec![(
        "/new.mp4".into(),
        200,
        "video/mp4".into(),
        "00000000ftypisom".into(),
    )]);
    let dir = temp_dir("refresh");
    let cache = Arc::new(TitleCache::new(dir.clone(), CacheLimits::default()));
    let key = title_key("Demo", "ep2");
    cache
        .save(
            &key,
            &CachedTitle {
                complete: true,
                links: vec![CachedLink {
                    link: make_link("Old", server.url("/old.mp4"), 360, "VIDEO"),
                    health: LinkHealth::Working,
                }],
                last_good: None,
            },
        )
        .expect("seed cache");

    let playing = make_link("Current 720p", server.url("/playing.mp4"), 720, "VIDEO");
    let gw = Arc::new(ScriptedGateway::single(
        vec![make_link(
            "Fresh 1080p",
            server.url("/new.mp4"),
            1080,
            "VIDEO",
        )],
        Vec::new(),
    ));
    let resolver = StreamResolver::new(gw);
    let live = LiveSources::new();
    let control = OpenControl::default();

    let refreshed = refresh_sources(RefreshRequest {
        streams: &resolver,
        provider: "Demo",
        data: "ep2",
        media_key: "ep2",
        playing_link: Some(&playing),
        cache: Some(&cache),
        live: &live,
        control: &control,
        on_progress: &|_| {},
        on_update: &|_| {},
    })
    .expect("refresh succeeds");

    let play_idx = refreshed.play_index.expect("playing index pinned");
    assert_eq!(refreshed.links[play_idx].0.url, playing.url);
    assert_eq!(live.snapshot().links.len(), refreshed.links.len());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn check_and_pick_and_cancelled_open_behave_cleanly() {
    let server = MockHttpServer::start(vec![(
        "/ok.mp4".into(),
        200,
        "video/mp4".into(),
        "00000000ftypisom".into(),
    )]);
    let gw = Arc::new(ScriptedGateway::single(
        vec![make_link("OK", server.url("/ok.mp4"), 720, "VIDEO")],
        Vec::new(),
    ));
    let resolver = StreamResolver::new(gw);
    let picked = check_and_pick(&resolver, "Demo", "ep3", &|_| {}).expect("picked");
    assert_eq!(picked.play_index, Some(0));

    let control = OpenControl::default();
    control.cancel_open();
    let cancelled = run_open(OpenRequest {
        streams: &resolver,
        provider: "Demo",
        data: "ep3",
        media_key: "ep3",
        cache: None,
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &|_| {},
        control: &control,
    });
    assert!(matches!(cancelled, Err(playback_session::OpenError::Empty)));
}

#[test]
fn live_sources_and_open_control_concurrent_race_invariants() {
    let live = LiveSources::new();
    let control = OpenControl::default();
    let mut handles = Vec::new();

    for t in 0..8usize {
        let live_c = live.clone();
        let ctrl_c = control.clone();
        handles.push(thread::spawn(move || {
            for i in 0..400usize {
                let link = make_link(
                    &format!("t{t}-{i}"),
                    format!("https://example.invalid/{t}/{i}.mp4"),
                    720,
                    "VIDEO",
                );
                live_c.publish(CheckedOpen {
                    links: vec![(link, None)],
                    play_index: Some(0),
                    subtitles: Vec::new(),
                });
                let snap = live_c.snapshot();
                if !snap.links.is_empty() {
                    assert_eq!(snap.links.len(), 1);
                    assert_eq!(snap.play_index, Some(0));
                }
                if i == 200 && t == 0 {
                    ctrl_c.skip_checks();
                }
            }
        }));
    }

    for h in handles {
        h.join().expect("thread finished without panic");
    }
    assert!(control.skip.load(Ordering::SeqCst));
    assert_eq!(live.snapshot().links.len(), 1);
}

#[test]
fn fresh_and_stale_seeds_play_without_live_links_and_skip_broken_seeds() {
    let server = MockHttpServer::start(vec![
        (
            "/broken_seed.mp4".into(),
            404,
            "text/plain".into(),
            "no".into(),
        ),
        (
            "/good_seed.mp4".into(),
            200,
            "video/mp4".into(),
            "00000000ftypisom".into(),
        ),
    ]);
    let dir = temp_dir("seed_only");
    let now = Arc::new(Mutex::new(20_000u64));
    let cache = Arc::new(TitleCache::with_clock(
        dir.clone(),
        CacheLimits::default(),
        fixed_clock(Arc::clone(&now)),
    ));
    let key = title_key("Demo", "seed_only");
    let broken_link = make_link(
        "Broken 1080p",
        server.url("/broken_seed.mp4"),
        1080,
        "VIDEO",
    );
    let good_link = make_link("Good 720p", server.url("/good_seed.mp4"), 720, "VIDEO");
    cache
        .save(
            &key,
            &CachedTitle {
                complete: true,
                links: vec![
                    CachedLink {
                        link: broken_link.clone(),
                        health: LinkHealth::Broken,
                    },
                    CachedLink {
                        link: good_link.clone(),
                        health: LinkHealth::Working,
                    },
                ],
                last_good: Some(good_link.url.clone()),
            },
        )
        .expect("seed");

    // Live gateway returns zero links; only the non-broken cached seed can play.
    let empty_gw = Arc::new(ScriptedGateway::single(Vec::new(), Vec::new()));
    let resolver = StreamResolver::new(empty_gw);
    let control = OpenControl::default();

    // 1. Fresh cache: broken seed must be skipped, working seed plays immediately.
    let fresh = run_open(OpenRequest {
        streams: &resolver,
        provider: "Demo",
        data: "seed_only",
        media_key: "seed_only",
        cache: Some(&cache),
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &|_| {},
        control: &control,
    })
    .expect("fresh seed opens without live links");
    assert_eq!(fresh.links.len(), 1);
    assert_eq!(fresh.links[0].0.url, good_link.url);
    assert_eq!(fresh.play_index, Some(0));

    // 2. Stale cache (past 20m fresh_for, within 24h keep_for): probes & plays working seed.
    *now.lock().expect("clock") += 7_200;
    let stale = run_open(OpenRequest {
        streams: &resolver,
        provider: "Demo",
        data: "seed_only",
        media_key: "seed_only",
        cache: Some(&cache),
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &|_| {},
        control: &control,
    })
    .expect("stale seed opens without live links");
    assert_eq!(stale.links.len(), 1);
    assert_eq!(stale.links[0].0.url, good_link.url);
    assert_eq!(stale.play_index, Some(0));

    // 3. cached_title_from_open preserves Broken vs Working health accurately.
    let mixed = CheckedOpen {
        links: vec![
            (
                broken_link,
                Some(app_services::LinkHealth::Broken("404".into())),
            ),
            (
                good_link,
                Some(app_services::LinkHealth::Working {
                    size_bytes: Some(16),
                    subtitles: Vec::new(),
                }),
            ),
        ],
        play_index: Some(1),
        subtitles: Vec::new(),
    };
    let saved_title = cached_title_from_open(&mixed);
    assert_eq!(saved_title.links[0].health, LinkHealth::Broken);
    assert_eq!(saved_title.links[1].health, LinkHealth::Working);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn run_open_deduplicates_identical_subtitle_urls() {
    let server = MockHttpServer::start(vec![(
        "/ok.mp4".into(),
        200,
        "video/mp4".into(),
        "00000000ftypisom".into(),
    )]);
    let gw = Arc::new(ScriptedGateway::single(
        vec![make_link("OK", server.url("/ok.mp4"), 720, "VIDEO")],
        vec![
            SubtitleTrack {
                lang: "English".into(),
                url: "https://subs.example/en.vtt".into(),
                headers: None,
            },
            SubtitleTrack {
                lang: "English (dup)".into(),
                url: "https://subs.example/en.vtt".into(),
                headers: None,
            },
            SubtitleTrack {
                lang: "Arabic".into(),
                url: "https://subs.example/ar.vtt".into(),
                headers: None,
            },
        ],
    ));
    let resolver = StreamResolver::new(gw);
    let control = OpenControl::default();
    let opened = run_open(OpenRequest {
        streams: &resolver,
        provider: "Demo",
        data: "sub_dedup",
        media_key: "sub_dedup",
        cache: None,
        on_progress: &|_| {},
        on_update: &|_| {},
        on_ready: &|_| {},
        control: &control,
    })
    .expect("open succeeds");
    assert_eq!(opened.subtitles.len(), 2);
}

#[test]
fn open_control_priority_transitions() {
    use playback_session::CheckPriority;

    let control = OpenControl::default();
    assert_eq!(control.check_priority(), CheckPriority::Normal);

    control.prioritize_subtitles();
    assert!(matches!(
        control.check_priority(),
        CheckPriority::SubtitlesAuto { .. }
    ));

    control.signal_subtitles_satisfied();
    assert_eq!(control.check_priority(), CheckPriority::Normal);

    control.pin_subtitles_focus();
    assert_eq!(control.check_priority(), CheckPriority::SubtitlesPinned);
    control.prioritize_subtitles();
    assert_eq!(control.check_priority(), CheckPriority::SubtitlesPinned);
    control.signal_subtitles_satisfied();
    assert_eq!(control.check_priority(), CheckPriority::SubtitlesPinned);

    control.resume_checks_now();
    assert_eq!(control.check_priority(), CheckPriority::Normal);
}

#[test]
fn refresh_sources_resets_pinned_priority_to_normal() {
    use playback_session::CheckPriority;

    let control = OpenControl::default();
    control.pin_subtitles_focus();
    let server = MockHttpServer::start(vec![(
        "/ok.mp4".into(),
        200,
        "video/mp4".into(),
        "00000000ftypisom".into(),
    )]);
    let gw = Arc::new(ScriptedGateway::single(
        vec![make_link("OK", server.url("/ok.mp4"), 1080, "VIDEO")],
        vec![],
    ));
    let resolver = StreamResolver::new(gw);
    let live = LiveSources::new();
    let refreshed = refresh_sources(RefreshRequest {
        streams: &resolver,
        provider: "Demo",
        data: "reset_gate",
        media_key: "reset_gate",
        playing_link: None,
        cache: None,
        live: &live,
        control: &control,
        on_progress: &|_| {},
        on_update: &|_| {},
    })
    .expect("refresh succeeds");
    assert_eq!(refreshed.links.len(), 1);
    assert_eq!(control.check_priority(), CheckPriority::Normal);
}
