use core_model::PluginListing;

use crate::{HttpFetcher, RepoError, RepositoryManifest};

/// Reads repository manifests and plugin lists.
pub struct RepositoryClient<F: HttpFetcher> {
    fetcher: F,
}

impl<F: HttpFetcher> RepositoryClient<F> {
    /// Wraps a fetcher.
    pub fn new(fetcher: F) -> Self {
        Self { fetcher }
    }

    /// Downloads and parses a repository's `repo.json`.
    pub fn fetch_manifest(&self, repo_url: &str) -> Result<RepositoryManifest, RepoError> {
        let bytes = self.fetcher.get_bytes(repo_url)?;
        serde_json::from_slice(&bytes).map_err(|error| RepoError::Invalid {
            url: repo_url.to_owned(),
            reason: error.to_string(),
        })
    }

    /// Lists every plugin of a repository.
    ///
    /// A plugin list that cannot be fetched or parsed is logged and skipped, so one broken
    /// list does not hide the plugins of the others.
    pub fn fetch_plugins(&self, manifest: &RepositoryManifest) -> Vec<PluginListing> {
        manifest
            .plugin_lists
            .iter()
            .flat_map(|list_url| match self.fetch_plugin_list(list_url) {
                Ok(listings) => listings,
                Err(error) => {
                    tracing::warn!(%error, list_url, "skipping unreadable plugin list");
                    Vec::new()
                }
            })
            .collect()
    }

    fn fetch_plugin_list(&self, list_url: &str) -> Result<Vec<PluginListing>, RepoError> {
        let bytes = self.fetcher.get_bytes(list_url)?;
        serde_json::from_slice(&bytes).map_err(|error| RepoError::Invalid {
            url: list_url.to_owned(),
            reason: error.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    struct FakeFetcher(HashMap<String, Vec<u8>>);

    impl HttpFetcher for FakeFetcher {
        fn get_bytes(&self, url: &str) -> Result<Vec<u8>, RepoError> {
            self.0.get(url).cloned().ok_or_else(|| RepoError::Download {
                url: url.to_owned(),
                reason: "404".to_owned(),
            })
        }
    }

    fn client(files: &[(&str, &str)]) -> RepositoryClient<FakeFetcher> {
        let map = files
            .iter()
            .map(|(u, b)| ((*u).to_owned(), b.as_bytes().to_vec()))
            .collect();
        RepositoryClient::new(FakeFetcher(map))
    }

    #[test]
    fn fetch_plugins_merges_lists_and_skips_broken_ones() {
        let client = client(&[
            ("good", r#"[{"internalName":"A","url":"u1"}]"#),
            ("garbage", "not json"),
        ]);
        let manifest = RepositoryManifest {
            name: "r".into(),
            plugin_lists: vec!["good".into(), "garbage".into(), "missing".into()],
            ..RepositoryManifest::default()
        };
        let plugins = client.fetch_plugins(&manifest);
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].internal_name, "A");
    }

    #[test]
    fn fetch_manifest_rejects_invalid_json_with_url_in_error() {
        let error = client(&[("repo", "[]")])
            .fetch_manifest("repo")
            .expect_err("not a manifest");
        assert!(matches!(error, RepoError::Invalid { url, .. } if url == "repo"));
    }

    #[test]
    fn fetch_manifest_reports_download_failures() {
        let error = client(&[]).fetch_manifest("gone").expect_err("missing");
        assert!(matches!(error, RepoError::Download { .. }));
    }
}
