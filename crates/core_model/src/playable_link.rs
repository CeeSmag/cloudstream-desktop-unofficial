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
    /// ClearKey DRM key identifier (`DrmExtractorLink.kid`), in Base64, Base64URL or hex.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kid: Option<String>,
    /// ClearKey DRM decryption key (`DrmExtractorLink.key`), in Base64, Base64URL or hex.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
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
        assert_eq!(links.links[0].kid, None);
        assert_eq!(links.links[0].key, None);
    }

    #[test]
    fn deserialize_playable_links_reads_drm_clear_key_fields() {
        let json = r#"{"success":true,"links":[{"source":"LivXow","name":"TNT SPORTS FHD",
            "url":"https://otte.cache.aiv-cdn.net/live/cenc.mpd","referer":"","quality":400,
            "headers":{"User-Agent":"Mozilla/5.0"},"type":"DASH",
            "kid":"8994QwgK50O_hl3F_fZMaA","key":"VnyGO8Eut0eI6nSIjAQuGw"}],"subtitles":[]}"#;
        let links: PlayableLinks = serde_json::from_str(json).expect("valid DRM links JSON");
        assert_eq!(
            links.links[0].kid.as_deref(),
            Some("8994QwgK50O_hl3F_fZMaA")
        );
        assert_eq!(
            links.links[0].key.as_deref(),
            Some("VnyGO8Eut0eI6nSIjAQuGw")
        );
    }
}
