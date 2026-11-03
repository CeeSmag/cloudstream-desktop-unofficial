use serde::{Deserialize, Serialize};

use crate::{PlayableLink, SubtitleTrack};

/// How much of a link search the caller has already received.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LinkSearchCursor {
    /// Links received so far.
    pub links: usize,
    /// Subtitles received so far.
    pub subtitles: usize,
}

/// What a running link search found since the last time it was asked, and whether it is over.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkSearchUpdate {
    /// Streams found since the cursor.
    #[serde(default)]
    pub links: Vec<PlayableLink>,
    /// Subtitles found since the cursor.
    #[serde(default)]
    pub subtitles: Vec<SubtitleTrack>,
    /// Number of links received after this update.
    #[serde(default)]
    pub next_link: usize,
    /// Number of subtitles received after this update.
    #[serde(default)]
    pub next_subtitle: usize,
    /// True once the search has finished (or was stopped); nothing more will come.
    #[serde(default)]
    pub done: bool,
    /// Whether the provider handled the request.
    #[serde(default)]
    pub success: bool,
    /// Why the search failed, when it did.
    #[serde(default)]
    pub failure: Option<String>,
}

impl LinkSearchUpdate {
    /// Where to continue after this update.
    pub fn cursor(&self) -> LinkSearchCursor {
        LinkSearchCursor {
            links: self.next_link,
            subtitles: self.next_subtitle,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_update_from_the_runtime_is_read_and_gives_the_next_cursor() {
        let json = r#"{"links":[{"url":"https://h/v.mp4","type":"VIDEO"}],"subtitles":[],
            "nextLink":5,"nextSubtitle":2,"done":false,"success":false}"#;
        let update: LinkSearchUpdate = serde_json::from_str(json).expect("valid update");
        assert_eq!(update.links.len(), 1);
        assert_eq!(
            update.cursor(),
            LinkSearchCursor {
                links: 5,
                subtitles: 2
            }
        );
        assert!(!update.done && update.failure.is_none());
    }
}
