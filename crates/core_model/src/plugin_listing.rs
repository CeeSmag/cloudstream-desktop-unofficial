use serde::{Deserialize, Serialize};

/// One downloadable extension in a repository's plugin list.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginListing {
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// Stable internal name; also used for the downloaded file name.
    pub internal_name: String,
    /// Direct download URL of the `.cs3` file.
    pub url: String,
    /// Version number; higher means newer.
    #[serde(default)]
    pub version: i64,
    /// Size in bytes as published by the repository, when given.
    #[serde(default)]
    pub file_size: Option<u64>,
    /// Repository status flag: `1` means working, `0` down, `2` slow, `3` beta.
    #[serde(default)]
    pub status: Option<i32>,
    /// Language code of the content.
    #[serde(default)]
    pub language: Option<String>,
    /// Content types the plugin provides.
    #[serde(default)]
    pub tv_types: Vec<String>,
    /// Short description.
    #[serde(default)]
    pub description: Option<String>,
    /// Plugin authors.
    #[serde(default)]
    pub authors: Vec<String>,
    /// Icon URL.
    #[serde(default)]
    pub icon_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_listing_from_official_plugins_json_entry() {
        let json = r#"{"iconUrl":"https://i/x.png","apiVersion":1,"repositoryUrl":"https://github.com/r",
            "fileSize":28037,"status":1,"language":"en","authors":["a"],"tvTypes":["Others"],
            "version":1,"internalName":"InternetArchiveProvider","description":null,
            "url":"https://r/InternetArchiveProvider.cs3","name":"InternetArchiveProvider"}"#;
        let listing: PluginListing = serde_json::from_str(json).expect("valid listing");
        assert_eq!(listing.internal_name, "InternetArchiveProvider");
        assert_eq!(listing.file_size, Some(28037));
        assert_eq!(listing.status, Some(1));
    }
}
