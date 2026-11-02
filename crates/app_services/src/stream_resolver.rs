use std::sync::Arc;

use core_model::{PlayableLink, PlayableLinks};
use extension_api::{GatewayError, ProviderGateway};

use crate::ServiceError;

/// How many times link loading is attempted when it comes back empty or the call times out.
const MAX_ATTEMPTS: u32 = 3;

/// Link kinds a video player can open.
const PLAYABLE_TYPES: &[&str] = &["VIDEO", "M3U8", "DASH"];

/// File extensions of audio-only streams; they are offered after any video stream.
const AUDIO_EXTENSIONS: &[&str] = &[".mp3", ".m4a", ".ogg", ".flac", ".wav", ".opus", ".aac"];

fn is_audio_only(url: &str) -> bool {
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase();
    AUDIO_EXTENSIONS
        .iter()
        .any(|extension| path.ends_with(extension))
}

/// Keeps only links the player can open and sorts them best quality first.
pub fn playable_links_best_first(links: Vec<PlayableLink>) -> Vec<PlayableLink> {
    let mut playable: Vec<PlayableLink> = links
        .into_iter()
        .filter(|link| {
            PLAYABLE_TYPES.contains(&link.link_type.as_str()) || link.link_type.is_empty()
        })
        .collect();
    playable.sort_by_key(|link| (is_audio_only(&link.url), std::cmp::Reverse(link.quality)));
    playable
}

/// Turns a title or episode into playable streams.
///
/// Extensions swallow network errors and report "success" with no links, so an empty answer is
/// retried; if the payload is itself a URL, the extractor registry is tried as a fallback.
pub struct StreamResolver {
    gateway: Arc<dyn ProviderGateway>,
}

impl StreamResolver {
    /// Creates a resolver over the extension runtime.
    pub fn new(gateway: Arc<dyn ProviderGateway>) -> Self {
        Self { gateway }
    }

    /// Resolves the streams and subtitles for `data` (a movie payload or an episode payload).
    pub fn resolve(&self, provider: &str, data: &str) -> Result<PlayableLinks, ServiceError> {
        let mut last = PlayableLinks::default();
        for attempt in 1..=MAX_ATTEMPTS {
            match self.gateway.load_links(provider, data) {
                Ok(found) if !playable_links_best_first(found.links.clone()).is_empty() => {
                    return Ok(self.finish(found));
                }
                Ok(empty) => {
                    tracing::warn!(provider, attempt, "provider returned no playable streams");
                    last = empty;
                }
                Err(error @ (GatewayError::Timeout { .. } | GatewayError::Transport(_))) => {
                    tracing::warn!(provider, attempt, %error, "link loading failed, retrying");
                }
                Err(error) => return Err(error.into()),
            }
        }
        self.extractor_fallback(data, last)
    }

    fn finish(&self, mut found: PlayableLinks) -> PlayableLinks {
        found.links = playable_links_best_first(found.links);
        found
    }

    fn extractor_fallback(
        &self,
        data: &str,
        last: PlayableLinks,
    ) -> Result<PlayableLinks, ServiceError> {
        if !data.starts_with("http") {
            return Ok(last);
        }
        tracing::info!(data, "falling back to the extractor registry");
        let resolved = self.gateway.extract(data, None)?;
        Ok(self.finish(resolved))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeGateway;

    fn link(url: &str, quality: i32, link_type: &str) -> PlayableLink {
        PlayableLink {
            url: url.into(),
            quality,
            link_type: link_type.into(),
            ..PlayableLink::default()
        }
    }

    fn links(items: Vec<PlayableLink>) -> PlayableLinks {
        PlayableLinks {
            success: true,
            links: items,
            subtitles: Vec::new(),
        }
    }

    fn resolver(gateway: FakeGateway) -> (StreamResolver, Arc<FakeGateway>) {
        let gateway = Arc::new(gateway);
        (
            StreamResolver::new(Arc::clone(&gateway) as Arc<dyn ProviderGateway>),
            gateway,
        )
    }

    #[test]
    fn best_first_drops_torrents_and_sorts_by_quality() {
        let sorted = playable_links_best_first(vec![
            link("low", 360, "VIDEO"),
            link("magnet", 9999, "MAGNET"),
            link("high", 1080, "M3U8"),
        ]);
        let urls: Vec<_> = sorted.iter().map(|l| l.url.as_str()).collect();
        assert_eq!(urls, ["high", "low"]);
    }

    #[test]
    fn audio_only_links_come_after_video_even_with_higher_quality() {
        let sorted = playable_links_best_first(vec![
            link("https://h/a.mp3", 720, "VIDEO"),
            link("https://h/v.mp4", 400, "VIDEO"),
        ]);
        assert_eq!(
            sorted.first().map(|l| l.url.as_str()),
            Some("https://h/v.mp4")
        );
    }

    #[test]
    fn empty_answer_is_retried_and_later_success_wins() {
        let gateway = FakeGateway::default();
        {
            let mut replies = gateway.link_replies.lock().expect("lock");
            replies.push_back(Ok(links(vec![])));
            replies.push_back(Ok(links(vec![link("ok", 720, "VIDEO")])));
        }
        let (resolver, gateway) = resolver(gateway);
        let resolved = resolver.resolve("P", "data").expect("resolves");
        assert_eq!(resolved.links[0].url, "ok");
        assert_eq!(*gateway.load_links_calls.lock().expect("lock"), 2);
    }

    #[test]
    fn timeouts_are_retried_but_remote_errors_are_not() {
        let gateway = FakeGateway::default();
        gateway
            .link_replies
            .lock()
            .expect("lock")
            .push_back(Err(GatewayError::Remote {
                kind: "X".into(),
                message: "bad".into(),
            }));
        let (resolver, gateway) = resolver(gateway);
        assert!(resolver.resolve("P", "data").is_err());
        assert_eq!(*gateway.load_links_calls.lock().expect("lock"), 1);
    }

    #[test]
    fn exhausted_retries_fall_back_to_the_extractor_for_url_payloads() {
        let gateway = FakeGateway::default();
        *gateway.extract_reply.lock().expect("lock") =
            Some(links(vec![link("via-extractor", 480, "VIDEO")]));
        for _ in 0..MAX_ATTEMPTS {
            gateway
                .link_replies
                .lock()
                .expect("lock")
                .push_back(Ok(links(vec![])));
        }
        let (resolver, _) = resolver(gateway);
        let resolved = resolver
            .resolve("P", "https://host.test/v")
            .expect("falls back");
        assert_eq!(resolved.links[0].url, "via-extractor");
    }

    #[test]
    fn exhausted_retries_without_url_payload_return_empty_not_error() {
        let gateway = FakeGateway::default();
        for _ in 0..MAX_ATTEMPTS {
            gateway
                .link_replies
                .lock()
                .expect("lock")
                .push_back(Ok(links(vec![])));
        }
        let (resolver, _) = resolver(gateway);
        assert!(resolver
            .resolve("P", "identifier")
            .expect("no error")
            .links
            .is_empty());
    }
}
