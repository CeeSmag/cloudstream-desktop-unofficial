use serde::{Deserialize, Serialize};

/// An extension repository the user added.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryEntry {
    /// URL of the repository's `repo.json`.
    pub url: String,
    /// Repository display name.
    pub name: String,
    /// When the repository was added, in milliseconds since the Unix epoch.
    #[serde(default)]
    pub added_at_ms: i64,
}
