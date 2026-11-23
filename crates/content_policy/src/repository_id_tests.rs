use super::*;

fn id(url: &str) -> String {
    RepositoryId::of(url).as_str().to_owned()
}

#[test]
fn every_spelling_of_a_github_repository_is_one_identity() {
    let spellings = [
        "https://raw.githubusercontent.com/doGior/doGiorsHadEnough/refs/heads/builds/repo.json",
        "https://raw.githubusercontent.com/doGior/doGiorsHadEnough/builds/repo.json",
        "https://github.com/doGior/doGiorsHadEnough/raw/builds/repo.json",
        "https://cdn.jsdelivr.net/gh/doGior/doGiorsHadEnough@builds/repo.json",
        "http://RAW.githubusercontent.com/dogior/dogiorshadenough/builds/repo.json?x=1#y",
    ];
    for url in spellings {
        assert_eq!(id(url), "github.com/dogior/dogiorshadenough", "{url}");
    }
}

#[test]
fn gitlab_repositories_are_owner_and_name() {
    assert_eq!(
        id("https://gitlab.com/tearrs/cloudstream-vietnamese/-/raw/main/repo.json"),
        "gitlab.com/tearrs/cloudstream-vietnamese"
    );
}

#[test]
fn other_hosts_keep_their_path() {
    assert_eq!(
        id("https://cs.eclipsia.dpdns.org/repo.json"),
        "cs.eclipsia.dpdns.org/repo.json"
    );
    assert_eq!(id("example.org"), "example.org");
}

#[test]
fn different_repositories_stay_different() {
    assert_ne!(
        id("https://raw.githubusercontent.com/recloudstream/extensions/master/repo.json"),
        id("https://raw.githubusercontent.com/Luna712/Luna712-CloudStream-Extensions/master/repo.json")
    );
    // A fork is another repository.
    assert_ne!(
        id("https://raw.githubusercontent.com/a/extensions/master/repo.json"),
        id("https://raw.githubusercontent.com/b/extensions/master/repo.json")
    );
}

#[test]
fn malformed_addresses_do_not_panic() {
    for url in [
        "",
        "://",
        "https://",
        "https://raw.githubusercontent.com/",
        "https://cdn.jsdelivr.net/gh/x",
    ] {
        let _ = RepositoryId::of(url);
    }
}
