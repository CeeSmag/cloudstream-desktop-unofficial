//! Fuzz target for the Cloudflare exchange texts that cross a trust boundary: request files written by
//! (untrusted) plugin sidecars, status lines from the verification helper, and the pass JSON they carry.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let _ = cloudflare_bridge::parse_request(&text);
    let _ = cloudflare_bridge::classify_status_line(&text);
    let _ = cloudflare_bridge::parse_status(&text);
    let _ = cloudflare_bridge::decode_pass(&text);
});
