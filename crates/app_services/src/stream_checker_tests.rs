use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use extension_repo::RepoError;

use super::*;

fn answer(status: u16, content_type: &str, body: &[u8]) -> ProbeResponse {
    ProbeResponse {
        status,
        content_type: content_type.to_owned(),
        head: body.to_vec(),
        total_bytes: None,
    }
}

#[test]
fn a_video_file_is_working() {
    let matroska = [0x1A, 0x45, 0xDF, 0xA3, 0x42];
    assert!(classify("VIDEO", &answer(206, "video/x-matroska", &matroska)).is_working());
}

#[test]
fn a_video_file_reports_its_size_but_a_playlist_does_not() {
    let mut video = answer(206, "video/mp4", b"\0\0\0\x18ftypmp42");
    video.total_bytes = Some(1_400_000_000);
    assert_eq!(
        classify("VIDEO", &video),
        LinkHealth::Working {
            size_bytes: Some(1_400_000_000),
            subtitles: Vec::new()
        }
    );
    let mut playlist = answer(200, "application/x-mpegurl", b"#EXTM3U\n");
    playlist.total_bytes = Some(300);
    assert_eq!(
        classify("M3U8", &playlist),
        LinkHealth::Working {
            size_bytes: None,
            subtitles: Vec::new()
        }
    );
}

#[test]
fn an_hls_playlist_is_working_even_when_served_as_html() {
    let playlist = answer(
        200,
        "text/html; charset=UTF-8",
        b"#EXTM3U\n#EXT-X-VERSION:3",
    );
    assert!(classify("M3U8", &playlist).is_working());
}

#[test]
fn error_statuses_are_broken() {
    let health = classify("VIDEO", &answer(403, "", b""));
    assert_eq!(health, LinkHealth::Broken("HTTP 403".to_owned()));
}

#[test]
fn a_web_page_in_place_of_a_video_is_broken() {
    assert!(!classify("VIDEO", &answer(200, "text/html", b"<!doctype html>")).is_working());
    assert!(!classify("VIDEO", &answer(200, "", b"<html>")).is_working());
    assert!(!classify("VIDEO", &answer(200, "application/json", b"{\"error\":1}")).is_working());
}

#[test]
fn a_playlist_link_that_is_not_a_playlist_is_broken() {
    assert!(!classify("M3U8", &answer(200, "video/mp4", b"\0\0\0\x18ftypmp42")).is_working());
}

#[test]
fn a_dash_manifest_is_working_and_an_empty_answer_is_not() {
    assert!(classify(
        "DASH",
        &answer(200, "application/dash+xml", b"<?xml version")
    )
    .is_working());
    assert!(!classify("VIDEO", &answer(200, "video/mp4", b"")).is_working());
}

#[test]
fn request_headers_add_referer_and_user_agent_unless_the_link_has_them() {
    let link = PlayableLink {
        referer: "https://site.test".into(),
        headers: HashMap::from([("User-Agent".to_owned(), "MovieBlast".to_owned())]),
        ..PlayableLink::default()
    };
    let headers = request_headers(&link);
    assert!(headers.contains(&("Referer".to_owned(), "https://site.test".to_owned())));
    assert!(headers.contains(&("User-Agent".to_owned(), "MovieBlast".to_owned())));
    assert_eq!(headers.len(), 2);
}

#[test]
fn check_queue_stops_early_when_the_answers_are_no_longer_wanted() {
    let checker = StreamChecker::new(Arc::new(ScriptedProbe(HashMap::new())));
    let queue = queue_of(200);
    let seen = AtomicUsize::new(0);

    checker.check_queue(&queue, &|| true, &|_, _| {
        seen.fetch_add(1, Ordering::SeqCst);
        false
    });

    assert!(
        seen.load(Ordering::SeqCst) <= WORKERS,
        "kept going after being told to stop"
    );
}

#[test]
fn check_queue_starts_nothing_once_keep_going_says_no() {
    let checker = StreamChecker::new(Arc::new(ScriptedProbe(HashMap::new())));
    let queue = queue_of(50);
    let seen = AtomicUsize::new(0);

    checker.check_queue(&queue, &|| false, &|_, _| {
        seen.fetch_add(1, Ordering::SeqCst);
        true
    });

    assert_eq!(
        seen.load(Ordering::SeqCst),
        0,
        "a check started after cancellation"
    );
}

fn queue_of(count: usize) -> CheckQueue {
    let queue = CheckQueue::default();
    queue.push((0..count).map(|index| (index, link("https://dead/x"))));
    queue.close();
    queue
}

/// Answers by URL; unknown URLs fail like an unreachable host.
struct ScriptedProbe(HashMap<String, ProbeResponse>);

impl HttpProbe for ScriptedProbe {
    fn probe(&self, url: &str, _headers: &[(String, String)]) -> Result<ProbeResponse, RepoError> {
        self.0.get(url).cloned().ok_or(RepoError::Download {
            url: url.to_owned(),
            reason: "unreachable".to_owned(),
        })
    }
}

fn link(url: &str) -> PlayableLink {
    PlayableLink {
        url: url.to_owned(),
        link_type: "VIDEO".to_owned(),
        ..PlayableLink::default()
    }
}

#[test]
fn check_queue_reports_every_stream_under_the_index_the_caller_gave() {
    let probe = ScriptedProbe(HashMap::from([(
        "https://ok/a".to_owned(),
        answer(206, "video/mp4", b"\0\0\0\x18ftyp"),
    )]));
    let checker = StreamChecker::new(Arc::new(probe));
    let queue = CheckQueue::default();
    queue.push((0..30).map(|index| {
        let url = if index == 5 {
            "https://ok/a"
        } else {
            "https://dead/x"
        };
        (index * 2, link(url))
    }));
    queue.close();
    let results = Mutex::new(Vec::new());

    checker.check_queue(&queue, &|| true, &|index, health| {
        results.lock().expect("lock").push((index, health));
        true
    });

    let mut results = results.into_inner().expect("lock");
    results.sort_by_key(|(index, _)| *index);
    assert_eq!(results.len(), 30);
    let working: Vec<usize> = results
        .iter()
        .filter(|(_, h)| h.is_working())
        .map(|(i, _)| *i)
        .collect();
    assert_eq!(working, [10]);
}
