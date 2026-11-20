//! Fuzz target for untrusted repository manifests and plugin listings.
#![no_main]

use core_model::PluginListing;
use extension_repo::RepositoryManifest;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<RepositoryManifest>(data);
    let _ = serde_json::from_slice::<Vec<PluginListing>>(data);
});
