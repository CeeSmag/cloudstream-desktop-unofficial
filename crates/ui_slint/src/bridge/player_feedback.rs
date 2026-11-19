//! The pop-up in the middle of the player that confirms what the viewer just did.

use crate::CloudStream;

/// Shows `text` with `icon` (and a level bar from 0 to 1 when given); a newer one replaces it.
pub(crate) fn show_feedback(ui: &CloudStream, icon: &str, text: String, level: Option<f32>) {
    ui.set_player_feedback_icon(icon.into());
    ui.set_player_feedback(text.into());
    ui.set_player_feedback_level(level.map_or(-1.0, |level| level.clamp(0.0, 1.0)));
    ui.set_player_feedback_count(ui.get_player_feedback_count().wrapping_add(1));
}
