//! The rule against every listing of 39 real repositories (767 plugins, captured Nov 2026 into
//! `fixtures/listing_census.json`). Totals are pinned so a change of rule shows its effect in review:
//! if one moves, check the plugins that moved and update the numbers on purpose.
#![allow(clippy::expect_used, clippy::indexing_slicing)] // test code: failing fast is the point

use content_policy::{Allowed, ContentPolicy, PluginFacts, Verdict};
use serde_json::Value;

struct Row {
    repository_url: String,
    internal_name: String,
    tv_types: Vec<String>,
}

fn census() -> Vec<Row> {
    let parsed: Value =
        serde_json::from_str(include_str!("fixtures/listing_census.json")).expect("census is JSON");
    let text = |row: &Value, key: &str| row[key].as_str().unwrap_or_default().to_owned();
    parsed
        .as_array()
        .expect("census is an array")
        .iter()
        .map(|row| Row {
            repository_url: text(row, "repositoryUrl"),
            internal_name: text(row, "internalName"),
            tv_types: row["tvTypes"]
                .as_array()
                .map(|types| {
                    types
                        .iter()
                        .filter_map(|t| t.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect()
}

fn verdicts() -> Vec<(Row, Verdict)> {
    let policy = ContentPolicy::standard();
    census()
        .into_iter()
        .map(|row| {
            let verdict = policy.judge(&PluginFacts {
                repository_url: &row.repository_url,
                internal_name: &row.internal_name,
                declared_types: &row.tv_types,
            });
            (row, verdict)
        })
        .collect()
}

fn names(all: &[(Row, Verdict)], keep: impl Fn(&Verdict) -> bool) -> Vec<String> {
    let mut names: Vec<String> = all
        .iter()
        .filter(|(_, verdict)| keep(verdict))
        .map(|(row, _)| row.internal_name.clone())
        .collect();
    names.sort();
    names.dedup();
    names
}

#[test]
fn totals_are_pinned() {
    let all = verdicts();
    assert_eq!(all.len(), 767);
    assert_eq!(all.iter().filter(|(_, v)| v.is_video()).count(), 732);
    assert_eq!(
        all.iter()
            .filter(|(_, v)| matches!(v, Verdict::Video(Allowed::Reviewed(_))))
            .count(),
        16,
        "every reviewed entry matches its plugin in the census"
    );
}

#[test]
fn exactly_games_audio_and_tools_are_refused() {
    let refused = names(&verdicts(), |v| !v.is_video());
    let expected = [
        // Games (errorcode26/Ayu-CloudStream-Games)
        "AriaOfSorrow",
        "AvatarAirbender",
        "Castlevania",
        "Contra",
        "DesertStrike",
        "Doom",
        "FlappyBird",
        "Game2048",
        "JackieChan",
        "LittleMermaid",
        "MedalOfHonor",
        "MetalSlugAdvance",
        "MinishCap",
        "OnePiece",
        "Pokemon",
        "PrinceOfPersia",
        "SeriousSam",
        "SuperMario",
        "TekkenAdvance",
        "TombRaider",
        "Zelda",
        // Audio
        "GoldenAudiobook",
        "GoldenAudiobooks",
        "LibriVoxAudiobook",
        "SimpMusic",
        // Tools and app features
        "AutoExtensions",
        "SectionOrganizer",
        "SleepGuard",
        "SyncPlugin",
        "SyncProvider",
        "SyncStream",
        "Ultima",
        "WatchParty",
        "Wrapped",
    ];
    let mut expected = expected.map(str::to_owned).to_vec();
    expected.sort();
    assert_eq!(refused, expected);
}

#[test]
fn video_plugins_with_odd_labels_and_reviewed_sites_are_kept() {
    let all = verdicts();
    let video = names(&all, Verdict::is_video);
    for name in [
        "Serienstream",
        "Tamilian",
        "BingeCloud",
        "Aniworld",
        "Cimatn",
        "OscarTv", // odd labels
        "YouTube",
        "Nebula",
        "DailymotionProvider",
        "InternetArchiveProvider",
        "InvidiousProvider",
        "FootReplays",
        "FullReplays",
        "C3TV",
        "SkillShareProvider",
        "MegaProvider", // reviewed
    ] {
        assert!(video.contains(&name.to_owned()), "{name} must be kept");
    }
}

#[test]
fn every_reviewed_entry_is_found_in_the_census() {
    let all = verdicts();
    for entry in content_policy::REVIEWED {
        let found = all.iter().any(|(row, verdict)| {
            row.internal_name == entry.internal_name
                && matches!(verdict, Verdict::Video(Allowed::Reviewed(why)) if *why == entry.reason)
        });
        assert!(found, "{entry:?} matches no census listing");
    }
}
