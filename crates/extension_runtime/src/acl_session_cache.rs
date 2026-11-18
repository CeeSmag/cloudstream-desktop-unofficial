//! Remembers AppContainer ACL grants for this process so `icacls` is not repeated.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock, PoisonError};

/// Cache key: canonical path + SID + rights string.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn grant_key(path: &str, sid: &str, rights: &str) -> String {
    format!("{path}\0{sid}\0{rights}")
}

fn cache() -> &'static Mutex<HashSet<String>> {
    static CACHE: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Returns true when this grant was already applied in this process.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn already_granted(key: &str) -> bool {
    cache()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .contains(key)
}

/// Records a successful grant for the rest of this process.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn note_granted(key: String) {
    cache()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key);
}

#[cfg(test)]
pub(crate) fn clear_for_tests() {
    cache()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_look_up_hits_the_session_cache() {
        clear_for_tests();
        let key = grant_key("/data/extensions", "S-1-5-test", "(OI)(CI)(RX)");
        assert!(!already_granted(&key));
        note_granted(key.clone());
        assert!(already_granted(&key));
        assert!(!already_granted(&grant_key(
            "/other",
            "S-1-5-test",
            "(OI)(CI)(RX)"
        )));
        clear_for_tests();
        assert!(!already_granted(&key));
    }
}
