//! Map domain types into Slint poster / list models.

use core_model::{Bookmark, SearchResult, WatchProgress};
use slint::{Image, ModelRc, VecModel};

use crate::{EpisodeItem, PluginItem, PosterItem, SettingChoice, ShelfSection, SourceItem};

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
        media_key: item.url.clone().into(),
        data: item.url.clone().into(),
        image: empty_image(),
        progress: 0.0,
    }
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

pub(crate) fn posters_from_bookmarks(items: &[Bookmark]) -> ModelRc<PosterItem> {
    let rows: Vec<PosterItem> = items
        .iter()
        .map(|item| PosterItem {
            title: item.name.clone().into(),
            subtitle: item.provider.clone().into(),
            provider: item.provider.clone().into(),
            url: item.url.clone().into(),
            poster: item.poster_url.clone().unwrap_or_default().into(),
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
                media_key: item.media_key.clone().into(),
                data: item.detail_url.clone().into(),
                image: empty_image(),
                progress,
            }
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

pub(crate) fn source_rows(
    links: &[(core_model::PlayableLink, Option<app_services::LinkHealth>)],
    selected: Option<usize>,
) -> ModelRc<SourceItem> {
    let rows: Vec<SourceItem> = links
        .iter()
        .enumerate()
        .map(|(index, (link, health))| {
            let working = health
                .as_ref()
                .is_some_and(app_services::LinkHealth::is_working);
            let detail = match health {
                Some(h) if h.is_working() => "Working".to_owned(),
                Some(_) => "Broken".to_owned(),
                None => "Checking…".to_owned(),
            };
            SourceItem {
                label: app_services::full_label(link).into(),
                detail: detail.into(),
                working,
                selected: selected == Some(index),
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
