//! T3 parity: the community "CineStream" extension (a large plugin that links against many
//! CloudStream classes: app context, key/value store, sync accounts, Cloudflare helper) must load
//! through the sidecar and answer a search like the Kotlin app would.
//!
//! Needs the real sidecar and network, so it is ignored by default. Run with:
//! `CS_SIDECAR_HOME=... CS_SIDECAR_JAVA_HOME=... CS_TEST_CINESTREAM_CS3=...\CineStream.cs3
//!  cargo test -p extension_runtime --test t3_cinestream_loads -- --ignored`
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)] // test code: failing fast is the point

use std::path::PathBuf;

use extension_api::ProviderGateway;
use extension_runtime::{RpcProviderGateway, SidecarLaunchConfig, SidecarProcessTransport};

#[test]
#[ignore = "needs the built sidecar, CineStream.cs3 and network access (see file header)"]
fn cinestream_loads_and_finds_a_well_known_title() {
    let config = SidecarLaunchConfig::from_environment().expect("CS_SIDECAR_HOME must be set");
    let gateway = RpcProviderGateway::new(SidecarProcessTransport::new(config));
    let extension = PathBuf::from(
        std::env::var("CS_TEST_CINESTREAM_CS3").expect("CS_TEST_CINESTREAM_CS3 must be set"),
    );

    let providers = gateway
        .load_extension(&extension, false)
        .expect("CineStream links against the classes the sidecar provides");
    let names: Vec<&str> = providers
        .iter()
        .map(|provider| provider.name.as_str())
        .collect();
    assert!(names.contains(&"CineStream"), "providers were {names:?}");

    let page = gateway
        .search("CineStream", "Spider-Man", 1)
        .expect("search succeeds");
    assert!(
        page.items
            .iter()
            .any(|item| item.name.contains("Spider-Man")),
        "no Spider-Man title among {} results",
        page.items.len()
    );
}
