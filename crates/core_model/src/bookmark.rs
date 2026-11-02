use serde::{Deserialize, Serialize};

/// A title the user saved to their library.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    /// Provider the title belongs to.
    pub provider: String,
    /// Provider URL of the title.
    pub url: String,
    /// Display title.
    pub name: String,
    /// Poster image URL.
    #[serde(default)]
    pub poster_url: Option<String>,
    /// Content kind reported by the provider.
    #[serde(default)]
    pub media_type: Option<String>,
    /// When the bookmark was added, in milliseconds since the Unix epoch.
    #[serde(default)]
    pub added_at_ms: i64,
}
