use std::collections::HashMap;

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
fn lowercase_mpd_is_a_working_dash_manifest() {
    assert!(classify(
        "DASH",
        &answer(
            200,
            "application/dash+xml",
            b"<mpd xmlns=\"urn:mpeg:dash:schema:mpd:2011\">"
        )
    )
    .is_working());
}

#[test]
fn html_content_type_with_ftyp_body_is_working() {
    let body = b"\0\0\0\x18ftypmp42";
    assert!(classify("VIDEO", &answer(200, "text/html", body)).is_working());
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
fn request_headers_default_user_agent_matches_cloudstream_chrome() {
    let headers = request_headers(&PlayableLink::default());
    assert!(headers.contains(&(
        "User-Agent".to_owned(),
        core_model::DEFAULT_STREAM_USER_AGENT.to_owned()
    )));
}

#[test]
fn check_worker_count_stays_between_two_and_twelve() {
    assert_eq!(check_worker_count(1), 2);
    assert_eq!(check_worker_count(4), 4);
    assert_eq!(check_worker_count(12), 12);
    assert_eq!(check_worker_count(64), 12);
}
