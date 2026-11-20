//! Registers Windows symbol and math fallback fonts in Slint's shared `fontique` collection.
//!
//! On Windows, `fontique`'s default `SystemUi` / `SansSerif` families cover only `Segoe UI` and
//! `Arial`, and `Script::Common` has no sample text for DirectWrite's script fallback query.
//! Appending the standard Windows symbol/math/emoji families prevents uncommon Unicode symbols
//! from drawing as missing-glyph squares.

#[cfg(any(windows, test))]
use slint::fontique_012::fontique::{self, FallbackKey, GenericFamily, Script};

/// System families appended after `Segoe UI` / `Arial` on Windows.
#[cfg(any(windows, test))]
const WINDOWS_FALLBACK_FAMILIES: &[&str] = &["Segoe UI Symbol", "Cambria Math", "Segoe UI Emoji"];

/// Appends any of `family_names` that exist in `collection` to `GenericFamily::SansSerif` and
/// the `Script::Common` fallback list. Returns how many families were found and appended.
#[cfg(any(windows, test))]
pub(crate) fn append_fallback_families(
    collection: &mut fontique::Collection,
    family_names: &[&str],
) -> usize {
    let ids: Vec<_> = family_names
        .iter()
        .filter_map(|name| collection.family_id(name))
        .collect();
    if ids.is_empty() {
        return 0;
    }
    collection.append_generic_families(GenericFamily::SansSerif, ids.iter().copied());
    collection.append_fallbacks(
        FallbackKey::new(Script::from_bytes(*b"Zyyy"), None),
        ids.iter().copied(),
    );
    ids.len()
}

/// Adds standard Windows symbol/math/emoji fonts to Slint's fallback chain on Windows.
#[cfg(windows)]
pub(crate) fn register_windows_fallback_fonts() {
    let mut collection = slint::fontique_012::shared_collection();
    let registered = append_fallback_families(&mut collection, WINDOWS_FALLBACK_FAMILIES);
    tracing::debug!(registered, "registered Windows fallback font families");
}

#[cfg(not(windows))]
pub(crate) fn register_windows_fallback_fonts() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_fallback_families_ignores_missing_families_on_empty_collection() {
        let mut collection = fontique::Collection::new(fontique::CollectionOptions {
            shared: false,
            system_fonts: false,
        });
        assert_eq!(
            append_fallback_families(&mut collection, WINDOWS_FALLBACK_FAMILIES),
            0
        );
    }

    #[test]
    fn append_fallback_families_registers_available_system_families() {
        let mut collection = fontique::Collection::new(fontique::CollectionOptions {
            shared: false,
            system_fonts: true,
        });
        let Some(first_name) = collection.family_names().next().map(str::to_owned) else {
            return;
        };
        let added = append_fallback_families(
            &mut collection,
            &[first_name.as_str(), "NonExistentCloudStreamFont"],
        );
        assert_eq!(added, 1);
    }
}
