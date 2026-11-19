//! Leave/close saves sources; `run_open` itself does not write the title cache.

use app_services::LinkHealth;
use core_model::PlayableLink;
use title_cache::{CacheLimits, LinkHealth as CachedHealth, TitleCache};

use crate::open_types::{cached_title_from_open, save_open_kept, CheckedOpen};

fn folder(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("cs_open_persist_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn link(url: &str) -> PlayableLink {
    PlayableLink {
        url: url.to_owned(),
        name: "src".into(),
        source: "test".into(),
        ..PlayableLink::default()
    }
}

fn opened(urls: &[&str], play: usize) -> CheckedOpen {
    let links = urls
        .iter()
        .map(|url| {
            (
                link(url),
                Some(LinkHealth::Working {
                    size_bytes: Some(100),
                    subtitles: Vec::new(),
                }),
            )
        })
        .collect();
    CheckedOpen {
        links,
        subtitles: Vec::new(),
        play_index: Some(play),
    }
}

#[test]
fn cached_title_from_open_marks_working_and_last_good() {
    let open = opened(&["https://a.test/1", "https://a.test/2"], 1);
    let title = cached_title_from_open(&open);
    assert!(title.complete);
    assert_eq!(title.links.len(), 2);
    assert_eq!(
        title.links.get(1).map(|row| row.health),
        Some(CachedHealth::Working)
    );
    assert_eq!(title.last_good.as_deref(), Some("https://a.test/2"));
}

#[test]
fn save_open_kept_writes_only_while_token_is_current() {
    let dir = folder("kept");
    let cache = TitleCache::new(dir.clone(), CacheLimits::default());
    let kept_a = cache.keep_only("provider\na");
    let open = opened(&["https://a.test/a"], 0);
    save_open_kept(&cache, &kept_a, &open);
    assert!(matches!(
        cache.load("provider\na"),
        title_cache::Lookup::Hit(_)
    ));

    let kept_b = cache.keep_only("provider\nb");
    save_open_kept(&cache, &kept_a, &open);
    assert!(
        matches!(cache.load("provider\na"), title_cache::Lookup::Miss(_)),
        "replaced title must not resurrect after keep_only"
    );
    save_open_kept(&cache, &kept_b, &opened(&["https://b.test/b"], 0));
    assert!(matches!(
        cache.load("provider\nb"),
        title_cache::Lookup::Hit(_)
    ));
    let _ = std::fs::remove_dir_all(&dir);
}
