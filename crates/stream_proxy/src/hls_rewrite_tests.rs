use super::*;

const BASE: &str = "https://cdn.test/show/ep1/master.m3u8?sig=abc";

/// What the rewriter registered, in order, and the text it returned.
struct Outcome {
    text: String,
    registered: Vec<(String, EntryKind)>,
}

fn rewrite(playlist: &str) -> Result<Outcome, RewriteError> {
    let mut registered = Vec::new();
    let mut register = |url: &str, kind: EntryKind| {
        registered.push((url.to_owned(), kind));
        Some(format!("P{}", registered.len() - 1))
    };
    let text = rewrite_playlist(playlist, BASE, &mut register)?;
    Ok(Outcome { text, registered })
}

fn registered(outcome: &Outcome) -> Vec<(&str, EntryKind)> {
    outcome
        .registered
        .iter()
        .map(|(url, kind)| (url.as_str(), *kind))
        .collect()
}

#[test]
fn rewrite_playlist_master_playlist_points_variants_and_renditions_at_the_proxy() {
    let master = "#EXTM3U\n\
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"a\",NAME=\"English\",URI=\"audio/en.m3u8\"\n\
#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360\n\
360p/index.m3u8\n\
#EXT-X-STREAM-INF:BANDWIDTH=2000000,RESOLUTION=1280x720\n\
https://other.test/720p/index.m3u8\n\
#EXT-X-I-FRAME-STREAM-INF:BANDWIDTH=100000,URI=\"iframe.m3u8\"\n";
    let outcome = rewrite(master).expect("a playlist");
    assert_eq!(
        outcome.text,
        "#EXTM3U\n\
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"a\",NAME=\"English\",URI=\"P0\"\n\
#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360\n\
P1\n\
#EXT-X-STREAM-INF:BANDWIDTH=2000000,RESOLUTION=1280x720\n\
P2\n\
#EXT-X-I-FRAME-STREAM-INF:BANDWIDTH=100000,URI=\"P3\"\n"
    );
    assert_eq!(
        registered(&outcome),
        [
            (
                "https://cdn.test/show/ep1/audio/en.m3u8",
                EntryKind::Playlist
            ),
            (
                "https://cdn.test/show/ep1/360p/index.m3u8",
                EntryKind::Playlist
            ),
            ("https://other.test/720p/index.m3u8", EntryKind::Playlist),
            ("https://cdn.test/show/ep1/iframe.m3u8", EntryKind::Playlist),
        ]
    );
}

#[test]
fn rewrite_playlist_media_playlist_keeps_every_other_tag_and_rewrites_pieces_and_keys() {
    let media = "#EXTM3U\n\
#EXT-X-VERSION:6\n\
#EXT-X-TARGETDURATION:6\n\
#EXT-X-KEY:METHOD=AES-128,URI=\"../keys/k1.bin\",IV=0x0123,KEYFORMAT=\"identity\"\n\
#EXT-X-MAP:URI=\"init.mp4\",BYTERANGE=\"720@0\"\n\
#EXTINF:6.0,\n\
seg1.ts\n\
#EXTINF:6.0,\n\
/abs/seg2.ts?x=1\n\
#EXT-X-ENDLIST\n";
    let outcome = rewrite(media).expect("a playlist");
    assert_eq!(
        outcome.text,
        "#EXTM3U\n#EXT-X-VERSION:6\n#EXT-X-TARGETDURATION:6\n\
#EXT-X-KEY:METHOD=AES-128,URI=\"P0\",IV=0x0123,KEYFORMAT=\"identity\"\n\
#EXT-X-MAP:URI=\"P1\",BYTERANGE=\"720@0\"\n\
#EXTINF:6.0,\nP2\n#EXTINF:6.0,\nP3\n#EXT-X-ENDLIST\n"
    );
    assert_eq!(
        registered(&outcome),
        [
            ("https://cdn.test/show/keys/k1.bin", EntryKind::Key),
            ("https://cdn.test/show/ep1/init.mp4", EntryKind::Init),
            ("https://cdn.test/show/ep1/seg1.ts", EntryKind::Segment),
            ("https://cdn.test/abs/seg2.ts?x=1", EntryKind::Segment),
        ]
    );
}

#[test]
fn rewrite_playlist_a_playlist_on_a_media_line_is_a_playlist_and_disguised_pieces_are_pieces() {
    let outcome = rewrite("#EXTM3U\nnested/list.m3u8\nhidden/1.jpg\nplain\n").expect("a playlist");
    let kinds: Vec<EntryKind> = outcome.registered.iter().map(|(_, kind)| *kind).collect();
    assert_eq!(
        kinds,
        [EntryKind::Playlist, EntryKind::Segment, EntryKind::Segment]
    );
}

#[test]
fn rewrite_playlist_windows_line_endings_and_a_byte_order_mark_are_accepted() {
    let outcome = rewrite("\u{feff}#EXTM3U\r\n#EXTINF:4,\r\nseg.ts\r\n").expect("a playlist");
    assert_eq!(outcome.text, "#EXTM3U\n#EXTINF:4,\nP0\n");
}

