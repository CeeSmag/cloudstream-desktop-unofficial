//! When the mouse pointer is hidden.
//!
//! Slint hands the pointer's look to the operating system only while a mouse event is handled,
//! so a `mouse-cursor` binding cannot hide it when the controls fade out by themselves (no event
//! follows). The window therefore sets it explicitly, and this rule says what it should be and
//! remembers what was last set, so the system is asked only when something changes.

/// The pointer is hidden exactly while the player is on screen and has hidden its controls.
pub(crate) fn cursor_hidden(player_open: bool, chrome_visible: bool) -> bool {
    player_open && !chrome_visible
}

/// What the pointer was last set to.
#[derive(Debug, Default)]
pub(crate) struct CursorPolicy {
    hidden: bool,
}

impl CursorPolicy {
    /// The new state to set, or `None` when the pointer is already as it should be.
    pub(crate) fn change_for(&self, player_open: bool, chrome_visible: bool) -> Option<bool> {
        let wanted = cursor_hidden(player_open, chrome_visible);
        (wanted != self.hidden).then_some(wanted)
    }

    /// Records that the pointer was set to `hidden` (call it only when the system took it).
    pub(crate) fn applied(&mut self, hidden: bool) {
        self.hidden = hidden;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_only_while_the_player_is_open_with_hidden_controls() {
        assert!(cursor_hidden(true, false));
        assert!(!cursor_hidden(true, true), "controls on screen");
        assert!(!cursor_hidden(false, false), "player closed");
        assert!(!cursor_hidden(false, true));
    }

    #[test]
    fn asks_the_system_only_when_the_wanted_state_changes() {
        let mut policy = CursorPolicy::default();
        assert_eq!(policy.change_for(true, true), None, "visible already");
        assert_eq!(policy.change_for(true, false), Some(true));
        policy.applied(true);
        assert_eq!(policy.change_for(true, false), None, "already hidden");
        assert_eq!(policy.change_for(true, true), Some(false), "controls back");
        policy.applied(false);
        assert_eq!(policy.change_for(true, true), None);
    }

    #[test]
    fn closing_the_player_while_hidden_shows_the_pointer_again() {
        let mut policy = CursorPolicy::default();
        policy.applied(true);
        assert_eq!(policy.change_for(false, false), Some(false));
    }

    #[test]
    fn a_change_the_system_refused_is_asked_again() {
        let policy = CursorPolicy::default();
        assert_eq!(policy.change_for(true, false), Some(true));
        assert_eq!(
            policy.change_for(true, false),
            Some(true),
            "not applied yet"
        );
    }
}
