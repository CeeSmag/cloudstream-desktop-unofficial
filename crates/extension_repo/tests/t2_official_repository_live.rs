//! T2 against the real official repository (network). Run with `-- --ignored`.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)] // test code: failing fast is the point

use extension_repo::{RepositoryClient, UreqFetcher};

const OFFICIAL_REPO: &str =
    "https://raw.githubusercontent.com/recloudstream/extensions/master/repo.json";

#[test]
#[ignore = "needs network access"]
fn official_repository_lists_installable_plugins() {
    let client = RepositoryClient::new(UreqFetcher::default());
    let manifest = client
        .fetch_manifest(OFFICIAL_REPO)
        .expect("manifest downloads and parses");
    let plugins = client.fetch_plugins(&manifest);
    assert!(
        plugins
            .iter()
            .any(|plugin| plugin.internal_name == "InternetArchiveProvider"),
        "got {:?}",
        plugins
            .iter()
            .map(|plugin| &plugin.internal_name)
            .collect::<Vec<_>>()
    );
}
