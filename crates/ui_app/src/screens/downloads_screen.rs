use crate::widgets::empty_state;

/// The downloads tab. Downloading is not implemented yet; the tab keeps CloudStream's layout.
pub(crate) fn show(ui: &mut egui::Ui) {
    empty_state::show(
        ui,
        "Downloads are not available yet",
        "Streams can be played from the details page. Offline downloads are planned.",
    );
}
