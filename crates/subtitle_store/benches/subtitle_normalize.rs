//! Hot-path benchmarks for subtitle unpacking, decoding, and format detection.
#![allow(missing_docs)]

use std::io::Write;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use flate2::write::GzEncoder;
use flate2::Compression;
use subtitle_store::normalize_subtitle;

fn sample_srt(cues: usize) -> Vec<u8> {
    let mut out = String::with_capacity(cues * 64);
    for idx in 1..=cues {
        out.push_str(&format!(
            "{idx}\n00:00:{:02},000 --> 00:00:{:02},500\nDialogue line number {idx}.\n\n",
            idx % 58,
            (idx % 58) + 1
        ));
    }
    out.into_bytes()
}

fn sample_utf16_le(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

fn sample_gzip(raw: &[u8]) -> Vec<u8> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::fast());
    let _ = enc.write_all(raw);
    enc.finish().unwrap_or_default()
}

fn bench_normalize(c: &mut Criterion) {
    let srt_500 = sample_srt(500);
    let srt_gz = sample_gzip(&srt_500);
    let vtt_utf16 = sample_utf16_le(&format!(
        "WEBVTT\n\n{}",
        String::from_utf8_lossy(&sample_srt(200))
    ));
    let html_page = b"<!DOCTYPE html><html><body>403 Forbidden</body></html>";

    c.bench_function("normalize_srt_500_cues", |b| {
        b.iter(|| normalize_subtitle(black_box(&srt_500)))
    });
    c.bench_function("normalize_gzip_srt_500_cues", |b| {
        b.iter(|| normalize_subtitle(black_box(&srt_gz)))
    });
    c.bench_function("normalize_utf16le_vtt_200_cues", |b| {
        b.iter(|| normalize_subtitle(black_box(&vtt_utf16)))
    });
    c.bench_function("normalize_reject_html", |b| {
        b.iter(|| normalize_subtitle(black_box(html_page)))
    });
}

criterion_group!(benches, bench_normalize);
criterion_main!(benches);
