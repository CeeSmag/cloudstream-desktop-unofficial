//! TheIntroDB (<https://theintrodb.org/docs>): intro, recap, credits and preview of films and
//! episodes, by TMDb or IMDb id. The reference app asks it second, after AniSkip.

use serde::Deserialize;

use crate::skip_fetcher::{SkipError, SkipFetcher};
use crate::skip_segment::{SkipKind, SkipQuery, SkipSegment, TitleKind};
use crate::to_ms::to_ms;

const BASE: &str = "https://api.theintrodb.org/v4/media";

#[derive(Deserialize, Default)]
struct Reply {
    #[serde(default)]
    intro: Vec<Stamp>,
    #[serde(default)]
    recap: Vec<Stamp>,
    #[serde(default)]
    credits: Vec<Stamp>,
    #[serde(default)]
    preview: Vec<Stamp>,
}

/// A stretch; a missing start means the very beginning and a missing end the end of the video.
#[derive(Deserialize)]
struct Stamp {
    start_ms: Option<f64>,
    end_ms: Option<f64>,
}

/// The URL to ask, or none when the query lacks what this database needs.
fn url(query: &SkipQuery) -> Option<String> {
    let id = match (&query.ids.tmdb, &query.ids.imdb) {
        (Some(tmdb), _) => format!("tmdb_id={tmdb}"),
        (None, Some(imdb)) => format!("imdb_id={imdb}"),
        (None, None) => return None,
    };
    if query.kind == TitleKind::Movie {
        return Some(format!("{BASE}?{id}"));
    }
    Some(format!(
        "{BASE}?{id}&season={}&episode={}",
        query.season?, query.episode?
    ))
}

/// The segments TheIntroDB knows for the query; empty when it knows none or cannot be asked.
pub(crate) fn segments(
    fetcher: &dyn SkipFetcher,
    query: &SkipQuery,
) -> Result<Vec<SkipSegment>, SkipError> {
    let Some(url) = url(query) else {
        return Ok(Vec::new());
    };
    match fetcher.get_text(&url) {
        Ok(body) => parse(&body, query.duration_ms),
        Err(SkipError::Status { status: 404 }) => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

fn parse(body: &str, duration_ms: u64) -> Result<Vec<SkipSegment>, SkipError> {
    let reply: Reply = serde_json::from_str(body).map_err(|error| SkipError::Invalid {
        reason: error.to_string(),
    })?;
    let groups = [
        (SkipKind::Intro, reply.intro),
        (SkipKind::Recap, reply.recap),
        (SkipKind::Credits, reply.credits),
        (SkipKind::Preview, reply.preview),
    ];
    let mut found = Vec::new();
    for (kind, stamps) in groups {
        for stamp in stamps {
            let start_ms = stamp.start_ms.map_or(0, to_ms);
            let end_ms = stamp.end_ms.map_or(duration_ms, to_ms);
            found.push(SkipSegment {
                kind,
                start_ms,
                end_ms,
            });
        }
    }
    Ok(found)
}

#[cfg(test)]
#[path = "the_intro_db_tests.rs"]
mod tests;
