//! The ids a provider reports for a title, checked so they are safe to put in a URL.

use std::collections::BTreeMap;

use serde_json::Value;

/// The ids of a title in the databases. Only well-formed ids are kept, so a provider cannot
/// smuggle anything else into a request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ExternalIds {
    /// IMDb id such as `tt18546730`.
    pub imdb: Option<String>,
    /// TheMovieDB id, digits only.
    pub tmdb: Option<String>,
    /// MyAnimeList id, digits only.
    pub mal: Option<String>,
}

fn is_imdb(text: &str) -> bool {
    text.strip_prefix("tt").is_some_and(|digits| {
        (5..=12).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit())
    })
}

fn is_number(text: &str) -> bool {
    (1..=10).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit())
}

impl ExternalIds {
    /// Ids from three possibly absent texts; anything malformed is dropped.
    pub fn new(imdb: Option<&str>, tmdb: Option<&str>, mal: Option<&str>) -> Self {
        let keep = |text: Option<&str>, valid: fn(&str) -> bool| {
            text.map(str::trim).filter(|t| valid(t)).map(str::to_owned)
        };
        Self {
            imdb: keep(imdb, is_imdb),
            tmdb: keep(tmdb, is_number),
            mal: keep(mal, is_number),
        }
    }

    /// Whether no usable id is known.
    pub fn is_empty(&self) -> bool {
        self.imdb.is_none() && self.tmdb.is_none() && self.mal.is_none()
    }

    /// Reads the ids out of what a provider reports with a title: `sync_data` maps a service to
    /// its id (or, as CineStream sends it, to a JSON text such as `{"Imdb":"tt1"}`), and
    /// `unique_url` is a JSON text with an `id`.
    pub fn from_sync(sync_data: &BTreeMap<String, String>, unique_url: Option<&str>) -> Self {
        let mut found: [Option<String>; 3] = [None, None, None];
        for (key, value) in sync_data {
            match serde_json::from_str::<Value>(value) {
                Ok(Value::Object(object)) => {
                    for (name, id) in &object {
                        take_named(&mut found, name, &id_text(id));
                    }
                }
                _ => take_named(&mut found, key, value),
            }
        }
        if let Some(id) = unique_url.and_then(unique_id) {
            take_named(&mut found, "imdb", &id);
        }
        let [imdb, tmdb, mal] = found;
        Self::new(imdb.as_deref(), tmdb.as_deref(), mal.as_deref())
    }
}

/// Puts `id` in the slot `name` stands for (imdb, tmdb, mal) unless that slot is taken.
fn take_named(found: &mut [Option<String>; 3], name: &str, id: &str) {
    let name = name.to_ascii_lowercase();
    let slot = if name.contains("imdb") {
        0
    } else if name.contains("tmdb") || name.contains("themoviedb") {
        1
    } else if name.starts_with("mal") || name.contains("myanimelist") {
        2
    } else {
        return;
    };
    if let Some(place) = found.get_mut(slot) {
        place.get_or_insert_with(|| id.trim().to_owned());
    }
}

fn id_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        _ => String::new(),
    }
}

fn unique_id(unique_url: &str) -> Option<String> {
    let value: Value = serde_json::from_str(unique_url).ok()?;
    value.get("id").map(id_text)
}

#[cfg(test)]
#[path = "external_ids_tests.rs"]
mod tests;
