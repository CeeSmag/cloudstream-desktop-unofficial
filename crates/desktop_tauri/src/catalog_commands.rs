//! Tauri commands for browsing catalog, home shelves, and searching media.

use crate::app_state::AppState;
use crate::dto::{EpisodeDto, HomeItemDto, HomeSectionDto, MediaDetailsDto, SearchItemDto};
use core_model::{HomePageSection, LoadDetails, ProviderInfo, SearchResult};
use tauri::State;

/// Returns the list of installed providers.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn get_available_providers(
    state: State<'_, AppState>,
) -> Result<Vec<ProviderInfo>, String> {
    let services = state.read();
    Ok(services.extensions.providers())
}

/// Retrieves home page sections for the specified provider.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(provider_id = ?provider_id), err)]
pub async fn get_home_sections(
    provider_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<HomeSectionDto>, String> {
    let provider = resolve_provider(&state, provider_id)?;
    let Some(provider_info) = provider else {
        return Ok(Vec::new());
    };
    let services = state.read();
    let mut sections = Vec::new();
    for entry in &provider_info.main_page {
        if let Ok(Some(page)) = services.catalog.home(&provider_info.name, entry) {
            for section in page.items {
                sections.push(map_home_section(section, &provider_info.name));
            }
        }
    }
    Ok(sections)
}

/// Searches media across active providers.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(query = ?query, provider_id = ?provider_id), err)]
pub async fn search_media(
    query: String,
    provider_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<SearchItemDto>, String> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let services = state.read();
    let all_providers = services.extensions.providers();
    let target_providers: Vec<ProviderInfo> = match provider_id {
        Some(name) => all_providers
            .into_iter()
            .filter(|p| p.name == name)
            .collect(),
        None => all_providers,
    };
    let outcomes = services.catalog.search(&target_providers, &query);
    let mut results = Vec::new();
    for outcome in outcomes {
        if let Ok(page) = outcome.result {
            for item in page.items {
                results.push(map_search_item(item, &outcome.provider));
            }
        }
    }
    Ok(results)
}

/// Fetches detailed metadata and episode list for a media URL.
#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, fields(provider_id = ?provider_id, url = ?url), err)]
pub async fn get_media_details(
    provider_id: String,
    url: String,
    state: State<'_, AppState>,
) -> Result<MediaDetailsDto, String> {
    let services = state.read();
    let details = services
        .catalog
        .details(&provider_id, &url)
        .map_err(|error| error.to_string())?;
    Ok(map_media_details(details, &provider_id))
}

fn resolve_provider(
    state: &State<'_, AppState>,
    name: Option<String>,
) -> Result<Option<ProviderInfo>, String> {
    let services = state.read();
    let providers = services.extensions.providers();
    if let Some(target) = name {
        return Ok(providers.into_iter().find(|p| p.name == target));
    }
    Ok(providers.first().cloned())
}

fn map_home_section(section: HomePageSection, provider: &str) -> HomeSectionDto {
    HomeSectionDto {
        title: section.name,
        is_horizontal: section.is_horizontal_images,
        items: section
            .list
            .into_iter()
            .map(|item| HomeItemDto {
                id: item.url.clone(),
                name: item.name,
                url: item.url,
                poster_url: item.poster_url,
                provider: if item.api_name.is_empty() {
                    provider.to_owned()
                } else {
                    item.api_name
                },
                media_type: item.media_type,
                year: item.year,
            })
            .collect(),
    }
}

fn map_search_item(item: SearchResult, provider: &str) -> SearchItemDto {
    SearchItemDto {
        id: item.url.clone(),
        name: item.name,
        url: item.url,
        poster_url: item.poster_url,
        provider: if item.api_name.is_empty() {
            provider.to_owned()
        } else {
            item.api_name
        },
        year: item.year,
    }
}

fn map_media_details(details: LoadDetails, provider: &str) -> MediaDetailsDto {
    let ids = skip_times::ExternalIds::from_sync(&details.sync_data, details.unique_url.as_deref());
    let episodes = details
        .episodes
        .into_iter()
        .map(|ep| EpisodeDto {
            id: ep.data.clone(),
            data: ep.data,
            name: ep.name.unwrap_or_else(|| "Episode".to_owned()),
            season: ep.season,
            episode: ep.episode,
            poster_url: ep.poster_url,
            description: ep.description,
        })
        .collect();

    MediaDetailsDto {
        name: details.name,
        url: details.url,
        provider: provider.to_owned(),
        media_type: details.media_type,
        data_url: details.data_url,
        poster_url: details.poster_url,
        year: details.year,
        plot: details.plot,
        tags: details.tags.unwrap_or_default(),
        episodes,
        background_url: details.background_poster_url,
        rating: details
            .rating
            .filter(|rating| *rating > 0)
            .map(|rating| rating as f32 / 1000.0),
        duration_minutes: details.duration.filter(|minutes| *minutes > 0),
        cast: details.actors,
        imdb_id: ids.imdb,
        tmdb_id: ids.tmdb,
        mal_id: ids.mal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ids_a_provider_reports_reach_the_title_page_checked() {
        let details: LoadDetails = serde_json::from_str(
            r#"{"name":"Dead City","syncData":{"":"{\"Imdb\":\"tt18546730\",\"Tmdb\":\"x/../y\"}"}}"#,
        )
        .expect("valid JSON");
        let page = map_media_details(details, "CineStream");
        assert_eq!(page.imdb_id.as_deref(), Some("tt18546730"));
        assert_eq!(page.tmdb_id, None, "a malformed id is dropped");
        assert_eq!(page.mal_id, None);
    }
}
