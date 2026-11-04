use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

/// A person in the cast of a title.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CastMember {
    /// Name of the person.
    pub name: String,
    /// Photo.
    pub image: Option<String>,
    /// Main, supporting or background, when the provider says.
    pub role: Option<String>,
}

/// How providers send one cast entry: the person nested inside the entry.
#[derive(Deserialize)]
struct RawCastMember {
    actor: RawActor,
    #[serde(default)]
    role: Option<String>,
}

#[derive(Deserialize)]
struct RawActor {
    #[serde(default)]
    name: String,
    #[serde(default)]
    image: Option<String>,
}

fn cast_list<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<CastMember>, D::Error> {
    let raw = Option::<Vec<RawCastMember>>::deserialize(deserializer)?;
    Ok(raw
        .unwrap_or_default()
        .into_iter()
        .map(|entry| CastMember {
            name: entry.actor.name,
            image: entry.actor.image,
            role: entry.role,
        })
        .collect())
}

/// The ids a provider reports for a title in other services, keyed by service. Values that are
/// not text are left out rather than failing the whole title.
fn text_map<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Value {
        Text(String),
        Other(serde::de::IgnoredAny),
    }
    let raw = Option::<BTreeMap<String, Value>>::deserialize(deserializer)?;
    Ok(raw
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(key, value)| match value {
            Value::Text(text) => Some((key, text)),
            Value::Other(_) => None,
        })
        .collect())
}

/// Providers send episodes either as one list or, for anime, grouped by language
/// (`{"Subbed": [...], "Dubbed": [...]}`). The same episodes repeat in every group, so the first
/// group that has any is used.
fn episodes_list_or_groups<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<EpisodeInfo>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Shape {
        List(Vec<EpisodeInfo>),
        Groups(BTreeMap<String, Vec<EpisodeInfo>>),
    }
    Ok(match Option::<Shape>::deserialize(deserializer)? {
        None => Vec::new(),
        Some(Shape::List(list)) => list,
        Some(Shape::Groups(groups)) => groups
            .into_values()
            .find(|list| !list.is_empty())
            .unwrap_or_default(),
    })
}

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
    #[serde(default, deserialize_with = "episodes_list_or_groups")]
    pub episodes: Vec<EpisodeInfo>,
    /// Wide background picture.
    #[serde(default)]
    pub background_poster_url: Option<String>,
    /// Rating from 0 to 10000 (7.5 out of 10 is 7500).
    #[serde(default)]
    pub rating: Option<i32>,
    /// Running time in minutes.
    #[serde(default)]
    pub duration: Option<i32>,
    /// The cast.
    #[serde(default, deserialize_with = "cast_list")]
    pub actors: Vec<CastMember>,
    /// Ids of the title in other services (IMDb, TMDb, MyAnimeList), as the provider sends them.
    #[serde(default, deserialize_with = "text_map")]
    pub sync_data: BTreeMap<String, String>,
    /// A JSON text identifying the title across providers, such as `{"id":"tt1","type":"movie"}`.
    #[serde(default)]
    pub unique_url: Option<String>,
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

    #[test]
    fn a_series_with_grouped_episodes_nulls_and_nested_cast_is_read() {
        let json = r#"{"name":"Dead City","type":"TvSeries","rating":7000,
            "backgroundPosterUrl":"https://bg","duration":null,"recommendations":null,
            "episodes":{"Dubbed":[],"Subbed":[{"data":"d1","season":1,"episode":1},
                                              {"data":"d2","season":1,"episode":2}]},
            "actors":[{"actor":{"name":"Lauren Cohan","image":"https://p"},"role":null}]}"#;
        let details: LoadDetails = serde_json::from_str(json).expect("valid series JSON");
        assert_eq!(details.episodes.len(), 2, "the first non-empty group");
        assert_eq!(details.rating, Some(7000));
        assert_eq!(details.background_poster_url.as_deref(), Some("https://bg"));
        assert_eq!(details.actors[0].name, "Lauren Cohan");
        assert_eq!(details.actors[0].image.as_deref(), Some("https://p"));
    }

    #[test]
    fn the_ids_a_provider_reports_are_kept_and_odd_values_do_not_break_the_title() {
        let json = r#"{"name":"X","syncData":{"":"{\"Imdb\":\"tt18546730\"}","n":5,"z":null},
            "uniqueUrl":"{\"id\":\"tt18546730\",\"type\":\"series\"}"}"#;
        let details: LoadDetails = serde_json::from_str(json).expect("valid JSON");
        assert_eq!(details.sync_data.len(), 1);
        assert_eq!(details.sync_data[""], r#"{"Imdb":"tt18546730"}"#);
        assert!(details
            .unique_url
            .as_deref()
            .is_some_and(|u| u.contains("tt18546730")));
        let none: LoadDetails =
            serde_json::from_str(r#"{"name":"X","syncData":null}"#).expect("ok");
        assert!(none.sync_data.is_empty() && none.unique_url.is_none());
    }

    #[test]
    fn null_episodes_and_actors_mean_none() {
        let json = r#"{"name":"X","episodes":null,"actors":null}"#;
        let details: LoadDetails = serde_json::from_str(json).expect("valid JSON");
        assert!(details.episodes.is_empty() && details.actors.is_empty());
    }
}
