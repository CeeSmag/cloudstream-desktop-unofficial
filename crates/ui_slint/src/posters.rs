//! Map domain types into Slint poster / list models.

use app_services::PlaybackItem;
use core_model::{Bookmark, SearchResult, WatchProgress};
use slint::{ModelRc, VecModel};

use crate::{EpisodeItem, PluginItem, PosterItem, SettingChoice, SourceItem};

pub(crate) fn posters_from_search(items: &[SearchResult]) -> ModelRc<PosterItem> {
    let rows: Vec<PosterItem> = items
        .iter()
        .map(|item| PosterItem {
            title: item.name.clone().into(),
            subtitle: item.api_name.clone().into(),
            provider: item.api_name.clone().into(),
            url: item.url.clone().into(),
            poster: item.poster_url.clone().unwrap_or_default().into(),
            media_key: item.url.clone().into(),
            data: item.url.clone().into(),
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
            PosterItem {
                title: item.title.clone().into(),
                subtitle: subtitle.into(),
                provider: item.provider.clone().into(),
                url: item.detail_url.clone().into(),
                poster: item.poster_url.clone().unwrap_or_default().into(),
                media_key: item.media_key.clone().into(),
                data: item.detail_url.clone().into(),
            }
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

pub(crate) fn playback_from_poster(item: &PosterItem) -> PlaybackItem {
    PlaybackItem {
        provider: item.provider.to_string(),
        title: item.title.to_string(),
        media_key: item.media_key.to_string(),
        detail_url: item.url.to_string(),
        poster_url: {
            let poster = item.poster.to_string();
            (!poster.is_empty()).then_some(poster)
        },
        season: None,
        episode: None,
        next_episode: None,
    }
}

pub(crate) fn source_rows(
    links: &[(core_model::PlayableLink, Option<app_services::LinkHealth>)],
    selected: Option<usize>,
) -> ModelRc<SourceItem> {
    let rows: Vec<SourceItem> = links
        .iter()
        .enumerate()
        .map(|(index, (link, health))| {
            let working = health.as_ref().is_some_and(app_services::LinkHealth::is_working);
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

pub(crate) fn plugin_rows(
    installed: &[core_model::InstalledExtension],
) -> ModelRc<PluginItem> {
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

pub(crate) fn empty_posters() -> ModelRc<PosterItem> {
    ModelRc::new(VecModel::from(Vec::<PosterItem>::new()))
}

pub(crate) fn empty_episodes() -> ModelRc<EpisodeItem> {
    ModelRc::new(VecModel::from(Vec::<EpisodeItem>::new()))
}
