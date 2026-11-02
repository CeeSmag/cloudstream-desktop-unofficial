use serde::{Deserialize, Serialize};

/// A repository an extension asked the app to add while it was running (CloudStream's
/// `RepositoryManager.addRepository`), for example the "Mega" plugin adding every known repository.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredRepository {
    /// Display name the extension reported; may be empty.
    #[serde(default)]
    pub name: String,
    /// URL of the repository's `repo.json`.
    pub url: String,
    /// Icon URL, when the repository has one.
    #[serde(default)]
    pub icon_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_sidecar_reply_with_and_without_an_icon() {
        let json = r#"[{"name":"A","url":"https://a/repo.json","iconUrl":null},{"url":"https://b/repo.json"}]"#;
        let list: Vec<DiscoveredRepository> = serde_json::from_str(json).expect("valid reply");
        assert_eq!(list[0].name, "A");
        assert_eq!(list[1].name, "");
        assert!(list[1].icon_url.is_none());
    }
}
