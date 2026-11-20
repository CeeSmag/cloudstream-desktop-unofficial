//! Property-based tests for `subtitle_store::normalize_subtitle`.

use std::io::Write;

use flate2::write::GzEncoder;
use flate2::Compression;
use proptest::prelude::*;
use subtitle_store::{normalize_subtitle, NormalizeError};

proptest! {
    #[test]
    fn normalize_subtitle_never_panics_on_arbitrary_bytes(raw in prop::collection::vec(any::<u8>(), 0..2048)) {
        let res = normalize_subtitle(&raw);
        if let Ok(norm) = res {
            prop_assert!(!norm.text.trim().is_empty());
            prop_assert!(matches!(norm.extension, "srt" | "vtt" | "ass" | "smi" | "m3u8"));
        }
    }

    #[test]
    fn normalize_subtitle_always_rejects_html_documents(
        title in "[a-zA-Z0-9 ]{0,30}",
        body in "[a-zA-Z0-9 ]{0,60}",
        prefix in prop_oneof![Just("<!DOCTYPE html>"), Just("<html>"), Just("  <HTML lang=\"en\">")],
    ) {
        let doc = format!("{prefix}<head><title>{title}</title></head><body>{body}</body></html>");
        prop_assert_eq!(normalize_subtitle(doc.as_bytes()), Err(NormalizeError::WebPage));
    }

    #[test]
    fn normalize_subtitle_preserves_valid_vtt_through_bom_and_gzip(
        cue in "[a-zA-Z0-9 ,.!?]{1,60}",
        with_bom in prop::bool::ANY,
        with_gzip in prop::bool::ANY,
    ) {
        let content = format!("WEBVTT\n\n00:00:01.000 --> 00:00:03.000\n{cue}\n");
        let mut raw = Vec::new();
        if with_bom {
            raw.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
        }
        raw.extend_from_slice(content.as_bytes());
        let payload = if with_gzip {
            let mut enc = GzEncoder::new(Vec::new(), Compression::fast());
            enc.write_all(&raw).expect("gzip write");
            enc.finish().expect("gzip finish")
        } else {
            raw
        };
        let norm = normalize_subtitle(&payload).expect("valid vtt normalizes");
        prop_assert_eq!(norm.extension, "vtt");
        let starts_with_bom = norm.text.starts_with('\u{FEFF}');
        prop_assert!(!starts_with_bom);
        prop_assert!(norm.text.contains(&cue));
    }
}

#[test]
fn normalize_subtitle_unpacks_zip_with_extra_field_and_multibyte_length() {
    // Build a stored ZIP entry whose payload is >256 bytes and includes a 6-byte extra field.
    let mut srt = String::new();
    for i in 1..=12 {
        srt.push_str(&format!(
            "{i}\n00:00:{i:02},000 --> 00:00:{i:02},500\nLine number {i} in zip payload.\n\n"
        ));
    }
    let payload = srt.as_bytes();
    assert!(payload.len() > 260);
    let name = b"movie.srt";
    let extra = b"EXTRA!";

    let mut zip = Vec::new();
    zip.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]); // local file header
    zip.extend_from_slice(&20u16.to_le_bytes()); // version
    zip.extend_from_slice(&0u16.to_le_bytes()); // flags
    zip.extend_from_slice(&0u16.to_le_bytes()); // method = stored
    zip.extend_from_slice(&0u16.to_le_bytes()); // mod time
    zip.extend_from_slice(&0u16.to_le_bytes()); // mod date
    zip.extend_from_slice(&0u32.to_le_bytes()); // crc32
    zip.extend_from_slice(&(payload.len() as u32).to_le_bytes()); // compressed size (>256)
    zip.extend_from_slice(&(payload.len() as u32).to_le_bytes()); // uncompressed size
    zip.extend_from_slice(&(name.len() as u16).to_le_bytes()); // name len
    zip.extend_from_slice(&(extra.len() as u16).to_le_bytes()); // extra len (>0)
    zip.extend_from_slice(name);
    zip.extend_from_slice(extra);
    zip.extend_from_slice(payload);

    let norm = normalize_subtitle(&zip).expect("stored zip with extra field unpacks");
    assert_eq!(norm.extension, "srt");
    assert!(norm.text.contains("Line number 12 in zip payload."));
}
