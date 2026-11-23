use super::*;

const REPO: &str = "https://cdn.example/repo.json";

fn installed(declared_types: Option<Vec<&str>>) -> InstalledExtension {
    InstalledExtension {
        file_name: "P.cs3".into(),
        internal_name: "P".into(),
        repository_url: Some(REPO.into()),
        declared_types: declared_types.map(|types| types.into_iter().map(str::to_owned).collect()),
        ..InstalledExtension::default()
    }
}

fn provider(types: &[&str]) -> ProviderInfo {
    ProviderInfo {
        name: "P".into(),
        types: types.iter().map(|t| (*t).to_owned()).collect(),
        ..ProviderInfo::default()
    }
}

#[test]
fn installed_extensions_are_judged_only_when_their_types_are_known() {
    let policy = ContentPolicy::standard();
    assert_eq!(judge_installed(&policy, &installed(None)), None);
    assert!(judge_installed(&policy, &installed(Some(vec!["Movie"]))).is_some_and(|v| v.is_video()));
    assert!(
        judge_installed(&policy, &installed(Some(vec!["Others"]))).is_some_and(|v| !v.is_video())
    );
    assert!(
        judge_installed(&policy, &installed(Some(vec![]))).is_some_and(|v| !v.is_video()),
        "known to declare nothing is not the same as unknown"
    );
}

#[test]
fn provider_types_decide_for_extensions_installed_before_types_were_recorded() {
    let policy = ContentPolicy::standard();
    let (verdict, types) = judge_by_providers(
        &policy,
        &installed(None),
        &[provider(&["Others"]), provider(&["Movie", "Others"])],
    );
    assert!(verdict.is_video());
    assert_eq!(
        types,
        vec!["Others".to_owned(), "Movie".to_owned()],
        "union, first seen first"
    );

    let (game, _) = judge_by_providers(&policy, &installed(None), &[provider(&["Others"])]);
    assert!(!game.is_video());
    let (nothing, none) = judge_by_providers(&policy, &installed(None), &[]);
    assert!(!nothing.is_video());
    assert!(none.is_empty());
}

#[test]
fn support_follows_the_verdict() {
    let policy = ContentPolicy::standard();
    assert_eq!(support_of(None), Support::Unknown);
    let video = judge_installed(&policy, &installed(Some(vec!["Anime"])));
    assert_eq!(support_of(video.as_ref()), Support::Supported);
    let game = judge_installed(&policy, &installed(Some(vec!["Others"])));
    assert_eq!(
        support_of(game.as_ref()),
        Support::NotVideo("not a video plugin (it declares Others)".into())
    );
}

#[test]
fn a_listing_is_named_by_its_display_name_when_it_has_one() {
    let mut listing = PluginListing {
        internal_name: "Doom".into(),
        ..PluginListing::default()
    };
    assert_eq!(listing_label(&listing), "Doom");
    listing.name = "DOOM".into();
    assert_eq!(listing_label(&listing), "DOOM");
}
