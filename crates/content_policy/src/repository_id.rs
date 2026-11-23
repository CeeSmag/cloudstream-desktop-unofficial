//! Which repository a `repo.json` address belongs to, however it is written.
//!
//! The same repository reaches viewers through different addresses: `.../builds/repo.json` and
//! `.../refs/heads/builds/repo.json` on raw GitHub, `github.com/.../raw/...`, jsDelivr's `gh` mirror.
//! Reviewed entries name the repository, not one spelling of its address.

/// A repository as `host/owner/name` for GitHub and GitLab, else `host/path`. Lower case.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepositoryId(String);

impl RepositoryId {
    /// The identity of the repository behind `url`.
    #[must_use]
    pub fn of(url: &str) -> Self {
        let rest = strip_scheme(url.trim());
        let rest = rest.split(['?', '#']).next().unwrap_or_default();
        let mut parts = rest.split('/').filter(|part| !part.is_empty());
        let host = parts.next().unwrap_or_default().to_lowercase();
        let path: Vec<&str> = parts.collect();
        Self(match host.as_str() {
            "raw.githubusercontent.com" | "github.com" => owner_and_name("github.com", &path),
            "cdn.jsdelivr.net" => jsdelivr(&path),
            "gitlab.com" => owner_and_name("gitlab.com", &path),
            _ => join(&host, &path),
        })
    }

    /// The identity as text (`github.com/owner/name`).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn strip_scheme(url: &str) -> &str {
    url.split_once("://").map_or(url, |(_, rest)| rest)
}

fn owner_and_name(host: &str, path: &[&str]) -> String {
    match path {
        [owner, name, ..] => format!("{host}/{}/{}", owner.to_lowercase(), name.to_lowercase()),
        _ => join(host, path),
    }
}

/// `cdn.jsdelivr.net/gh/<owner>/<name>@<ref>/...` is a GitHub repository.
fn jsdelivr(path: &[&str]) -> String {
    match path {
        ["gh", owner, name, ..] => {
            let name = name.split('@').next().unwrap_or_default();
            owner_and_name("github.com", &[owner, name])
        }
        _ => join("cdn.jsdelivr.net", path),
    }
}

fn join(host: &str, path: &[&str]) -> String {
    if path.is_empty() {
        host.to_owned()
    } else {
        format!("{host}/{}", path.join("/").to_lowercase())
    }
}

#[cfg(test)]
#[path = "repository_id_tests.rs"]
mod tests;
