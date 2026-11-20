//! Deterministic corpus replay and mutation stress test for all 4 untrusted-input parsers.
//!
//! Runs on stable Rust under `cargo test` and `scripts/run_fuzz_sanity.sh` so nightly QA
//! exercises all fuzz corpora even without a nightly `cargo-fuzz` toolchain.

use std::path::Path;

use core_model::{
    HomePage, LinkSearchUpdate, LoadDetails, PlayableLink, PlayableLinks, PluginListing, SearchPage,
};
use extension_repo::RepositoryManifest;

fn mutate_bytes(seed: &[u8], iter: u64) -> Vec<u8> {
    let mut out = seed.to_vec();
    if out.is_empty() {
        out.push((iter & 0xFF) as u8);
        return out;
    }
    let idx = (iter as usize) % out.len();
    if let Some(byte) = out.get_mut(idx) {
        *byte ^= ((iter.wrapping_mul(31)) & 0xFF) as u8;
    }
    if iter.is_multiple_of(5) {
        out.truncate(idx);
    } else if iter.is_multiple_of(7) {
        out.push((iter & 0xFF) as u8);
    }
    out
}

fn read_corpus(target: &str) -> Vec<Vec<u8>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fuzz/corpus")
        .join(target);
    let mut seeds = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(bytes) = std::fs::read(entry.path()) {
                seeds.push(bytes);
            }
        }
    }
    assert!(!seeds.is_empty(), "expected corpus seeds for {target}");
    seeds
}

#[test]
fn fuzz_corpus_subtitle_normalize_never_panics() {
    for seed in read_corpus("subtitle_normalize") {
        let _ = subtitle_store::normalize_subtitle(&seed);
        for i in 0..200u64 {
            let mutated = mutate_bytes(&seed, i);
            let _ = subtitle_store::normalize_subtitle(&mutated);
        }
    }
}

#[test]
fn fuzz_corpus_m3u8_and_pipe_links_never_panic() {
    for seed in read_corpus("m3u8_rewrite") {
        let _ = subtitle_store::normalize_subtitle(&seed);
        for i in 0..200u64 {
            let mutated = mutate_bytes(&seed, i);
            let _ = subtitle_store::normalize_subtitle(&mutated);
            let text = String::from_utf8_lossy(&mutated).into_owned();
            let _ = app_services::normalize_pipe_link(PlayableLink {
                url: text,
                link_type: "M3U8".into(),
                ..PlayableLink::default()
            });
        }
    }
}

#[test]
fn fuzz_corpus_sidecar_rpc_reply_never_panics() {
    for seed in read_corpus("sidecar_rpc_reply") {
        for i in 0..200u64 {
            let mutated = mutate_bytes(&seed, i);
            let _ = serde_json::from_slice::<SearchPage>(&mutated);
            let _ = serde_json::from_slice::<HomePage>(&mutated);
            let _ = serde_json::from_slice::<LoadDetails>(&mutated);
            let _ = serde_json::from_slice::<PlayableLinks>(&mutated);
            let _ = serde_json::from_slice::<LinkSearchUpdate>(&mutated);
        }
    }
}

#[test]
fn fuzz_corpus_repo_manifest_never_panics() {
    for seed in read_corpus("repo_manifest") {
        for i in 0..200u64 {
            let mutated = mutate_bytes(&seed, i);
            let _ = serde_json::from_slice::<RepositoryManifest>(&mutated);
            let _ = serde_json::from_slice::<Vec<PluginListing>>(&mutated);
        }
    }
}
