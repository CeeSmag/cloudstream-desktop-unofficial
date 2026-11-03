use super::*;

/// One EBML element with a one- or two-byte size.
fn element(id: &[u8], body: &[u8]) -> Vec<u8> {
    let mut bytes = id.to_vec();
    if body.len() < 127 {
        bytes.push(0x80 | u8::try_from(body.len()).expect("small"));
    } else {
        let length = u16::try_from(body.len()).expect("medium");
        bytes.push(0x40 | u8::try_from(length >> 8).expect("high byte"));
        bytes.push(u8::try_from(length & 0xFF).expect("low byte"));
    }
    bytes.extend_from_slice(body);
    bytes
}

fn track(kind: u8, language: Option<&str>, name: Option<&str>) -> Vec<u8> {
    let mut body = element(&[0x83], &[kind]);
    if let Some(language) = language {
        body.extend(element(&[0x22, 0xB5, 0x9C], language.as_bytes()));
    }
    if let Some(name) = name {
        body.extend(element(&[0x53, 0x6E], name.as_bytes()));
    }
    element(&[0xAE], &body)
}

fn matroska(tracks: &[Vec<u8>]) -> Vec<u8> {
    let list = element(&[0x16, 0x54, 0xAE, 0x6B], &tracks.concat());
    let segment = element(&[0x18, 0x53, 0x80, 0x67], &list);
    let mut file = element(&[0x1A, 0x45, 0xDF, 0xA3], b"\x42\x82\x84webm");
    file.extend(segment);
    file
}

#[test]
fn a_matroska_file_lists_its_subtitle_tracks_and_skips_video_and_audio() {
    let file = matroska(&[
        track(1, Some("und"), None),
        track(2, Some("eng"), None),
        track(17, Some("ara"), None),
        track(17, Some("eng"), None),
        track(17, Some("eng"), Some("SDH")),
    ]);
    assert_eq!(subtitle_languages(&file), ["ara", "eng"]);
}

#[test]
fn a_matroska_subtitle_track_without_a_language_is_english_and_an_unknown_one_uses_its_name() {
    let file = matroska(&[
        track(17, None, None),
        track(17, Some("und"), Some("Arabic")),
    ]);
    assert_eq!(subtitle_languages(&file), ["eng", "Arabic"]);
}

#[test]
fn a_matroska_file_cut_off_in_the_middle_still_gives_the_tracks_before_the_cut() {
    let mut file = matroska(&[track(17, Some("spa"), None), track(17, Some("fre"), None)]);
    file.truncate(file.len() - 4);
    assert_eq!(subtitle_languages(&file), ["spa"]);
}

#[test]
fn an_hls_master_playlist_lists_its_subtitle_renditions() {
    let playlist = b"#EXTM3U\n\
        #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"a\",LANGUAGE=\"fr\",NAME=\"Francais\"\n\
        #EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"s\",LANGUAGE=\"en\",NAME=\"English\"\n\
        #EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"s\",NAME=\"Arabic\"\n";
    assert_eq!(subtitle_languages(playlist), ["en", "Arabic"]);
}

#[test]
fn a_dash_manifest_lists_its_text_adaptation_sets() {
    let manifest = b"<?xml version=\"1.0\"?><MPD><Period>\
        <AdaptationSet contentType=\"video\" lang=\"und\"></AdaptationSet>\
        <AdaptationSet contentType=\"text\" lang=\"ar\"></AdaptationSet>\
        <AdaptationSet mimeType=\"text/vtt\" lang=\"en\"></AdaptationSet></Period></MPD>";
    assert_eq!(subtitle_languages(manifest), ["ar", "en"]);
}

#[test]
fn streams_that_say_nothing_at_the_start_yield_nothing() {
    assert!(subtitle_languages(b"\0\0\0\x18ftypmp42").is_empty());
    assert!(subtitle_languages(b"").is_empty());
    assert!(subtitle_languages(&[0x1A, 0x45, 0xDF, 0xA3, 0xFF]).is_empty());
}
