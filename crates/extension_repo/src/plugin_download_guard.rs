//! Rejects plugin downloads that are not HTTPS, not hosted with the repository, or not a ZIP.

/// ZIP local-file header magic (`.cs3` files are ZIP archives).
const ZIP_MAGIC: &[u8] = b"PK";

/// Hosts treated as one family so GitHub repos can ship plugins via raw/CDN URLs.
const GITHUB_FAMILY: &[&str] = &[
    "github.com",
    "www.github.com",
    "raw.githubusercontent.com",
    "objects.githubusercontent.com",
    "codeload.github.com",
    "cdn.jsdelivr.net",
];

/// Why a download URL or payload was refused before it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DownloadReject {
    /// Scheme is missing or not `https`.
    NotHttps,
    /// Host could not be read from the URL.
    MissingHost,
    /// Download host is not the repository host (or an allowed sibling).
    HostMismatch,
    /// Published size does not match the downloaded bytes.
    SizeMismatch { expected: u64, actual: u64 },
    /// Payload is empty or does not look like a ZIP/`.cs3`.
    NotZipArchive,
}

impl DownloadReject {
    /// Short reason for logs and [`crate::RepoError`].
    pub(crate) fn reason(&self) -> String {
        match self {
            Self::NotHttps => "download URL must use https".into(),
            Self::MissingHost => "download URL has no host".into(),
            Self::HostMismatch => "download host does not match the repository host".into(),
            Self::SizeMismatch { expected, actual } => {
                format!("downloaded size {actual} differs from listing size {expected}")
            }
            Self::NotZipArchive => "downloaded bytes are not a ZIP archive".into(),
        }
    }
}

/// Checks that `download_url` is HTTPS and hosted with `repository_url`.
pub(crate) fn check_download_url(
    download_url: &str,
    repository_url: &str,
) -> Result<(), DownloadReject> {
    let download_host = https_host(download_url)?;
    let repository_host = https_host(repository_url)?;
    if hosts_compatible(download_host, repository_host) {
        Ok(())
    } else {
        Err(DownloadReject::HostMismatch)
    }
}

/// Checks published size (when present) and ZIP magic before the file is renamed into place.
pub(crate) fn check_download_bytes(
    bytes: &[u8],
    file_size: Option<u64>,
) -> Result<(), DownloadReject> {
    if let Some(expected) = file_size {
        let actual = bytes.len() as u64;
        if expected != actual {
            return Err(DownloadReject::SizeMismatch { expected, actual });
        }
    }
    if bytes.len() < ZIP_MAGIC.len() || !bytes.starts_with(ZIP_MAGIC) {
        return Err(DownloadReject::NotZipArchive);
    }
    Ok(())
}

fn https_host(url: &str) -> Result<&str, DownloadReject> {
    let rest = url
        .strip_prefix("https://")
        .ok_or(DownloadReject::NotHttps)?;
    let authority = rest.split('/').next().unwrap_or("");
    let host = authority.split('@').next_back().unwrap_or("");
    let host = host.split(':').next().unwrap_or("").trim();
    if host.is_empty() {
        return Err(DownloadReject::MissingHost);
    }
    Ok(host)
}

fn hosts_compatible(download: &str, repository: &str) -> bool {
    let download = download.to_ascii_lowercase();
    let repository = repository.to_ascii_lowercase();
    if download == repository {
        return true;
    }
    if is_subdomain(&download, &repository) || is_subdomain(&repository, &download) {
        return true;
    }
    in_github_family(&download) && in_github_family(&repository)
}

fn is_subdomain(host: &str, parent: &str) -> bool {
    host.len() > parent.len() + 1
        && host.ends_with(parent)
        && host.as_bytes().get(host.len() - parent.len() - 1) == Some(&b'.')
}

fn in_github_family(host: &str) -> bool {
    GITHUB_FAMILY.contains(&host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_download_url_accepts_same_https_host() {
        assert!(check_download_url(
            "https://cdn.example/Plugin.cs3",
            "https://cdn.example/repo.json"
        )
        .is_ok());
    }

    #[test]
    fn check_download_url_rejects_http() {
        assert_eq!(
            check_download_url("http://cdn.example/p.cs3", "https://cdn.example/r"),
            Err(DownloadReject::NotHttps)
        );
    }

    #[test]
    fn check_download_url_rejects_foreign_host() {
        assert_eq!(
            check_download_url(
                "https://evil.example/malware.cs3",
                "https://cdn.example/repo.json"
            ),
            Err(DownloadReject::HostMismatch)
        );
    }

    #[test]
    fn check_download_url_allows_github_family_hosts() {
        assert!(check_download_url(
            "https://raw.githubusercontent.com/org/repo/main/P.cs3",
            "https://raw.githubusercontent.com/org/repo/main/repo.json"
        )
        .is_ok());
        assert!(check_download_url(
            "https://cdn.jsdelivr.net/gh/org/repo/P.cs3",
            "https://raw.githubusercontent.com/org/repo/main/repo.json"
        )
        .is_ok());
    }

    #[test]
    fn check_download_url_allows_subdomain_of_repository_host() {
        assert!(check_download_url(
            "https://files.cdn.example/P.cs3",
            "https://cdn.example/repo.json"
        )
        .is_ok());
    }

    #[test]
    fn check_download_bytes_rejects_size_mismatch() {
        assert_eq!(
            check_download_bytes(b"PK..", Some(99)),
            Err(DownloadReject::SizeMismatch {
                expected: 99,
                actual: 4
            })
        );
    }

    #[test]
    fn check_download_bytes_rejects_non_zip() {
        assert_eq!(
            check_download_bytes(b"MZ...", None),
            Err(DownloadReject::NotZipArchive)
        );
        assert_eq!(
            check_download_bytes(b"", None),
            Err(DownloadReject::NotZipArchive)
        );
    }

    #[test]
    fn check_download_bytes_accepts_zip_magic_without_published_size() {
        assert!(check_download_bytes(b"PK\x03\x04rest", None).is_ok());
    }
}
