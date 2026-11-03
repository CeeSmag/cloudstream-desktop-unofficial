use std::sync::Arc;

use extension_repo::{HttpProbe, ProbeResponse, RepoError};

use super::*;

struct Unreachable;

impl HttpProbe for Unreachable {
    fn probe(&self, url: &str, _: &[(String, String)]) -> Result<ProbeResponse, RepoError> {
        Err(RepoError::Download {
            url: url.to_owned(),
            reason: "unreachable".to_owned(),
        })
    }
}

fn manager() -> SourceManager {
    SourceManager::new(
        StreamChecker::new(Arc::new(Unreachable)),
        crate::test_subtitles::unused_loader(),
    )
}

#[test]
fn beginning_a_new_video_cancels_the_previous_one() {
    let manager = manager();
    let first = manager.begin();
    let second = manager.begin();
    assert!(first.is_cancelled() && !second.is_cancelled());
    manager.close();
    assert!(second.is_cancelled());
    assert!(manager.session().snapshot().is_none());
}

#[test]
fn opening_a_video_starts_an_empty_list_of_streams() {
    let manager = manager();
    let gate = manager.begin();
    let generation = manager.open(&gate, PlaybackItem::default()).expect("open");
    let active = manager.session().snapshot().expect("open");
    assert!(active.links.is_empty());
    assert!(manager.session().is_current(generation));
}

#[test]
fn a_video_that_was_replaced_cannot_open_its_list_over_the_newer_one() {
    let manager = manager();
    let first = manager.begin();
    let second = manager.begin();
    let newer = manager
        .open(&second, PlaybackItem::default())
        .expect("the newest video opens");
    assert!(
        manager.open(&first, PlaybackItem::default()).is_none(),
        "the replaced video must not take the session over"
    );
    assert!(manager.session().is_current(newer));
}

#[test]
fn a_closed_video_cannot_open_a_list() {
    let manager = manager();
    let gate = manager.begin();
    manager.close();
    assert!(manager.open(&gate, PlaybackItem::default()).is_none());
    assert!(manager.session().snapshot().is_none());
}
