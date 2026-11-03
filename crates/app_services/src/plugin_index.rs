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
    /// The extensions it offers.
    pub plugins: Vec<PluginListing>,
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
    pub fn plugin_count(&self) -> usize {
        self.repositories
            .iter()
            .map(|repository| repository.plugins.len())
            .sum()
    }

    /// Every extension with its repository, repository by repository in the order they are listed.
    pub fn all(&self) -> Vec<PluginHit> {
        self.repositories
            .iter()
            .flat_map(|repository| {
                repository.plugins.iter().map(|plugin| PluginHit {
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
}
