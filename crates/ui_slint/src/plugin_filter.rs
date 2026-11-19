//! The Extensions page's plugin list: search, language filter, installed / not installed, sort.
//!
//! Ported from the old app (`pluginCatalog.ts`, `pluginSort.ts`). A search looks through the name,
//! description, authors, language and repository; plugins with the search in their name come
//! before those that only mention it.

use app_services::PluginHit;

/// Which plugins the list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PluginView {
    #[default]
    All,
    NotInstalled,
    Installed,
}

/// How the list is ordered (a search orders by match first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PluginSort {
    #[default]
    Name,
    Language,
    /// Biggest first; unknown sizes last.
    Size,
}

/// What the viewer narrowed the list to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PluginQuery {
    pub(crate) text: String,
    /// A language code as the repositories write it (`en`); empty for every language.
    pub(crate) language: String,
    pub(crate) view: PluginView,
    pub(crate) sort: PluginSort,
}

/// The plugins that fit `query`, in the order to show them. `installed` holds internal names.
pub(crate) fn filter_plugins<'a>(
    hits: &'a [PluginHit],
    installed: &[String],
    query: &PluginQuery,
) -> Vec<&'a PluginHit> {
    let is_installed = |hit: &PluginHit| installed.contains(&hit.plugin.internal_name);
    let mut kept: Vec<&PluginHit> = hits
        .iter()
        .filter(|hit| match query.view {
            PluginView::All => true,
            PluginView::NotInstalled => !is_installed(hit),
            PluginView::Installed => is_installed(hit),
        })
        .filter(|hit| query.language.is_empty() || language_of(hit) == query.language)
        .collect();
    kept.sort_by(|a, b| compare(a, b, query.sort));
    let whole = query.text.trim().to_lowercase();
    let words: Vec<&str> = whole.split_whitespace().collect();
    if words.is_empty() {
        return kept;
    }
    let mut ranked: Vec<(u8, &PluginHit)> = kept
        .into_iter()
        .filter_map(|hit| rank_of(hit, &whole, &words).map(|rank| (rank, hit)))
        .collect();
    ranked.sort_by_key(|(rank, _)| *rank);
    ranked.into_iter().map(|(_, hit)| hit).collect()
}

/// The languages the plugins are in, each once, in alphabetical order.
pub(crate) fn language_choices(hits: &[PluginHit]) -> Vec<String> {
    let mut languages: Vec<String> = hits
        .iter()
        .map(language_of)
        .filter(|language| !language.is_empty())
        .collect();
    languages.sort();
    languages.dedup();
    languages
}

fn language_of(hit: &PluginHit) -> String {
    hit.plugin
        .language
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_lowercase()
}

fn compare(a: &PluginHit, b: &PluginHit, sort: PluginSort) -> std::cmp::Ordering {
    let by_name = || {
        a.plugin
            .name
            .to_lowercase()
            .cmp(&b.plugin.name.to_lowercase())
    };
    match sort {
        PluginSort::Name => by_name(),
        PluginSort::Language => language_of(a).cmp(&language_of(b)).then_with(by_name),
        PluginSort::Size => {
            let size = |hit: &PluginHit| std::cmp::Reverse(hit.plugin.file_size);
            // Reverse(None) sorts after every Reverse(Some(_)): unknown sizes go last.
            size(a).cmp(&size(b)).then_with(by_name)
        }
    }
}

/// 0: the whole search is in the name; 1: every word is in the name; 2: every word is somewhere.
fn rank_of(hit: &PluginHit, whole: &str, words: &[&str]) -> Option<u8> {
    let plugin = &hit.plugin;
    let name = format!("{} {}", plugin.name, plugin.internal_name).to_lowercase();
    if name.contains(whole) {
        return Some(0);
    }
    if words.iter().all(|word| name.contains(word)) {
        return Some(1);
    }
    let everything = [
        name,
        plugin.description.clone().unwrap_or_default(),
        plugin.authors.join(" "),
        plugin.language.clone().unwrap_or_default(),
        hit.repository_name.clone(),
    ]
    .join(" ")
    .to_lowercase();
    words
        .iter()
        .all(|word| everything.contains(word))
        .then_some(2)
}

#[cfg(test)]
#[path = "plugin_filter_tests.rs"]
mod tests;
