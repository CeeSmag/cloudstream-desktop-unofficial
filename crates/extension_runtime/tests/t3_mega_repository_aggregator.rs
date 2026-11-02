//! T3 parity: the "Mega" extension adds every verified CloudStream repository when it loads.
//! Through the sidecar it must hand the app the same repositories the public list contains.
//!
//! Needs the real sidecar and network, so it is ignored by default. Run with:
//! `CS_SIDECAR_HOME=... CS_SIDECAR_JAVA_HOME=... CS_TEST_MEGA_CS3=...\MegaProvider.cs3
//!  cargo test -p extension_runtime --test t3_mega_repository_aggregator -- --ignored`
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)] // test code: failing fast is the point

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use extension_api::ProviderGateway;
use extension_runtime::{RpcProviderGateway, SidecarLaunchConfig, SidecarProcessTransport};
use serde_json::Value;

/// The list the Mega plugin reads (its own source names this address).
const REPOS_DB: &str =
    "https://raw.githubusercontent.com/recloudstream/cs-repos/master/repos-db.json";
/// How long the plugin gets to read every repository manifest.
const COLLECT_FOR: Duration = Duration::from_secs(150);
/// Collecting stops once this long passes with nothing new.
const QUIET_FOR: Duration = Duration::from_secs(30);

/// The repository addresses in `repos-db.json`; entries are plain strings or `{ "url": ... }`.
fn reference_urls() -> HashSet<String> {
    let list: Value = ureq::get(REPOS_DB)
        .call()
        .expect("repository list reachable")
        .into_body()
        .read_json()
        .expect("JSON body");
    list.as_array()
        .expect("a list")
        .iter()
        .filter_map(|entry| {
            entry
                .as_str()
                .or_else(|| entry["url"].as_str())
                .map(str::to_owned)
        })
        .collect()
}

#[test]
#[ignore = "needs the built sidecar and network access (see file header)"]
fn loading_mega_hands_over_every_repository_of_the_public_list() {
    let config = SidecarLaunchConfig::from_environment().expect("CS_SIDECAR_HOME must be set");
    let gateway = RpcProviderGateway::new(SidecarProcessTransport::new(config));
    let extension =
        PathBuf::from(std::env::var("CS_TEST_MEGA_CS3").expect("CS_TEST_MEGA_CS3 must be set"));
    gateway
        .load_extension(&extension, false)
        .expect("the extension loads even though it offers no provider");

    let started = Instant::now();
    let mut last_new = Instant::now();
    let mut collected = HashSet::new();
    while started.elapsed() < COLLECT_FOR && last_new.elapsed() < QUIET_FOR {
        std::thread::sleep(Duration::from_secs(2));
        for repository in gateway.take_added_repositories().expect("call succeeds") {
            if collected.insert(repository.url) {
                last_new = Instant::now();
            }
        }
    }

    let expected = reference_urls();
    let missing: Vec<&String> = expected.difference(&collected).collect();
    assert!(
        missing.is_empty(),
        "{} of {} repositories never arrived, for example {:?}",
        missing.len(),
        expected.len(),
        missing.iter().take(3).collect::<Vec<_>>()
    );
    assert!(
        gateway
            .take_added_repositories()
            .expect("call succeeds")
            .is_empty(),
        "a repository was handed over twice"
    );
}
