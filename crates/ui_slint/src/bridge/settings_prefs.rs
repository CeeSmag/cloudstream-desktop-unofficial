//! Language, wipe, and repository settings callbacks.

use std::cell::RefCell;
use std::rc::Rc;

use app_services::UiServices;
use slint::ComponentHandle;

use crate::runner::{EventPump, UiEvent};
use crate::CloudStream;

use super::session::AppExtras;
use super::settings_extra::tx_notice;

pub(crate) fn wire_settings_prefs(
    some_ui: &CloudStream,
    ui: slint::Weak<CloudStream>,
    services: Rc<UiServices>,
    pump: Rc<EventPump>,
    extras: Rc<RefCell<AppExtras>>,
) {
    {
        let extras = Rc::clone(&extras);
        let pump = Rc::clone(&pump);
        some_ui.on_confirm_wipe(move || {
            if let Some(dir) = extras.borrow().data_dir.clone() {
                let _ = std::fs::write(dir.join("delete_all_data.pending"), b"1");
                let _ = tx_notice(&pump, "Data wipe scheduled — restart the app");
            }
        });
    }
    {
        some_ui.on_cancel_wipe(|| {});
    }
    {
        let services = Rc::clone(&services);
        let ui = ui.clone();
        some_ui.on_set_auto_subtitles(move |on| {
            let _ = services.settings.set_auto_select_subtitles(on);
            if let Some(ui) = ui.upgrade() {
                ui.set_auto_subtitles(on);
            }
        });
    }
    {
        let services = Rc::clone(&services);
        let ui = ui.clone();
        some_ui.on_set_audio(move |label| {
            let language = if label == "Stream default" {
                None
            } else {
                player_api::all_languages()
                    .iter()
                    .find(|l| l.name == label.as_str())
                    .copied()
            };
            let _ = services.settings.set_preferred_audio(language);
            if let Some(playback) = &services.playback {
                playback.set_preferred_audio(language);
            }
            if let Some(ui) = ui.upgrade() {
                ui.set_audio_value(label);
            }
        });
    }
    {
        let services = Rc::clone(&services);
        let ui = ui.clone();
        some_ui.on_set_subtitle_add(move |label| {
            let Some(language) = player_api::all_languages()
                .iter()
                .find(|l| l.name == label.as_str())
                .copied()
            else {
                return;
            };
            let mut current = services.settings.preferred_subtitles().unwrap_or_default();
            if !current.iter().any(|l| l.code() == language.code()) {
                current.push(language);
                current.truncate(app_services::MAX_PREFERRED_SUBTITLES);
            }
            let _ = services.settings.set_preferred_subtitles(&current);
            if let Some(playback) = &services.playback {
                playback.set_preferred_subtitles(&current);
            }
            if let Some(ui) = ui.upgrade() {
                let chips = current
                    .iter()
                    .map(|l| l.name)
                    .collect::<Vec<_>>()
                    .join(", ");
                ui.set_subtitle_chips(chips.into());
                ui.set_subtitle_add_value(label);
            }
        });
    }
    {
        let services = Rc::clone(&services);
        let ui = ui.clone();
        some_ui.on_clear_subtitles(move || {
            let _ = services.settings.set_preferred_subtitles(&[]);
            if let Some(playback) = &services.playback {
                playback.set_preferred_subtitles(&[]);
            }
            if let Some(ui) = ui.upgrade() {
                ui.set_subtitle_chips("".into());
            }
        });
    }
    {
        let services = Rc::clone(&services);
        let pump = Rc::clone(&pump);
        let ui = ui.clone();
        some_ui.on_install_repo(move |url| {
            let url = url.to_string();
            if url.is_empty() {
                let _ = tx_notice(&pump, "Enter a repository URL");
                return;
            }
            match services.repositories.add(&url) {
                Ok(entry) => {
                    let _ = tx_notice(&pump, &format!("Added repository {}", entry.name));
                }
                Err(error) => {
                    let _ = tx_notice(&pump, &format!("Repository failed: {error}"));
                }
            }
            let _ = ui.upgrade_in_event_loop(|_| {});
        });
    }
}
