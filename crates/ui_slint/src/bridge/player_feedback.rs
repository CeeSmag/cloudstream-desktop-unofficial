//! The pop-up in the middle of the player that confirms what the viewer just did.

use crate::CloudStream;

/// Shows `text` (with a level bar from 0 to 1 when given) for a moment; a newer one replaces it.
pub(crate) fn show_feedback(ui: &CloudStream, text: String, level: Option<f32>) {
    ui.set_player_feedback(text.into());
    ui.set_player_feedback_level(level.map_or(-1.0, |level| level.clamp(0.0, 1.0)));
    ui.set_player_feedback_count(ui.get_player_feedback_count().wrapping_add(1));
}
