//! Fuzz target for untrusted sidecar JSON-RPC reply payloads.
#![no_main]

use core_model::{HomePage, LinkSearchUpdate, LoadDetails, PlayableLinks, SearchPage};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<SearchPage>(data);
    let _ = serde_json::from_slice::<HomePage>(data);
    let _ = serde_json::from_slice::<LoadDetails>(data);
    let _ = serde_json::from_slice::<PlayableLinks>(data);
    let _ = serde_json::from_slice::<LinkSearchUpdate>(data);
});
