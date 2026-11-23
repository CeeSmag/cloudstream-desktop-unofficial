use super::*;
use crate::review_list::ReviewedPlugin;

const GAMES: &str =
    "https://raw.githubusercontent.com/errorcode26/Ayu-CloudStream-Games/builds/repo.json";
const DOGIOR: &str =
    "https://raw.githubusercontent.com/doGior/doGiorsHadEnough/refs/heads/builds/repo.json";
const MEGA: &str = "https://raw.githubusercontent.com/self-similarity/MegaRepo/builds/repo.json";
const ANY: &str = "https://example.org/repo.json";

fn labels(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

fn judge(repository_url: &str, internal_name: &str, types: &[&str]) -> Verdict {
    let declared_types = labels(types);
    ContentPolicy::standard().judge(&PluginFacts {
        repository_url,
        internal_name,
        declared_types: &declared_types,
    })
}

#[test]
fn any_video_type_makes_a_video_plugin() {
    let verdict = judge(ANY, "Mixed", &["Movie", "Others", "Music"]);
    assert_eq!(
        verdict,
        Verdict::Video(Allowed::ByTypes(vec![MediaType::Movie]))
    );
    for single in [
        "Movie",
        "AnimeMovie",
        "TvSeries",
        "Cartoon",
        "Anime",
        "OVA",
        "Documentary",
        "AsianDrama",
        "Video",
        "Live",
        "Torrent",
        "NSFW",
    ] {
        assert!(judge(ANY, "P", &[single]).is_video(), "{single}");
    }
}

#[test]
fn normalised_spellings_count_as_video() {
    assert!(judge(ANY, "Serienstream", &["Series"]).is_video());
    assert!(judge(ANY, "Tamilian", &["Movies"]).is_video());
    assert!(judge(ANY, "BingeCloud", &["Movies", "TV Series", "Anime"]).is_video());
    assert!(judge(ANY, "Packed", &["Movie,Anime,Cartoon"]).is_video());
}

#[test]
fn games_audio_and_tools_are_not_video() {
    let doom = judge(GAMES, "Doom", &["Others"]);
    assert_eq!(
        doom,
        Verdict::NotVideo(Refusal {
            types: vec![MediaType::Others],
            unrecognised: vec![]
        })
    );
    for types in [
        &["Music"][..],
        &["AudioBook"],
        &["Podcast"],
        &["Audio"],
        &["CustomMedia"],
    ] {
        assert!(!judge(ANY, "P", types).is_video(), "{types:?}");
    }
}

#[test]
fn no_or_unreadable_types_are_not_video() {
    assert_eq!(
        judge(ANY, "SyncProvider", &[]),
        Verdict::NotVideo(Refusal {
            types: vec![],
            unrecognised: vec![]
        })
    );
    assert_eq!(
        judge(ANY, "SectionOrganizer", &["All"]),
        Verdict::NotVideo(Refusal {
            types: vec![],
            unrecognised: vec!["All".to_owned()]
        })
    );
}

#[test]
fn reviewed_plugins_are_allowed_whatever_they_declare() {
    assert!(matches!(
        judge(DOGIOR, "YouTube", &["Others"]),
        Verdict::Video(Allowed::Reviewed(_))
    ));
    assert!(matches!(
        judge(MEGA, "MegaProvider", &[]),
        Verdict::Video(Allowed::Reviewed(_))
    ));
    // The same name in another repository is judged by its types.
    assert!(!judge(GAMES, "YouTube", &["Others"]).is_video());
}

#[test]
fn the_reviewed_list_is_injected() {
    static ONLY_DOOM: &[ReviewedPlugin] = &[ReviewedPlugin {
        repository: "github.com/errorcode26/ayu-cloudstream-games",
        internal_name: "Doom",
        reason: "test entry",
    }];
    let policy = ContentPolicy::with_reviewed(ONLY_DOOM);
    let doom = policy.judge(&PluginFacts {
        repository_url: GAMES,
        internal_name: "Doom",
        declared_types: &[],
    });
    assert_eq!(doom, Verdict::Video(Allowed::Reviewed("test entry")));
    let youtube = policy.judge(&PluginFacts {
        repository_url: DOGIOR,
        internal_name: "YouTube",
        declared_types: &labels(&["Others"]),
    });
    assert!(!youtube.is_video(), "the standard list is not consulted");
}

#[test]
fn refusals_explain_themselves() {
    let text = |v: Verdict| match v {
        Verdict::NotVideo(refusal) => refusal.to_string(),
        Verdict::Video(_) => panic!("expected a refusal"),
    };
    assert_eq!(
        text(judge(GAMES, "Doom", &["Others"])),
        "not a video plugin (it declares Others)"
    );
    assert_eq!(
        text(judge(ANY, "S", &["All"])),
        "not a video plugin (it declares only \"All\")"
    );
    assert_eq!(
        text(judge(ANY, "S", &[])),
        "not a video plugin (it declares no content type)"
    );
}

#[test]
fn providers_follow_their_plugin_and_their_own_types() {
    let policy = ContentPolicy::standard();
    let by_types = judge(ANY, "Youtube", &["Movie", "Music"]);
    assert!(policy.shows_provider(&by_types, &labels(&["Movie"])));
    assert!(
        policy.shows_provider(&by_types, &[]),
        "no types: follows the plugin"
    );
    assert!(
        !policy.shows_provider(&by_types, &labels(&["Music"])),
        "a music provider is hidden"
    );

    let reviewed = judge(DOGIOR, "YouTube", &["Others"]);
    assert!(
        policy.shows_provider(&reviewed, &labels(&["Others"])),
        "reviewed plugin's providers stay"
    );

    let refused = judge(GAMES, "Doom", &["Others"]);
    assert!(
        !policy.shows_provider(&refused, &labels(&["Movie"])),
        "a refused plugin shows nothing"
    );
}
