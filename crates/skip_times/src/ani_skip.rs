//! AniSkip (<https://api.aniskip.com>): opening, ending and recap of anime episodes, by
//! MyAnimeList id. The reference app asks it first for anime.

use serde::Deserialize;

use crate::skip_fetcher::{SkipError, SkipFetcher};
use crate::skip_segment::{SkipKind, SkipQuery, SkipSegment, TitleKind};
use crate::to_ms::seconds_to_ms;

const BASE: &str = "https://api.aniskip.com/v2/skip-times";
/// `types[]=...` for every kind asked for, with the brackets escaped.
const TYPES: &str =
    "types%5B%5D=ed&types%5B%5D=mixed-ed&types%5B%5D=mixed-op&types%5B%5D=op&types%5B%5D=recap";

#[derive(Deserialize, Default)]
struct Reply {
    results: Option<Vec<Item>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    interval: Interval,
    skip_type: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Interval {
    start_time: f64,
    end_time: f64,
}

fn url(query: &SkipQuery) -> Option<String> {
    if query.kind != TitleKind::Anime {
        return None;
    }
    Some(format!(
        "{BASE}/{}/{}?{TYPES}&episodeLength={}",
        query.ids.mal.as_deref()?,
        query.episode?,
        query.duration_ms / 1000
    ))
}

/// The segments AniSkip knows for the query; empty when it knows none or cannot be asked.
pub(crate) fn segments(
    fetcher: &dyn SkipFetcher,
    query: &SkipQuery,
) -> Result<Vec<SkipSegment>, SkipError> {
    let Some(url) = url(query) else {
        return Ok(Vec::new());
    };
    match fetcher.get_text(&url) {
        Ok(body) => parse(&body),
        Err(SkipError::Status { status: 404 }) => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

fn kind_of(skip_type: &str) -> Option<SkipKind> {
    match skip_type {
        "op" | "mixed-op" => Some(SkipKind::Intro),
        "ed" | "mixed-ed" => Some(SkipKind::Credits),
        "recap" => Some(SkipKind::Recap),
        _ => None,
    }
}

fn parse(body: &str) -> Result<Vec<SkipSegment>, SkipError> {
    let reply: Reply = serde_json::from_str(body).map_err(|error| SkipError::Invalid {
        reason: error.to_string(),
    })?;
    Ok(reply
        .results
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| {
            Some(SkipSegment {
                kind: kind_of(&item.skip_type)?,
                start_ms: seconds_to_ms(item.interval.start_time),
                end_ms: seconds_to_ms(item.interval.end_time),
            })
        })
        .collect())
}

#[cfg(test)]
#[path = "ani_skip_tests.rs"]
mod tests;
