//! Map domain types into Slint poster / list models.

use core_model::{Bookmark, SearchResult, WatchProgress};
use slint::{Image, ModelRc, VecModel};

use crate::bridge::HomeShelf;
use crate::shelf_pager::collapsed_items;
use crate::text_sanitize::clean_display_text;
use crate::{CastItem, EpisodeItem, PluginItem, PosterItem, ShelfSection};

fn empty_image() -> Image {
    Image::default()
}

pub(crate) fn poster_from_search(item: &SearchResult) -> PosterItem {
    PosterItem {
        title: clean_display_text(&item.name).into(),
        subtitle: item.api_name.clone().into(),
        provider: item.api_name.clone().into(),
        url: item.url.clone().into(),
        poster: item.poster_url.clone().unwrap_or_default().into(),
        poster_headers: encode_poster_headers(item.poster_headers.as_ref()).into(),
        media_key: item.url.clone().into(),
        image: empty_image(),
        progress: 0.0,
    }
}

/// The provider's poster headers as text the Slint model can carry, sorted, one per line.
pub(crate) fn encode_poster_headers(
    headers: Option<&std::collections::HashMap<String, String>>,
) -> String {
    let mut lines: Vec<String> = headers
        .into_iter()
        .flatten()
        .filter(|(name, _)| !name.contains(['\n', ':']))
        .map(|(name, value)| format!("{name}: {}", value.replace('\n', " ")))
        .collect();
    lines.sort();
    lines.join("\n")
}

/// The headers of [`encode_poster_headers`] back as pairs.
pub(crate) fn decode_poster_headers(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
        .filter(|(name, _)| !name.is_empty())
        .collect()
}

/// Poster rows for `items`, without pictures yet.
pub(crate) fn poster_rows_from_search(items: &[SearchResult]) -> Vec<PosterItem> {
    items.iter().map(poster_from_search).collect()
}

pub(crate) fn posters_from_search(items: &[SearchResult]) -> ModelRc<PosterItem> {
    ModelRc::new(VecModel::from(poster_rows_from_search(items)))
}

