//! Map domain types into Slint poster / list models.

use core_model::{Bookmark, SearchResult, WatchProgress};
use slint::{Image, ModelRc, VecModel};

use crate::{EpisodeItem, PluginItem, PosterItem, SettingChoice, ShelfSection};

fn empty_image() -> Image {
    Image::default()
}

pub(crate) fn poster_from_search(item: &SearchResult) -> PosterItem {
    PosterItem {
        title: item.name.clone().into(),
        subtitle: item.api_name.clone().into(),
        provider: item.api_name.clone().into(),
        url: item.url.clone().into(),
        poster: item.poster_url.clone().unwrap_or_default().into(),
        poster_headers: encode_poster_headers(item.poster_headers.as_ref()).into(),
        media_key: item.url.clone().into(),
        data: item.url.clone().into(),
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

pub(crate) fn posters_from_search(items: &[SearchResult]) -> ModelRc<PosterItem> {
    let rows: Vec<PosterItem> = items.iter().map(poster_from_search).collect();
    ModelRc::new(VecModel::from(rows))
}

pub(crate) fn shelves_from_sections(
    sections: &[(String, Vec<SearchResult>)],
) -> ModelRc<ShelfSection> {
    let rows: Vec<ShelfSection> = sections
        .iter()
        .map(|(title, items)| ShelfSection {
            title: title.clone().into(),
            items: ModelRc::new(VecModel::from(
                items.iter().map(poster_from_search).collect::<Vec<_>>(),
            )),
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
            data: item.id.clone().into(),
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
            data: item.url.clone().into(),
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
                data: item.detail_url.clone().into(),
                image: empty_image(),
                progress,
            }
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

pub(crate) fn plugin_rows(installed: &[core_model::InstalledExtension]) -> ModelRc<PluginItem> {
    let rows: Vec<PluginItem> = installed
        .iter()
        .map(|ext| PluginItem {
            name: ext.internal_name.clone().into(),
            detail: format!("v{}", ext.version).into(),
            file_name: ext.file_name.clone().into(),
            trusted: ext.trusted,
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

pub(crate) fn cache_choices() -> ModelRc<SettingChoice> {
    let rows: Vec<SettingChoice> = app_services::VIDEO_CACHE_SIZES_MB
        .iter()
        .map(|mb| SettingChoice {
            label: format!("{mb} MB").into(),
            value: mb.to_string().into(),
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
}
