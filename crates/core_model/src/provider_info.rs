use serde::{Deserialize, Serialize};

/// A home page category a provider offers.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MainPageEntry {
    /// Category title.
    pub name: String,
    /// Provider-specific payload passed back when requesting this category.
    #[serde(default)]
    pub data: String,
    /// Whether the category uses wide artwork.
    #[serde(default)]
    pub horizontal_images: bool,
}

/// Description of a provider registered by a loaded extension.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    /// Unique provider name, used to address it in later calls.
    pub name: String,
    /// Base URL of the site the provider scrapes.
    #[serde(default)]
    pub main_url: String,
    /// Language code of the provider's content.
    #[serde(default)]
    pub lang: String,
    /// Content types the provider supports.
    #[serde(default)]
    pub types: Vec<String>,
    /// Whether the provider has a home page.
    #[serde(default)]
    pub has_main_page: bool,
    /// Home page categories.
    #[serde(default)]
    pub main_page: Vec<MainPageEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_provider_info_reads_main_page_entries() {
        let json = r#"{"name":"Internet Archive","mainUrl":"https://archive.org","lang":"en",
            "types":["Others"],"hasMainPage":true,
            "mainPage":[{"name":"","data":"","horizontalImages":false}]}"#;
        let info: ProviderInfo = serde_json::from_str(json).expect("valid provider JSON");
        assert_eq!(info.name, "Internet Archive");
        assert!(info.has_main_page);
        assert_eq!(info.main_page.len(), 1);
    }
}
