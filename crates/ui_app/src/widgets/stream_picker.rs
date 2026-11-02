use core_model::PlayableLink;

use crate::actions::UiAction;

/// Label for one stream in the picker, for example `1080p - Provider (mp4)`.
pub(crate) fn describe(link: &PlayableLink) -> String {
    let quality = if link.quality > 0 {
        format!("{}p", link.quality)
    } else {
        "auto".to_owned()
    };
    let name = if link.name.is_empty() {
        &link.source
    } else {
        &link.name
    };
    format!("{quality} - {name}")
}

/// A window listing the available streams; returns the user's choice.
pub(crate) fn show(ctx: &egui::Context, title: &str, links: &[PlayableLink]) -> Option<UiAction> {
    let mut action = None;
    egui::Window::new(format!("Choose a stream - {title}"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            for (index, link) in links.iter().enumerate() {
                if ui.button(describe(link)).clicked() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_shows_quality_and_name() {
        let link = PlayableLink {
            name: "Server 1".into(),
            quality: 1080,
            ..PlayableLink::default()
        };
        assert_eq!(describe(&link), "1080p - Server 1");
    }

    #[test]
    fn describe_falls_back_to_auto_and_source() {
        let link = PlayableLink {
            source: "Hoster".into(),
            ..PlayableLink::default()
        };
        assert_eq!(describe(&link), "auto - Hoster");
    }
}
