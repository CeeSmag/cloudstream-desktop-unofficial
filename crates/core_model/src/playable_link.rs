use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A resolved stream the player can open.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayableLink {
    /// Name of the extractor or provider that produced the link.
    #[serde(default)]
    pub source: String,
    /// Display name of the link.
    #[serde(default)]
    pub name: String,
    /// Stream URL.
    pub url: String,
    /// Referer header the host expects.
    #[serde(default)]
    pub referer: String,
    /// Vertical resolution, or a provider-defined quality score.
    #[serde(default)]
    pub quality: i32,
    /// Extra HTTP headers needed to open the stream.
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Stream container kind: `VIDEO`, `M3U8`, `DASH`, `TORRENT` or `MAGNET`.
    #[serde(default, rename = "type")]
    pub link_type: String,
}

/// An external subtitle file offered with a stream.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleTrack {
    /// Language label or code.
    #[serde(default)]
    pub lang: String,
    /// Subtitle file URL.
    pub url: String,
    /// Extra HTTP headers needed to download the file.
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
}

/// Result of resolving a title or hoster URL into streams.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayableLinks {
    /// Whether the provider or an extractor handled the request.
    #[serde(default)]
    pub success: bool,
    /// Streams found.
    #[serde(default)]
    pub links: Vec<PlayableLink>,
    /// Subtitles found.
    #[serde(default)]
    pub subtitles: Vec<SubtitleTrack>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_playable_links_reads_real_sidecar_reply() {
        let json = r#"{"success":true,"links":[{"source":"Internet Archive","name":"x (mp4)",
            "url":"https://archive.org/a.mp4","referer":"","quality":400,"headers":{},
            "extractorData":null,"type":"VIDEO","audioTracks":[]}],"subtitles":[]}"#;
        let links: PlayableLinks = serde_json::from_str(json).expect("valid links JSON");
        assert!(links.success);
        assert_eq!(links.links[0].link_type, "VIDEO");
        assert_eq!(links.links[0].quality, 400);
    }
}
