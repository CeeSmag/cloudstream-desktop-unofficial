//! Property-based tests for `skip_times` external ID validation and segment cleaning/bounds.

use std::collections::BTreeMap;
use std::sync::Arc;

use proptest::prelude::*;
use skip_times::{ExternalIds, SkipError, SkipFetcher, SkipLookup, SkipQuery, TitleKind};

struct StaticFetcher(String);

impl SkipFetcher for StaticFetcher {
    fn get_text(&self, _url: &str) -> Result<String, SkipError> {
        Ok(self.0.clone())
    }
}

proptest! {
    #[test]
    fn external_ids_from_sync_only_accepts_url_safe_ids(
        imdb in ".*",
        tmdb in ".*",
        mal in ".*",
        unique in proptest::option::of(".*"),
    ) {
        let mut sync = BTreeMap::new();
        sync.insert("imdb".to_owned(), imdb);
        sync.insert("tmdb".to_owned(), tmdb);
        sync.insert("mal".to_owned(), mal);
        let ids = ExternalIds::from_sync(&sync, unique.as_deref());
        if let Some(id) = ids.imdb.as_deref() {
            prop_assert!(id.starts_with("tt"));
            prop_assert!(id[2..].chars().all(|c| c.is_ascii_digit()));
        }
        if let Some(id) = ids.tmdb.as_deref() {
            prop_assert!(id.chars().all(|c| c.is_ascii_digit()));
        }
        if let Some(id) = ids.mal.as_deref() {
            prop_assert!(id.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn skip_lookup_returns_sorted_valid_segments_within_duration(
        intro_start in 0u64..1200u64,
        intro_len in 5u64..300u64,
        outro_start in 600u64..1800u64,
        outro_len in 5u64..300u64,
        duration_secs in 300u64..2400u64,
    ) {
        let body = format!(
            r#"{{"found":true,"results":[
                {{"interval":{{"startTime":{},"endTime":{}}},"skipType":"op","episodeLength":{}}},
                {{"interval":{{"startTime":{},"endTime":{}}},"skipType":"ed","episodeLength":{}}}
            ]}}"#,
            intro_start,
            intro_start + intro_len,
            duration_secs,
            outro_start,
            outro_start + outro_len,
            duration_secs,
        );
        let lookup = SkipLookup::new(Arc::new(StaticFetcher(body)));
        let query = SkipQuery {
            ids: ExternalIds {
                mal: Some("12345".into()),
                imdb: None,
                tmdb: None,
            },
            kind: TitleKind::Anime,
            season: Some(1),
            episode: Some(1),
            duration_ms: duration_secs * 1000,
        };
        let segs = lookup.find(&query).expect("parse succeeds");
        for seg in &segs {
            prop_assert!(seg.end_ms > seg.start_ms);
            prop_assert!(seg.end_ms <= duration_secs * 1000);
        }
        for pair in segs.windows(2) {
            prop_assert!(pair[0].start_ms <= pair[1].start_ms);
        }
    }
}