pub(crate) fn shelves_from_sections(sections: &[HomeShelf]) -> ModelRc<ShelfSection> {
    let rows: Vec<ShelfSection> = sections
        .iter()
        .map(|shelf| ShelfSection {
            title: clean_display_text(&shelf.title).into(),
            // Only the first posters of a shelf exist as rows (and get pictures); the rest of its
            // titles wait in the shelf's data until the viewer expands it.
            items: ModelRc::new(VecModel::from(poster_rows_from_search(collapsed_items(
                &shelf.items,
            )))),
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

/// Browse tiles: TMDB titles (their id stands in for the address until they are matched).
pub(crate) fn posters_from_browse(items: &[app_services::BrowseItem]) -> ModelRc<PosterItem> {
    let rows: Vec<PosterItem> = items
        .iter()
        .map(|item| PosterItem {
            title: item.name.clone().into(),
            subtitle: item
                .year
                .map(|year| year.to_string())
                .unwrap_or_default()
                .into(),
            provider: "".into(),
            url: item.id.clone().into(),
            poster: item.poster_url.clone().unwrap_or_default().into(),
            poster_headers: "".into(),
            media_key: item.id.clone().into(),
            image: empty_image(),
            progress: 0.0,
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

pub(crate) fn posters_from_bookmarks(items: &[Bookmark]) -> ModelRc<PosterItem> {
    let rows: Vec<PosterItem> = items
        .iter()
        .map(|item| PosterItem {
            title: item.name.clone().into(),
            subtitle: item.provider.clone().into(),
            provider: item.provider.clone().into(),
            url: item.url.clone().into(),
            poster: item.poster_url.clone().unwrap_or_default().into(),
            poster_headers: "".into(),
            media_key: item.url.clone().into(),
            image: empty_image(),
            progress: 0.0,
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

pub(crate) fn posters_from_continue(items: &[WatchProgress]) -> ModelRc<PosterItem> {
    let rows: Vec<PosterItem> = items
        .iter()
        .map(|item| {
            let subtitle = match (item.season, item.episode) {
                (Some(s), Some(e)) => format!("S{s} E{e}"),
                _ => item.provider.clone(),
            };
            let progress = item.fraction_watched().clamp(0.0, 1.0);
            PosterItem {
                title: item.title.clone().into(),
                subtitle: subtitle.into(),
                provider: item.provider.clone().into(),
                url: item.detail_url.clone().into(),
                poster: item.poster_url.clone().unwrap_or_default().into(),
                poster_headers: "".into(),
                media_key: item.media_key.clone().into(),
                image: empty_image(),
                progress,
            }
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

/// Settings' installed plugins. One that is not a video plugin says so and why, and offers only
/// Remove; one not judged yet (installed before content types were recorded) shows as usual.
pub(crate) fn plugin_rows(installed: &[app_services::InstalledStatus]) -> ModelRc<PluginItem> {
    let rows: Vec<PluginItem> = installed
        .iter()
        .map(|status| {
            let ext = &status.extension;
            let note = match &status.support {
                app_services::Support::NotVideo(reason) => {
                    format!("Not loaded: {reason}. You can remove it.")
                }
                app_services::Support::Supported | app_services::Support::Unknown => String::new(),
            };
            PluginItem {
                name: ext.internal_name.clone().into(),
                detail: format!("v{}", ext.version).into(),
                file_name: ext.file_name.clone().into(),
                trusted: ext.trusted,
                supported: note.is_empty(),
                note: note.into(),
            }
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

pub(crate) fn string_model(
    labels: impl IntoIterator<Item = String>,
) -> ModelRc<slint::SharedString> {
    ModelRc::new(VecModel::from(
        labels
            .into_iter()
            .map(slint::SharedString::from)
            .collect::<Vec<_>>(),
    ))
}

pub(crate) fn empty_posters() -> ModelRc<PosterItem> {
    ModelRc::new(VecModel::from(Vec::<PosterItem>::new()))
}

pub(crate) fn empty_shelves() -> ModelRc<ShelfSection> {
    ModelRc::new(VecModel::from(Vec::<ShelfSection>::new()))
}

pub(crate) fn empty_episodes() -> ModelRc<EpisodeItem> {
    ModelRc::new(VecModel::from(Vec::<EpisodeItem>::new()))
}

pub(crate) fn empty_cast() -> ModelRc<CastItem> {
    ModelRc::new(VecModel::from(Vec::<CastItem>::new()))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn poster_headers_round_trip_sorted() {
        let headers = HashMap::from([
            ("Referer".to_owned(), "https://site.test/".to_owned()),
            ("Cookie".to_owned(), "a=b".to_owned()),
        ]);
        let text = encode_poster_headers(Some(&headers));
        assert_eq!(text, "Cookie: a=b\nReferer: https://site.test/");
        assert_eq!(
            decode_poster_headers(&text),
            vec![
                ("Cookie".to_owned(), "a=b".to_owned()),
                ("Referer".to_owned(), "https://site.test/".to_owned()),
            ]
        );
    }

    #[test]
    fn poster_headers_none_or_broken_names_encode_to_nothing() {
        assert_eq!(encode_poster_headers(None), "");
        let broken = HashMap::from([("Bad:Name".to_owned(), "x".to_owned())]);
        assert_eq!(encode_poster_headers(Some(&broken)), "");
        assert!(decode_poster_headers("no colon here").is_empty());
    }

    use slint::Model;

    fn rows<T: Clone + 'static>(model: &ModelRc<T>) -> Vec<T> {
        (0..model.row_count())
            .filter_map(|row| model.row_data(row))
            .collect()
    }

    fn found(name: &str, url: &str) -> SearchResult {
        SearchResult {
            name: name.to_owned(),
            url: url.to_owned(),
            api_name: "Prov".to_owned(),
            poster_url: Some(format!("{url}.jpg")),
            ..SearchResult::default()
        }
    }

    #[test]
    fn a_search_result_becomes_a_poster_with_clean_text_and_its_address_as_key() {
        let mut item = found("\u{1D401}reaking \u{1D401}ad", "https://p/1");
        item.poster_headers = Some(HashMap::from([(
            "Referer".to_owned(),
            "https://p/".to_owned(),
        )]));
        let poster = poster_from_search(&item);
        assert_eq!(
            poster.title, "Breaking Bad",
            "styled letters are folded to plain ones"
        );
        assert_eq!(
            (poster.subtitle.as_str(), poster.provider.as_str()),
            ("Prov", "Prov")
        );
        assert_eq!(
            (poster.url.as_str(), poster.media_key.as_str()),
            ("https://p/1", "https://p/1")
        );
        assert_eq!(poster.poster, "https://p/1.jpg");
        assert_eq!(poster.poster_headers, "Referer: https://p/");
        assert!(poster.progress.abs() < f32::EPSILON);
    }

    #[test]
    fn a_header_value_on_several_lines_is_kept_on_one() {
        let headers = HashMap::from([("Cookie".to_owned(), "a=1\nb=2".to_owned())]);
        assert_eq!(encode_poster_headers(Some(&headers)), "Cookie: a=1 b=2");
        assert!(decode_poster_headers(": no name").is_empty());
    }

    #[test]
    fn a_shelf_holds_only_its_first_posters_until_it_is_expanded() {
        let items: Vec<SearchResult> = (0..25)
            .map(|n| found(&format!("T{n}"), &format!("https://p/{n}")))
            .collect();
        let shelf = HomeShelf {
            title: "\u{1D413}rending".to_owned(),
            items,
            provider: "Prov".to_owned(),
            entry: core_model::MainPageEntry::default(),
            section_index: 0,
            has_next: true,
        };
        let shelves = rows(&shelves_from_sections(&[shelf]));
        let [section] = shelves.as_slice() else {
            panic!("one shelf");
        };
        assert_eq!(section.title, "Trending");
        let titles: Vec<String> = rows(&section.items)
            .iter()
            .map(|p| p.title.to_string())
            .collect();
        assert_eq!(titles.len(), crate::poster_limits::COLLAPSED_SHELF_POSTERS);
        assert_eq!(titles.first().map(String::as_str), Some("T0"));
    }

    #[test]
    fn a_browse_tile_shows_its_year_and_keys_on_the_tmdb_id() {
        let item = |year| app_services::BrowseItem {
            id: "movie:603".to_owned(),
            name: "The Matrix".to_owned(),
            poster_url: None,
            year,
            rating: Some(8.2),
            media_type: "movie".to_owned(),
        };
        let tiles = rows(&posters_from_browse(&[item(Some(1999)), item(None)]));
        assert_eq!(tiles[0].subtitle, "1999");
        assert_eq!(tiles[1].subtitle, "", "no year, no subtitle");
        assert_eq!(
            (tiles[0].url.as_str(), tiles[0].media_key.as_str()),
            ("movie:603", "movie:603")
        );
        assert_eq!(
            (tiles[0].provider.as_str(), tiles[0].poster.as_str()),
            ("", "")
        );
    }

    #[test]
    fn a_bookmark_tile_names_its_provider_and_opens_its_title() {
        let bookmark = Bookmark {
            provider: "Prov".to_owned(),
            url: "https://p/9".to_owned(),
            name: "Dune".to_owned(),
            poster_url: Some("https://p/9.jpg".to_owned()),
            ..Bookmark::default()
        };
        let [tile] = rows(&posters_from_bookmarks(&[bookmark]))
            .try_into()
            .expect("one");
        assert_eq!(
            (tile.title.as_str(), tile.subtitle.as_str()),
            ("Dune", "Prov")
        );
        assert_eq!(
            (tile.url.as_str(), tile.media_key.as_str()),
            ("https://p/9", "https://p/9")
        );
        assert_eq!(tile.poster, "https://p/9.jpg");
    }

    #[test]
    fn a_continue_tile_shows_the_episode_and_how_much_was_watched() {
        let watched = |season, episode, position_ms| WatchProgress {
            provider: "Prov".to_owned(),
            media_key: "ep-key".to_owned(),
            title: "Show".to_owned(),
            detail_url: "https://p/show".to_owned(),
            position_ms,
            duration_ms: 1_000,
            season,
            episode,
            ..WatchProgress::default()
        };
        let tiles = rows(&posters_from_continue(&[
            watched(Some(2), Some(5), 250),
            watched(None, Some(5), 2_000),
        ]));
        assert_eq!(tiles[0].subtitle, "S2 E5");
        assert!((tiles[0].progress - 0.25).abs() < 1e-6);
        assert_eq!(
            tiles[1].subtitle, "Prov",
            "without a season the provider is shown"
        );
        assert!(
            (tiles[1].progress - 1.0).abs() < 1e-6,
            "past the end counts as watched"
        );
        assert_eq!(
            (tiles[0].url.as_str(), tiles[0].media_key.as_str()),
            ("https://p/show", "ep-key"),
            "the tile opens the show and resumes the episode"
        );
    }

    #[test]
    fn plugin_rows_show_the_version_and_the_trust_switch() {
        let ext = core_model::InstalledExtension {
            file_name: "Good.cs3".to_owned(),
            internal_name: "Good".to_owned(),
            version: 7,
            trusted: true,
            ..core_model::InstalledExtension::default()
        };
        let status = app_services::InstalledStatus {
            extension: ext,
            support: app_services::Support::Supported,
        };
        let [row] = rows(&plugin_rows(&[status])).try_into().expect("one");
        assert_eq!(
            (
                row.name.as_str(),
                row.detail.as_str(),
                row.file_name.as_str()
            ),
            ("Good", "v7", "Good.cs3")
        );
        assert!(row.trusted);
        assert!(row.supported);
        assert!(row.note.is_empty());
    }

    #[test]
    fn a_plugin_that_is_not_video_says_why_and_one_not_judged_yet_shows_as_usual() {
        let status = |name: &str, support| app_services::InstalledStatus {
            extension: core_model::InstalledExtension {
                internal_name: name.to_owned(),
                file_name: format!("{name}.cs3"),
                ..core_model::InstalledExtension::default()
            },
            support,
        };
        let shown = rows(&plugin_rows(&[
            status(
                "Doom",
                app_services::Support::NotVideo("not a video plugin (it declares Others)".into()),
            ),
            status("Old", app_services::Support::Unknown),
        ]));
        assert!(!shown[0].supported);
        assert_eq!(
            shown[0].note.as_str(),
            "Not loaded: not a video plugin (it declares Others). You can remove it."
        );
        assert!(
            shown[1].supported,
            "not judged yet: it loads, so it is shown as usual"
        );
    }

    #[test]
    fn empty_and_label_models_hold_what_they_were_given() {
        assert_eq!(empty_posters().row_count(), 0);
        assert_eq!(empty_shelves().row_count(), 0);
        assert_eq!(empty_episodes().row_count(), 0);
        assert_eq!(empty_cast().row_count(), 0);
        assert_eq!(rows(&posters_from_search(&[found("A", "u")])).len(), 1);
        let labels = rows(&string_model(["a".to_owned(), "b".to_owned()]));
        assert_eq!(labels, ["a", "b"]);
    }
}
