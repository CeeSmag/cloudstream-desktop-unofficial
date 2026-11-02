use serde::{Deserialize, Serialize};

/// One episode of a series, as reported by a provider.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeInfo {
    /// Provider payload passed to link loading when this episode is played.
    #[serde(default)]
    pub data: String,
    /// Episode title.
    #[serde(default)]
    pub name: Option<String>,
    /// Season number.
    #[serde(default)]
    pub season: Option<i32>,
    /// Episode number within the season.
    #[serde(default)]
    pub episode: Option<i32>,
    /// Episode artwork.
    #[serde(default)]
    pub poster_url: Option<String>,
    /// Episode synopsis.
    #[serde(default)]
    pub description: Option<String>,
}

/// Full details of a title (movie, series, ...) loaded from a provider.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadDetails {
    /// Title.
    pub name: String,
    /// Provider URL of the title.
    #[serde(default)]
    pub url: String,
    /// Provider name.
    #[serde(default)]
    pub api_name: String,
    /// Content kind reported by the provider.
    #[serde(default, rename = "type")]
    pub media_type: Option<String>,
    /// Provider payload passed to link loading for movie-like titles.
    #[serde(default)]
    pub data_url: Option<String>,
    /// Poster image URL.
    #[serde(default)]
    pub poster_url: Option<String>,
    /// Release year.
    #[serde(default)]
    pub year: Option<i32>,
    /// Synopsis.
    #[serde(default)]
    pub plot: Option<String>,
    /// Genre tags.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// Episodes, for series-like titles.
    #[serde(default)]
    pub episodes: Vec<EpisodeInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_movie_details_reads_data_url_and_tags() {
        let json = r#"{"name":"Apollo","url":"u","apiName":"Archive","type":"Movie",
            "dataUrl":"ID123","year":2019,"plot":"p","tags":["Space"],"score":null}"#;
        let details: LoadDetails = serde_json::from_str(json).expect("valid load JSON");
        assert_eq!(details.data_url.as_deref(), Some("ID123"));
        assert_eq!(details.tags, Some(vec!["Space".to_owned()]));
        assert!(details.episodes.is_empty());
    }
}
