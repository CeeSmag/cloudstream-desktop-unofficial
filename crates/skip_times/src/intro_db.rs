//! IntroDB (<https://introdb.app>): intro, recap and outro of series episodes, by IMDb id. The
//! reference app asks it third.

use serde::Deserialize;

use crate::skip_fetcher::{SkipError, SkipFetcher};
use crate::skip_segment::{SkipKind, SkipQuery, SkipSegment, TitleKind};
use crate::to_ms::to_ms;

const BASE: &str = "https://api.introdb.app/segments";

#[derive(Deserialize, Default)]
struct Reply {
    intro: Option<Stamp>,
    recap: Option<Stamp>,
    outro: Option<Stamp>,
}

#[derive(Deserialize)]
struct Stamp {
    start_ms: Option<f64>,
    end_ms: Option<f64>,
}

fn url(query: &SkipQuery) -> Option<String> {
    if query.kind == TitleKind::Movie {
        return None;
    }
    Some(format!(
        "{BASE}?imdb_id={}&season={}&episode={}",
        query.ids.imdb.as_deref()?,
        query.season?,
        query.episode?
    ))
}

/// The segments IntroDB knows for the query; empty when it knows none or cannot be asked.
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

fn parse(body: &str) -> Result<Vec<SkipSegment>, SkipError> {
    let reply: Reply = serde_json::from_str(body).map_err(|error| SkipError::Invalid {
        reason: error.to_string(),
    })?;
    let groups = [
        (SkipKind::Intro, reply.intro),
        (SkipKind::Recap, reply.recap),
        (SkipKind::Credits, reply.outro),
    ];
    Ok(groups
        .into_iter()
        .filter_map(|(kind, stamp)| {
            let stamp = stamp?;
            Some(SkipSegment {
                kind,
                start_ms: to_ms(stamp.start_ms?),
                end_ms: to_ms(stamp.end_ms?),
            })
        })
        .collect())
}

#[cfg(test)]
#[path = "intro_db_tests.rs"]
mod tests;
