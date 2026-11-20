//! Fuzz target for untrusted M3U8/pipe stream inputs.
#![no_main]

use core_model::PlayableLink;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = subtitle_store::normalize_subtitle(data);
    if let Ok(text) = std::str::from_utf8(data) {
        let link = PlayableLink {
            url: text.to_owned(),
            link_type: "M3U8".to_owned(),
            ..PlayableLink::default()
        };
        let _ = app_services::normalize_pipe_link(link);
    }
});
