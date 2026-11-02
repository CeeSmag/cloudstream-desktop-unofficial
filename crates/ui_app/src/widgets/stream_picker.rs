use core_model::PlayableLink;

use crate::actions::UiAction;

/// A window listing the available streams; returns the user's choice.
pub(crate) fn show(ctx: &egui::Context, title: &str, links: &[PlayableLink]) -> Option<UiAction> {
    let mut action = None;
    egui::Window::new(format!("Choose a stream - {title}"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            for (index, link) in links.iter().enumerate() {
                if ui.button(app_services::describe_link(link)).clicked() {
                    action = Some(UiAction::PlayLink(index));
                }
            }
            ui.separator();
            if ui.button("Cancel").clicked() {
                action = Some(UiAction::DismissStreamChoice);
            }
        });
    action
}
