use std::collections::HashMap;

use crate::subtitle_fetcher::FetchError;

use super::*;

/// Answers from a table; any other address is a 404.
struct Table(HashMap<&'static str, &'static str>);

impl SubtitleFetcher for Table {
    fn fetch(&self, url: &str, _: &[(String, String)]) -> Result<Vec<u8>, FetchError> {
        self.0
            .get(url)
            .map(|body| body.as_bytes().to_vec())
            .ok_or(FetchError::Status { status: 404 })
    }
}

#[test]
fn addresses_are_resolved_like_a_browser_would() {
    let base = "https://h.test/a/b/list.m3u8";
    assert_eq!(resolve_url(base, "seg1.vtt"), "https://h.test/a/b/seg1.vtt");
    assert_eq!(
        resolve_url(base, "/root/seg1.vtt"),
        "https://h.test/root/seg1.vtt"
    );
    assert_eq!(
        resolve_url(base, "https://x.test/s.vtt"),
        "https://x.test/s.vtt"
    );
    assert_eq!(
        resolve_url("https://h.test", "s.vtt"),
        "https://h.test/s.vtt"
    );
}

#[test]
fn the_pieces_of_a_playlist_are_joined_into_one_file() {
    let fetcher = Table(HashMap::from([
        ("https://h.test/p/one.vtt", "WEBVTT\n\n00:01.000 --> 00:02.000\nfirst\n"),
        ("https://h.test/p/two.vtt", "\u{FEFF}WEBVTT\nX-TIMESTAMP-MAP=LOCAL:00:00:00.000\n\n00:03.000 --> 00:04.000\nsecond\n"),
    ]));
    let playlist = "#EXTM3U\n#EXT-X-TARGETDURATION:10\n#EXTINF:10,\none.vtt\n#EXTINF:10,\ntwo.vtt\n#EXT-X-ENDLIST\n";
    let joined = flatten_playlist(playlist, "https://h.test/p/list.m3u8", &fetcher).expect("cues");
    assert!(joined.starts_with("WEBVTT\n\n"));
    assert!(joined.contains("first") && joined.contains("second"));
    assert_eq!(joined.matches("WEBVTT").count(), 1, "only one header");
    assert!(!joined.contains("X-TIMESTAMP-MAP"));
}

#[test]
fn a_master_playlist_is_followed_to_its_subtitle_rendition() {
    let fetcher = Table(HashMap::from([
        (
            "https://h.test/m/subs/en.m3u8",
            "#EXTM3U\n#EXTINF:5,\nen1.vtt\n",
        ),
        (
            "https://h.test/m/subs/en1.vtt",
            "WEBVTT\n\n00:01.000 --> 00:02.000\nhello\n",
        ),
    ]));
    let master = "#EXTM3U\n#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"s\",NAME=\"English\",URI=\"subs/en.m3u8\"\n";
    let joined = flatten_playlist(master, "https://h.test/m/master.m3u8", &fetcher).expect("cues");
    assert!(joined.contains("hello"));
}

#[test]
fn nothing_comes_out_when_no_piece_has_cues_or_can_be_downloaded() {
    let fetcher = Table(HashMap::new());
    assert_eq!(
        flatten_playlist(
            "#EXTM3U\n#EXTINF:5,\nmissing.vtt\n",
            "https://h.test/l.m3u8",
            &fetcher
        ),
        None
    );
}
