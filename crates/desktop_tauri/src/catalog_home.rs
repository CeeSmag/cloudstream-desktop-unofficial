//! Home shelf loading with optional offset/limit so the first paint can stay short.

use app_services::CatalogService;
use core_model::{HomePageSection, MainPageEntry, ProviderInfo};

use crate::dto::HomeSectionDto;

/// How many `main_page` entries the first Home paint loads before the rest.
pub const HOME_FIRST_ENTRIES: usize = 1;

/// Slice of `main_page` entries for this request (`limit = None` means the rest of the list).
pub fn entry_window(total: usize, offset: usize, limit: Option<usize>) -> std::ops::Range<usize> {
    let start = offset.min(total);
    let end = match limit {
        Some(n) => start.saturating_add(n).min(total),
        None => total,
    };
    start..end
}

/// Loads Home shelves for `entries` of `provider`, mapping each section for the UI.
pub fn load_sections(
    catalog: &CatalogService,
    provider: &ProviderInfo,
    entries: &[MainPageEntry],
) -> Result<Vec<HomeSectionDto>, String> {
    let mut sections = Vec::new();
    let mut first_error: Option<String> = None;
    for entry in entries {
        match catalog.home(&provider.name, entry) {
            Ok(Some(page)) => {
                for section in page.items {
                    sections.push(map_home_section(section, &provider.name));
                }
            }
            Ok(None) => tracing::debug!(
                provider = %provider.name,
                category = %entry.name,
                "the provider has nothing for this Home category"
            ),
            Err(error) => {
                tracing::warn!(
                    provider = %provider.name,
                    category = %entry.name,
                    %error,
                    "a Home category failed to load"
                );
                first_error.get_or_insert_with(|| error.to_string());
            }
        }
    }
    match (sections.is_empty(), first_error) {
        (true, Some(error)) => Err(format!(
            "{} could not load its Home page: {error}",
            provider.name
        )),
        _ => Ok(sections),
    }
}

fn map_home_section(section: HomePageSection, provider: &str) -> HomeSectionDto {
    HomeSectionDto {
        title: section.name,
        is_horizontal: section.is_horizontal_images,
        items: section
            .list
            .into_iter()
            .map(|item| crate::dto::HomeItemDto {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_paint_takes_one_entry_then_the_remainder() {
        assert_eq!(entry_window(5, 0, Some(HOME_FIRST_ENTRIES)), 0..1);
        assert_eq!(entry_window(5, 1, None), 1..5);
        assert_eq!(entry_window(1, 0, Some(1)), 0..1);
        assert_eq!(entry_window(1, 1, None), 1..1);
        assert_eq!(entry_window(0, 0, Some(1)), 0..0);
    }
}
