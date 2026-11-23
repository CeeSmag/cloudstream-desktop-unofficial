//! Every extension of every saved repository, kept in memory.

use core_model::PluginListing;

/// One repository with the extensions it offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedRepository {
    /// Address of the repository's `repo.json`.
    pub url: String,
    /// Display name.
    pub name: String,
    /// Address of the repository's icon, when it has one.
    pub icon_url: Option<String>,
    /// The extensions it offers that the app supports (video plugins).
    pub plugins: Vec<PluginListing>,
    /// The extensions it offers that the app does not support (not video plugins). Never shown or
    /// installed; kept so installed extensions can be matched to their listing.
    pub refused: Vec<PluginListing>,
}

/// An extension together with the repository that offers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginHit {
    /// Address of the repository's `repo.json`.
    pub repository_url: String,
    /// Display name of the repository.
    pub repository_name: String,
    /// Address of the repository's icon, when it has one.
    pub repository_icon_url: Option<String>,
    /// The extension.
    pub plugin: PluginListing,
}

/// The extensions of all repositories.
#[derive(Debug, Default)]
pub struct PluginIndex {
    repositories: Vec<IndexedRepository>,
}

impl PluginIndex {
    /// Builds the index over `repositories`, in the order they are listed.
    pub fn new(repositories: Vec<IndexedRepository>) -> Self {
        Self { repositories }
    }

    /// How many extensions are indexed (one that several repositories offer counts once for each).
    pub(crate) fn plugin_count(&self) -> usize {
        self.repositories
            .iter()
            .map(|repository| repository.plugins.len())
            .sum()
    }

    /// How many listings were left out because the app does not support them.
    pub(crate) fn refused_count(&self) -> usize {
        self.repositories
            .iter()
            .map(|repository| repository.refused.len())
            .sum()
    }

    /// Every supported extension with its repository, repository by repository in the order they
    /// are listed.
    pub fn all(&self) -> Vec<PluginHit> {
        self.hits(|repository| &repository.plugins)
    }

    /// Every listing the app does not support, with its repository (same order as [`Self::all`]).
    pub fn refused(&self) -> Vec<PluginHit> {
        self.hits(|repository| &repository.refused)
    }

    fn hits(&self, listings: impl Fn(&IndexedRepository) -> &Vec<PluginListing>) -> Vec<PluginHit> {
        self.repositories
            .iter()
            .flat_map(|repository| {
                listings(repository).iter().map(|plugin| PluginHit {
                    repository_url: repository.url.clone(),
                    repository_name: repository.name.clone(),
                    repository_icon_url: repository.icon_url.clone(),
                    plugin: plugin.clone(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin(name: &str) -> PluginListing {
        PluginListing {
            name: name.to_owned(),
            internal_name: name.replace(' ', ""),
            ..PluginListing::default()
        }
    }

    fn repository(name: &str, plugins: Vec<PluginListing>) -> IndexedRepository {
        IndexedRepository {
            url: format!("https://{name}/repo.json"),
            name: name.to_owned(),
            icon_url: Some(format!("https://{name}/icon.png")),
            plugins,
            refused: vec![plugin("Game")],
        }
    }

    #[test]
    fn all_lists_every_extension_with_its_repository_in_order() {
        let index = PluginIndex::new(vec![
            repository("Alpha", vec![plugin("One"), plugin("Two")]),
            repository("Beta", vec![plugin("Two")]),
        ]);
        let hits = index.all();
        let listed: Vec<(&str, &str)> = hits
            .iter()
            .map(|hit| (hit.repository_name.as_str(), hit.plugin.name.as_str()))
            .collect();
        assert_eq!(
            listed,
            [("Alpha", "One"), ("Alpha", "Two"), ("Beta", "Two")]
        );
        assert_eq!(hits[2].repository_url, "https://Beta/repo.json");
        assert_eq!(
            hits[0].repository_icon_url.as_deref(),
            Some("https://Alpha/icon.png")
        );
    }

    #[test]
    fn the_count_adds_up_the_extensions_of_all_repositories() {
        let index = PluginIndex::new(vec![
            repository("A", vec![plugin("x"), plugin("y")]),
            repository("B", vec![plugin("z")]),
        ]);
        assert_eq!(index.plugin_count(), 3);
        assert_eq!(PluginIndex::default().plugin_count(), 0);
        assert!(PluginIndex::default().all().is_empty());
    }

    #[test]
    fn refused_listings_are_kept_apart_and_never_listed_as_offered() {
        let index = PluginIndex::new(vec![
            repository("A", vec![plugin("x")]),
            repository("B", vec![]),
        ]);
        assert_eq!(index.refused_count(), 2);
        assert!(index.all().iter().all(|hit| hit.plugin.name != "Game"));
        let refused: Vec<String> = index
            .refused()
            .into_iter()
            .map(|hit| hit.repository_name)
            .collect();
        assert_eq!(refused, ["A", "B"]);
    }
}
