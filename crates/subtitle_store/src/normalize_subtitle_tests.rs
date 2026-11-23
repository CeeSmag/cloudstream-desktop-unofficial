use std::io::Write;

use flate2::write::{DeflateEncoder, GzEncoder};
use flate2::Compression;

use super::*;

const SRT: &str = "1\n00:00:01,000 --> 00:00:02,000\nمرحبا\n\n";

fn normalized(bytes: &[u8]) -> NormalizedSubtitle {
    normalize_subtitle(bytes).expect("a usable subtitle")
}

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).expect("write");
    encoder.finish().expect("finish")
}

fn deflate(data: &[u8]) -> Vec<u8> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).expect("write");
    encoder.finish().expect("finish")
}

/// One zip entry: a local header, the name and the data (deflated or stored).
fn zip_entry(name: &str, data: &[u8], deflated: bool) -> Vec<u8> {
    let body = if deflated {
        deflate(data)
    } else {
        data.to_vec()
    };
    let mut entry = vec![0x50, 0x4B, 0x03, 0x04, 20, 0, 0, 0];
    entry.extend(if deflated { [8u8, 0] } else { [0u8, 0] });
    entry.extend([0; 8]); // time, date, crc
    entry.extend(u32::try_from(body.len()).expect("small").to_le_bytes());
    entry.extend(u32::try_from(data.len()).expect("small").to_le_bytes());
    entry.extend(u16::try_from(name.len()).expect("short").to_le_bytes());
    entry.extend([0, 0]); // no extra field
    entry.extend(name.as_bytes());
    entry.extend(body);
    entry
}

#[test]
fn plain_text_is_kept_and_taken_for_srt() {
    let subtitle = normalized(SRT.as_bytes());
    assert_eq!((subtitle.text.as_str(), subtitle.extension), (SRT, "srt"));
}

#[test]
fn the_kind_of_file_is_read_from_its_start() {
    assert_eq!(
        normalized(b"WEBVTT\n\n00:01.000 --> 00:02.000\nhi").extension,
        "vtt"
    );
    assert_eq!(normalized(b"[Script Info]\nTitle: x\n").extension, "ass");
    assert_eq!(normalized(b"<SAMI><BODY></BODY></SAMI>").extension, "smi");
    assert_eq!(normalized(b"#EXTM3U\n#EXT-X-VERSION:3\n").extension, "m3u8");
}

#[test]
fn a_utf8_byte_order_mark_is_removed() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend(SRT.as_bytes());
    assert_eq!(normalized(&bytes).text, SRT);
}

#[test]
fn utf16_in_either_byte_order_is_read() {
    let units: Vec<u16> = SRT.encode_utf16().collect();
    let little: Vec<u8> = [0xFF, 0xFE]
        .into_iter()
        .chain(units.iter().flat_map(|u| u.to_le_bytes()))
        .collect();
    let big: Vec<u8> = [0xFE, 0xFF]
        .into_iter()
        .chain(units.iter().flat_map(|u| u.to_be_bytes()))
        .collect();
    assert_eq!(normalized(&little).text, SRT);
    assert_eq!(normalized(&big).text, SRT);
}

#[test]
fn old_windows_1252_text_is_read_instead_of_failing() {
    // "café – ok" with the é as 0xE9 and the dash as 0x96.
    let bytes = [b'c', b'a', b'f', 0xE9, b' ', 0x96, b' ', b'o', b'k'];
    assert_eq!(normalized(&bytes).text, "café \u{2013} ok");
}

#[test]
fn a_gzip_answer_is_unpacked() {
    assert_eq!(normalized(&gzip(SRT.as_bytes())).text, SRT);
}

#[test]
fn the_first_subtitle_of_a_zip_is_taken_and_other_files_are_skipped() {
    let mut zip = zip_entry("__MACOSX/._a.srt", b"junk", false);
    zip.extend(zip_entry("readme.txt", b"hello", true));
    zip.extend(zip_entry("movie.srt", SRT.as_bytes(), true));
    zip.extend(zip_entry("other.srt", b"not this one", false));
    assert_eq!(normalized(&zip).text, SRT);
}

#[test]
fn a_zip_without_a_subtitle_is_refused() {
    let zip = zip_entry("readme.txt", b"hello", true);
    assert_eq!(
        normalize_subtitle(&zip),
        Err(NormalizeError::NoSubtitleInArchive)
    );
}

#[test]
fn a_broken_archive_is_reported_not_trusted() {
    let mut zip = zip_entry("movie.srt", SRT.as_bytes(), true);
    zip.truncate(40);
    assert!(matches!(
        normalize_subtitle(&zip),
        Err(NormalizeError::Archive(_))
    ));
    assert!(matches!(
        normalize_subtitle(&[0x1F, 0x8B, 0x08, 0x00, 0xFF, 0xFF]),
        Err(NormalizeError::Archive(_))
    ));
}

#[test]
fn nothing_and_web_pages_are_refused() {
    assert_eq!(normalize_subtitle(b""), Err(NormalizeError::Empty));
    assert_eq!(normalize_subtitle(b"  \n\t "), Err(NormalizeError::Empty));
    assert_eq!(
        normalize_subtitle(b"<!DOCTYPE html><html></html>"),
        Err(NormalizeError::WebPage)
    );
    assert_eq!(
        normalize_subtitle(b"  <html><body>Not found</body></html>"),
        Err(NormalizeError::WebPage)
    );
}

/// A real subtitle of `bytes` or a little more: the same cue over and over.
fn long_srt(bytes: usize) -> String {
    let mut text = String::new();
    let mut n = 1;
    while text.len() < bytes {
        text.push_str(&format!("{n}\n00:00:01,000 --> 00:00:02,000\nline {n}\n\n"));
        n += 1;
    }
    text
}

/// One zip entry whose sizes follow the data in a descriptor (bit 3 of the flags), as zips
/// written by streaming tools are; `signed` puts the optional descriptor signature first.
fn zip_entry_with_descriptor(name: &str, data: &[u8], signed: bool) -> Vec<u8> {
    let body = deflate(data);
    let mut entry = vec![0x50, 0x4B, 0x03, 0x04, 20, 0, 0x08, 0, 8, 0];
    entry.extend([0; 16]); // time, date, crc and both sizes: all in the descriptor
    entry.extend(u16::try_from(name.len()).expect("short").to_le_bytes());
    entry.extend([0, 0]);
    entry.extend(name.as_bytes());
    entry.extend(&body);
    if signed {
        entry.extend([0x50, 0x4B, 0x07, 0x08]);
    }
    entry.extend([0; 4]); // crc
    entry.extend(u32::try_from(body.len()).expect("small").to_le_bytes());
    entry.extend(u32::try_from(data.len()).expect("small").to_le_bytes());
    entry
}

#[test]
fn entries_whose_sizes_follow_in_a_descriptor_are_stepped_over_with_or_without_its_signature() {
    for signed in [true, false] {
        let mut zip = zip_entry_with_descriptor("readme.txt", b"hello there", signed);
        zip.extend(zip_entry_with_descriptor(
            "movie.srt",
            SRT.as_bytes(),
            signed,
        ));
        assert_eq!(normalized(&zip).text, SRT, "signed descriptor: {signed}");
    }
}

#[test]
fn a_large_real_subtitle_is_unpacked_whole_from_gzip_and_zip() {
    let text = long_srt(3 * 1024 * 1024);
    assert_eq!(normalized(&gzip(text.as_bytes())).text, text);
    let zip = zip_entry("movie.srt", text.as_bytes(), true);
    assert_eq!(normalized(&zip).text, text);
}
