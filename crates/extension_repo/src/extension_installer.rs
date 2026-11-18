use std::fs;
use std::path::{Path, PathBuf};

use core_model::PluginListing;

use crate::plugin_download_guard::{check_download_bytes, check_download_url};
use crate::safe_file_name::plugin_file_name;
use crate::{HttpFetcher, RepoError};

/// Downloads plugin files into an extensions folder and removes them again.
pub struct ExtensionInstaller<F: HttpFetcher> {
    fetcher: F,
    extensions_dir: PathBuf,
}

impl<F: HttpFetcher> ExtensionInstaller<F> {
    /// Creates an installer that writes into `extensions_dir`.
    pub fn new(fetcher: F, extensions_dir: PathBuf) -> Self {
        Self {
            fetcher,
            extensions_dir,
        }
    }

    /// Folder the plugins are stored in.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn extensions_dir(&self) -> &Path {
        &self.extensions_dir
    }

    /// Downloads a plugin and returns the path of the installed file.
    ///
    /// The file is written to a temporary name first so a failed download never leaves a
    /// half-written plugin behind. The download URL must be HTTPS and hosted with
    /// `repository_url`; the bytes must look like a ZIP and match any published size.
    #[tracing::instrument(level = "debug", skip_all, fields(repository_url = ?repository_url), err)]
    pub fn install(
        &self,
        listing: &PluginListing,
        repository_url: &str,
    ) -> Result<PathBuf, RepoError> {
        let file_name = plugin_file_name(&listing.internal_name)?;
        check_download_url(&listing.url, repository_url).map_err(|reject| RepoError::Rejected {
            url: listing.url.clone(),
            reason: reject.reason(),
        })?;
        let bytes = self.fetcher.get_bytes(&listing.url)?;
        check_download_bytes(&bytes, listing.file_size).map_err(|reject| RepoError::Rejected {
            url: listing.url.clone(),
            reason: reject.reason(),
        })?;
        self.write_plugin_file(&file_name, &bytes, &listing.internal_name)
    }

    /// Deletes an installed plugin file; a file that is already gone counts as success.
    #[tracing::instrument(level = "debug", skip_all, fields(internal_name = ?internal_name), err)]
    pub fn uninstall(&self, internal_name: &str) -> Result<(), RepoError> {
        let path = self.extensions_dir.join(plugin_file_name(internal_name)?);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(RepoError::Disk { path, source }),
        }
    }

    fn write_plugin_file(
        &self,
        file_name: &str,
        bytes: &[u8],
        plugin: &str,
    ) -> Result<PathBuf, RepoError> {
        let disk = |path: &Path| {
            let path = path.to_path_buf();
            move |source| RepoError::Disk { path, source }
        };
        fs::create_dir_all(&self.extensions_dir).map_err(disk(&self.extensions_dir))?;
        let final_path = self.extensions_dir.join(file_name);
        let temp_path = final_path.with_extension("cs3.part");
        fs::write(&temp_path, bytes).map_err(disk(&temp_path))?;
        fs::rename(&temp_path, &final_path).map_err(disk(&final_path))?;
        tracing::info!(plugin, path = %final_path.display(), "installed plugin");
        Ok(final_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StaticFetcher(Result<Vec<u8>, ()>);

    impl HttpFetcher for StaticFetcher {
        fn get_bytes(&self, url: &str) -> Result<Vec<u8>, RepoError> {
            self.0.clone().map_err(|()| RepoError::Download {
                url: url.to_owned(),
                reason: "boom".into(),
            })
        }
    }

    fn listing(name: &str) -> PluginListing {
        PluginListing {
            internal_name: name.to_owned(),
            url: "https://cdn.example/x.cs3".to_owned(),
            ..PluginListing::default()
        }
    }

    const REPO: &str = "https://cdn.example/repo.json";

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cs_repo_test_{label}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn install_writes_the_downloaded_bytes_and_leaves_no_temp_file() {
        let dir = temp_dir("install");
        let installer = ExtensionInstaller::new(StaticFetcher(Ok(b"PK".to_vec())), dir.clone());
        let path = installer.install(&listing("Good"), REPO).expect("installs");
        assert_eq!(fs::read(&path).expect("read"), b"PK");
        assert!(!dir.join("Good.cs3.part").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn failed_download_leaves_nothing_behind() {
        let dir = temp_dir("fail");
        let installer = ExtensionInstaller::new(StaticFetcher(Err(())), dir.clone());
        assert!(installer.install(&listing("Bad"), REPO).is_err());
        assert!(!dir.join("Bad.cs3").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn install_refuses_unsafe_names_before_downloading() {
        let installer =
            ExtensionInstaller::new(StaticFetcher(Ok(b"PK".to_vec())), temp_dir("unsafe"));
        assert!(matches!(
            installer.install(&listing("../x"), REPO),
            Err(RepoError::UnsafeName(_))
        ));
    }

    #[test]
    fn install_rejects_foreign_download_host_before_fetch() {
        let installer =
            ExtensionInstaller::new(StaticFetcher(Ok(b"PK".to_vec())), temp_dir("foreign"));
        let mut listing = listing("Evil");
        listing.url = "https://evil.example/x.cs3".into();
        assert!(matches!(
            installer.install(&listing, REPO),
            Err(RepoError::Rejected { .. })
        ));
    }

    #[test]
    fn install_rejects_size_mismatch_and_non_zip() {
        let dir = temp_dir("payload");
        let mut sized = listing("Sized");
        sized.file_size = Some(99);
        let installer = ExtensionInstaller::new(StaticFetcher(Ok(b"PK".to_vec())), dir.clone());
        assert!(matches!(
            installer.install(&sized, REPO),
            Err(RepoError::Rejected { .. })
        ));
        let installer = ExtensionInstaller::new(StaticFetcher(Ok(b"MZ".to_vec())), dir.clone());
        assert!(matches!(
            installer.install(&listing("BadZip"), REPO),
            Err(RepoError::Rejected { .. })
        ));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn uninstall_removes_file_and_tolerates_missing_file() {
        let dir = temp_dir("uninstall");
        let installer = ExtensionInstaller::new(StaticFetcher(Ok(b"PK".to_vec())), dir.clone());
        installer.install(&listing("Gone"), REPO).expect("installs");
        installer.uninstall("Gone").expect("removes");
        installer.uninstall("Gone").expect("already gone is fine");
        let _ = fs::remove_dir_all(dir);
    }
}
