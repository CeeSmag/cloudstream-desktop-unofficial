//! T3 parity: the converted official Internet Archive extension, driven through the sidecar,
//! must return what a direct call to archive.org returns (what the Kotlin app would show).
//!
//! Needs the real sidecar, so it is ignored by default. Run with:
//! `CS_SIDECAR_HOME=... CS_SIDECAR_JAVA_HOME=... CS_TEST_IA_CS3=...\InternetArchiveProvider.cs3
//!  cargo test -p extension_runtime --test t3_parity_internet_archive -- --ignored`
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)] // test code: failing fast is the point

use std::path::PathBuf;

use extension_api::ProviderGateway;
use extension_runtime::{RpcProviderGateway, SidecarLaunchConfig, SidecarProcessTransport};
use serde_json::Value;

const PROVIDER: &str = "Internet Archive";
const QUERY: &str = "moon landing";
const ATTEMPTS: usize = 3;

fn gateway() -> RpcProviderGateway<SidecarProcessTransport> {
    let config = SidecarLaunchConfig::from_environment().expect("CS_SIDECAR_HOME must be set");
    let gateway = RpcProviderGateway::new(SidecarProcessTransport::new(config));
    let extension =
        PathBuf::from(std::env::var("CS_TEST_IA_CS3").expect("CS_TEST_IA_CS3 must be set"));
    let providers = gateway
        .load_extension(&extension, true)
        .expect("extension loads");
    assert!(providers.iter().any(|provider| provider.name == PROVIDER));
    gateway
}

fn fetch_json(request: ureq::RequestBuilder<ureq::typestate::WithoutBody>) -> Value {
    request
        .call()
        .expect("archive.org reachable")
        .into_body()
        .read_json()
        .expect("JSON body")
}

/// The same request the Kotlin provider source makes (InternetArchiveProvider.search).
fn reference_search_identifiers() -> Vec<String> {
    let request = ureq::get("https://archive.org/advancedsearch.php")
        .query("q", format!("{QUERY} mediatype:(movies OR audio)"))
        .query("fl[]", "identifier")
        .query("fl[]", "title")
        .query("fl[]", "mediatype")
        .query("rows", "26")
        .query("page", "1")
        .query("output", "json");
    fetch_json(request)["response"]["docs"]
        .as_array()
        .expect("docs array")
        .iter()
        .filter_map(|doc| doc["identifier"].as_str().map(str::to_owned))
        .collect()
}

#[test]
#[ignore = "needs the built sidecar and network access (see file header)"]
fn search_results_match_a_direct_archive_org_query() {
    let gateway = gateway();
    let reference = reference_search_identifiers();
    let through_sidecar: Vec<String> = gateway
        .search(PROVIDER, QUERY, 1)
        .expect("search succeeds")
        .items
        .iter()
        .map(|item| item.url.rsplit('/').next().unwrap_or_default().to_owned())
        .collect();

    assert!(!reference.is_empty(), "reference query returned nothing");
    let overlap = reference
        .iter()
        .filter(|id| through_sidecar.contains(id))
        .count();
    // Rankings can shift between two live requests; the result sets must still be near-identical.
    assert!(
        overlap * 10 >= reference.len() * 9,
        "only {overlap}/{} identifiers match\nreference: {reference:?}\nsidecar: {through_sidecar:?}",
        reference.len()
    );
}

/// True when the stream answers a one-byte range request (some archive items are access-restricted).
fn stream_is_reachable(url: &str) -> bool {
    ureq::get(url)
        .header("Range", "bytes=0-0")
        .call()
        .map(|response| matches!(response.status().as_u16(), 200 | 206))
        .unwrap_or(false)
}

fn resolved_links(
    gateway: &impl ProviderGateway,
    details_url: &str,
) -> Vec<core_model::PlayableLink> {
    (0..ATTEMPTS)
        .filter_map(|_| gateway.extract(details_url, None).ok())
        .find(|resolved| !resolved.links.is_empty())
        .map(|resolved| resolved.links)
        .unwrap_or_default()
}

#[test]
#[ignore = "needs the built sidecar and network access (see file header)"]
fn load_title_matches_archive_org_metadata() {
    let gateway = gateway();
    let identifier = reference_search_identifiers()
        .into_iter()
        .next()
        .expect("a search hit");
    let metadata = fetch_json(ureq::get(format!(
        "https://archive.org/metadata/{identifier}"
    )));
    let reference_title = metadata["metadata"]["title"]
        .as_str()
        .expect("title")
        .to_owned();

    let details = gateway
        .load(
            PROVIDER,
            &format!("https://archive.org/details/{identifier}"),
        )
        .expect("load succeeds")
        .expect("provider returns details");
    assert_eq!(details.name, reference_title);
}

#[test]
#[ignore = "needs the built sidecar and network access (see file header)"]
fn resolved_streams_point_into_the_item_and_at_least_one_plays() {
    let gateway = gateway();
    let identifiers: Vec<String> = reference_search_identifiers().into_iter().take(5).collect();
    let mut playable = 0;
    for identifier in &identifiers {
        let links = resolved_links(
            &gateway,
            &format!("https://archive.org/details/{identifier}"),
        );
        for link in &links {
            assert!(link
                .url
                .starts_with(&format!("https://archive.org/download/{identifier}/")));
        }
        if links.iter().any(|link| stream_is_reachable(&link.url)) {
            playable += 1;
        }
    }
    assert!(playable >= 1, "no playable stream among {identifiers:?}");
}