#[test]
fn rewrite_playlist_an_inline_key_is_left_as_it_is() {
    let line = "#EXT-X-KEY:METHOD=AES-128,URI=\"data:text/plain;base64,AAAA\"\n";
    let outcome = rewrite(&format!("#EXTM3U\n{line}")).expect("a playlist");
    assert_eq!(outcome.text, format!("#EXTM3U\n{line}"));
    assert!(outcome.registered.is_empty());
}

#[test]
fn rewrite_playlist_tags_whose_uri_is_not_media_are_left_alone() {
    let data = "#EXT-X-SESSION-DATA:DATA-ID=\"com.x\",URI=\"data.json\"\n";
    let lookalike = "#EXT-X-MEDIA:TYPE=AUDIO,XURI=\"nothing.m3u8\"\n";
    let outcome = rewrite(&format!("#EXTM3U\n{data}{lookalike}")).expect("a playlist");
    assert_eq!(outcome.text, format!("#EXTM3U\n{data}{lookalike}"));
    assert!(outcome.registered.is_empty());
}

#[test]
fn rewrite_playlist_unsupported_schemes_are_refused_so_the_stream_cannot_read_local_files() {
    for address in [
        "file:///C:/Windows/win.ini",
        "skd://key",
        "ftp://host/seg.ts",
    ] {
        let result = rewrite(&format!("#EXTM3U\n#EXTINF:4,\n{address}\n"));
        assert!(
            matches!(result, Err(RewriteError::UnsupportedAddress(_))),
            "{address}"
        );
    }
    let key = rewrite("#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"file:///etc/key\"\n");
    assert_eq!(
        key.err(),
        Some(RewriteError::UnsupportedAddress("file".to_owned()))
    );
}

#[test]
fn rewrite_playlist_something_that_is_not_a_playlist_is_refused() {
    for body in ["<html>login</html>", "", "{\"error\":1}", "EXTM3U"] {
        assert_eq!(
            rewrite(body).err(),
            Some(RewriteError::NotAPlaylist),
            "{body:?}"
        );
    }
}

#[test]
fn rewrite_playlist_a_vod_playlist_without_an_end_marker_gets_one() {
    let vod = rewrite("#EXTM3U\n#EXT-X-PLAYLIST-TYPE:VOD\n#EXTINF:4,\ns.ts\n").expect("a playlist");
    assert!(vod.text.ends_with("#EXT-X-ENDLIST\n"));
    let live = rewrite("#EXTM3U\n#EXTINF:4,\ns.ts\n").expect("a playlist");
    assert!(
        !live.text.contains("ENDLIST"),
        "a live playlist must stay open"
    );
    let done = rewrite("#EXTM3U\n#EXT-X-PLAYLIST-TYPE:VOD\n#EXTINF:4,\ns.ts\n#EXT-X-ENDLIST\n")
        .expect("a playlist");
    assert_eq!(done.text.matches("#EXT-X-ENDLIST").count(), 1);
}

#[test]
fn rewrite_playlist_a_full_table_fails_the_playlist_instead_of_leaving_real_addresses_in_it() {
    let mut register = |_: &str, _: EntryKind| None;
    let result = rewrite_playlist("#EXTM3U\n#EXTINF:4,\ns.ts\n", BASE, &mut register);
    assert_eq!(result.err(), Some(RewriteError::TooManyAddresses));
}

#[test]
fn rewrite_playlist_a_relative_base_is_an_error() {
    let mut register = |_: &str, _: EntryKind| Some("P".to_owned());
    let result = rewrite_playlist("#EXTM3U\ns.ts\n", "/relative/base.m3u8", &mut register);
    assert_eq!(result.err(), Some(RewriteError::BadBase));
}

#[test]
fn rewrite_playlist_the_stream_tag_makes_only_the_next_line_a_playlist() {
    let outcome =
        rewrite("#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1\n\n# a comment\nv.m3u8\nseg.ts\n")
            .expect("a playlist");
    let kinds: Vec<EntryKind> = outcome.registered.iter().map(|(_, kind)| *kind).collect();
    assert_eq!(kinds, [EntryKind::Playlist, EntryKind::Segment]);
}

#[test]
fn rewrite_playlist_low_latency_parts_and_preload_hints_are_pieces_of_the_proxy() {
    let live = "#EXTM3U\n\
#EXT-X-TARGETDURATION:4\n\
#EXT-X-PART-INF:PART-TARGET=1.0\n\
#EXT-X-PART:DURATION=1.0,URI=\"part1.mp4\"\n\
#EXT-X-PRELOAD-HINT:TYPE=PART,URI=\"part2.mp4\"\n";
    let outcome = rewrite(live).expect("a playlist");
    assert_eq!(
        registered(&outcome),
        [
            ("https://cdn.test/show/ep1/part1.mp4", EntryKind::Segment),
            ("https://cdn.test/show/ep1/part2.mp4", EntryKind::Segment),
        ]
    );
    assert!(
        !outcome.text.contains("part1.mp4") && !outcome.text.contains("part2.mp4"),
        "no address of the host may stay in the playlist: {}",
        outcome.text
    );
}
