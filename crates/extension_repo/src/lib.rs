//! Reads extension repositories and downloads plugin files.
#![forbid(unsafe_code)]

mod extension_installer;
mod http_fetcher;
mod repo_error;
mod repository_client;
mod repository_manifest;
mod safe_file_name;

pub use extension_installer::ExtensionInstaller;
pub use http_fetcher::{HttpFetcher, UreqFetcher};
pub use repo_error::RepoError;
pub use repository_client::RepositoryClient;
pub use repository_manifest::RepositoryManifest;
