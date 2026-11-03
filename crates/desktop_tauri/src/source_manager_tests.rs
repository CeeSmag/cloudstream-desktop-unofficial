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
    SourceManager::new(StreamChecker::new(Arc::new(Unreachable)))
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
    let generation = manager.open(PlaybackItem::default());
    let active = manager.session().snapshot().expect("open");
    assert!(active.links.is_empty());
    assert!(manager.session().is_current(generation));
}
