use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// One entry returned by a provider search or listed on a provider home page.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    /// Display title shown in result lists.
    pub name: String,
    /// Provider-specific URL used to load the full details page.
    pub url: String,
    /// Name of the provider that produced this entry.
    #[serde(default)]
    pub api_name: String,
    /// Content kind reported by the provider (for example `Movie`, `TvSeries`).
    #[serde(default, rename = "type")]
    pub media_type: Option<String>,
    /// Poster image URL, if the provider has one.
    #[serde(default)]
    pub poster_url: Option<String>,
    /// Release year, if known.
    #[serde(default)]
    pub year: Option<i32>,
    /// Extra HTTP headers needed to download the poster.
    #[serde(default)]
    pub poster_headers: Option<HashMap<String, String>>,
}

/// One page of search results.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPage {
    /// Results on this page.
    #[serde(default)]
    pub items: Vec<SearchResult>,
    /// Whether another page can be requested.
    #[serde(default)]
    pub has_next: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_search_page_from_sidecar_reply_keeps_known_fields() {
        let json = r#"{"items":[{"name":"Apollo","url":"https://a.test/1","apiName":"Archive",
            "type":"Movie","posterUrl":"https://a.test/p.jpg","year":null,"id":null,"score":null}],
            "hasNext":true}"#;
        let page: SearchPage = serde_json::from_str(json).expect("valid sidecar JSON");
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].media_type.as_deref(), Some("Movie"));
        assert!(page.has_next);
    }

    #[test]
    fn deserialize_search_page_tolerates_missing_optional_fields() {
        let page: SearchPage = serde_json::from_str(r#"{"items":[{"name":"x","url":"u"}]}"#)
            .expect("optional fields may be absent");
        assert_eq!(page.items[0].api_name, "");
        assert!(!page.has_next);
    }
}
