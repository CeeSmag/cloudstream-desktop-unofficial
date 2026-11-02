use serde::{Deserialize, Serialize};

/// An extension file the user installed.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledExtension {
    /// File name inside the extensions directory (unique key).
    pub file_name: String,
    /// Internal plugin name from the repository listing.
    pub internal_name: String,
    /// Repository the extension came from, if any.
    #[serde(default)]
    pub repository_url: Option<String>,
    /// Version number from the repository listing.
    #[serde(default)]
    pub version: i64,
    /// Whether the user trusts the extension to run with full library access.
    #[serde(default)]
    pub trusted: bool,
}
