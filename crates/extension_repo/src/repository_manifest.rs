use serde::Deserialize;

/// The contents of a repository's `repo.json`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryManifest {
    /// Repository display name.
    pub name: String,
    /// Short description.
    #[serde(default)]
    pub description: Option<String>,
    /// URLs of the JSON files listing the repository's plugins.
    #[serde(default)]
    pub plugin_lists: Vec<String>,
    /// Address of the repository's icon, when it has one.
    #[serde(default)]
    pub icon_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_official_repo_json() {
        let json = r#"{"name":"Cloudstream providers repository","description":"d",
            "manifestVersion":1,"pluginLists":["https://r/plugins.json"]}"#;
        let manifest: RepositoryManifest = serde_json::from_str(json).expect("valid manifest");
        assert_eq!(manifest.plugin_lists, ["https://r/plugins.json"]);
    }
}
