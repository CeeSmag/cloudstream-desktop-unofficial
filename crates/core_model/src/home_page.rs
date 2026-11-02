use serde::{Deserialize, Serialize};

use crate::SearchResult;

/// A titled row of items on a provider home page.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomePageSection {
    /// Row title.
    pub name: String,
    /// Items in the row.
    #[serde(default)]
    pub list: Vec<SearchResult>,
    /// Whether the row uses wide artwork.
    #[serde(default)]
    pub is_horizontal_images: bool,
}

/// A provider home page (one page of sections).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomePage {
    /// Rows on this page.
    #[serde(default)]
    pub items: Vec<HomePageSection>,
    /// Whether another page can be requested.
    #[serde(default)]
    pub has_next: bool,
}
