//! Property-based tests for stream link filtering, pipe normalization, and quality ranking.

use app_services::{
    is_audio_only_url, link_rank, normalize_clear_key, normalize_pipe_link,
    playable_links_best_first,
};
use core_model::PlayableLink;
use proptest::prelude::*;

fn link_strategy() -> impl Strategy<Value = PlayableLink> {
    let ext = prop_oneof![
        Just(".mp4"),
        Just(".m3u8"),
        Just(".mkv"),
        Just(".mp3"),
        Just(".m4a"),
        Just(".opus"),
        Just(".aac"),
    ];
    let kind = prop_oneof![
        Just("VIDEO"),
        Just("M3U8"),
        Just("DASH"),
        Just(""),
        Just("TORRENT"),
        Just("MAGNET"),
    ];
    ("[a-z]{3,8}", ext, -100i32..4320i32, kind, prop::bool::ANY).prop_map(
        |(name, ext, quality, link_type, with_query)| {
            let suffix = if with_query { "?sig=abc" } else { "" };
            PlayableLink {
                source: "Prop".into(),
                name: name.clone(),
                url: format!("https://cdn.example/{name}{ext}{suffix}"),
                quality,
                link_type: link_type.into(),
                ..PlayableLink::default()
            }
        },
    )
}

proptest! {
    #[test]
    fn playable_links_best_first_drops_non_playable_and_orders_video_before_audio(
        links in prop::collection::vec(link_strategy(), 0..40)
    ) {
        let ranked = playable_links_best_first(links);
        for link in &ranked {
            prop_assert!(
                matches!(link.link_type.as_str(), "VIDEO" | "M3U8" | "DASH" | ""),
                "unexpected link_type {}", link.link_type
            );
        }
        for pair in ranked.windows(2) {
            let a_audio = is_audio_only_url(&pair[0].url);
            let b_audio = is_audio_only_url(&pair[1].url);
            if a_audio == b_audio {
                prop_assert!(pair[0].quality >= pair[1].quality);
            } else {
                prop_assert!(!a_audio && b_audio, "video must precede audio-only");
            }
        }
    }

    #[test]
    fn link_rank_assigns_minus_one_to_audio_and_quality_to_video(
        quality in 0i32..4320i32,
        is_audio in prop::bool::ANY,
    ) {
        let url = if is_audio {
            "https://cdn.example/track.mp3?token=1"
        } else {
            "https://cdn.example/video.m3u8?token=1"
        };
        let link = PlayableLink {
            url: url.into(),
            quality,
            link_type: "VIDEO".into(),
            ..PlayableLink::default()
        };
        if is_audio {
            prop_assert_eq!(link_rank(&link), -1);
        } else {
            prop_assert_eq!(link_rank(&link), quality);
        }
    }

    #[test]
    fn normalize_pipe_link_strips_pipe_options_from_url(
        base in "https://[a-z]{3,8}\\.example/[a-z]{3,8}\\.m3u8",
        referer in "https://[a-z]{3,8}\\.example/",
    ) {
        let link = PlayableLink {
            url: format!("{base}|Referer={referer}"),
            ..PlayableLink::default()
        };
        let normalized = normalize_pipe_link(link);
        prop_assert_eq!(normalized.url, base);
        prop_assert_eq!(normalized.referer, referer);
    }

    #[test]
    fn normalize_clear_key_never_panics_on_arbitrary_strings(
        kid in proptest::option::of(".*"),
        key in proptest::option::of(".*"),
    ) {
        let _ = normalize_clear_key(kid.as_deref(), key.as_deref());
    }
}
