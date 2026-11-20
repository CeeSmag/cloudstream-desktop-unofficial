//! Fuzz target for untrusted subtitle payloads (raw, UTF-16, gzip, zip, HTML error pages).
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = subtitle_store::normalize_subtitle(data);
});
