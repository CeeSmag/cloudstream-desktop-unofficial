use serde::{Deserialize, Serialize};

/// The pointer for one show or movie to resume in Continue Watching.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumePointer {
    /// Provider the item belongs to.
    pub provider: String,
    /// Provider URL of the parent title / show.
    pub detail_url: String,
    /// Provider payload of the episode (or movie) to resume.
    pub media_key: String,
    /// Title shown in continue watching.
    pub title: String,
    /// Poster image URL.
    #[serde(default)]
    pub poster_url: Option<String>,
    /// Season number when known.
    #[serde(default)]
    pub season: Option<i32>,
    /// Episode number when known.
    #[serde(default)]
    pub episode: Option<i32>,
    /// When the pointer was last updated, in milliseconds since the Unix epoch.
    #[serde(default)]
    pub updated_at_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_pointer_defaults_are_empty_or_none() {
        let pointer = ResumePointer::default();
        assert!(pointer.provider.is_empty());
        assert_eq!(pointer.season, None);
        assert_eq!(pointer.episode, None);
    }
}
