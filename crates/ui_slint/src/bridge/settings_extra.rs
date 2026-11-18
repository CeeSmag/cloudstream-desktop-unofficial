//! Small settings helpers and Cloudflare callback wiring.

use std::cell::RefCell;
use std::rc::Rc;

use app_services::UiServices;
use slint::ComponentHandle;

use crate::posters::plugin_rows;
use crate::runner::{EventPump, UiEvent};
use crate::CloudStream;

use super::session::{AppExtras, Session};

pub(crate) fn pump_dummy(ui: &slint::Weak<CloudStream>) -> Rc<EventPump> {
    let _ = ui;
    Rc::new(EventPump::new())
}

pub(crate) fn tx_notice(pump: &EventPump, text: &str) -> Result<(), ()> {
    pump.sender()
        .send(UiEvent::Notice(text.into()))
        .map_err(|_| ())
}

pub(crate) fn tx_notice_from_extras(_extras: &Rc<RefCell<AppExtras>>, _text: &str) {}

pub(crate) fn refresh_plugins(ui: &slint::Weak<CloudStream>, services: &UiServices) {
    if let Some(ui) = ui.upgrade() {
        if let Ok(list) = services.extensions.installed() {
            ui.set_plugins(plugin_rows(&list));
        }
    }
}

pub(crate) fn wire_cloudflare(
    some_ui: &CloudStream,
    ui: slint::Weak<CloudStream>,
    session: Rc<RefCell<Session>>,
    extras: Rc<RefCell<AppExtras>>,
) {
    {
        let session = Rc::clone(&session);
        let extras = Rc::clone(&extras);
        let ui = ui.clone();
        some_ui.on_allow_cf_host(move || {
            let host = session.borrow().cf_host.clone();
            if let Some(host) = host {
                session.borrow_mut().allowed_cf.insert(host);
                if let Some(url) = ui.upgrade().map(|u| u.get_cf_url().to_string()) {
                    let _ = open::that(&url);
                }
                if let (Some(dir), Some(id)) = (
                    extras
                        .borrow()
                        .data_dir
                        .as_ref()
                        .map(|d| d.join("sidecar/cloudflare")),
                    session.borrow().cf_id.clone(),
                ) {
                    let _ = cloudflare_bridge::write_result(
                        &dir,
                        &id,
                        &cloudflare_bridge::Outcome::Waiting(
                            "viewer allowed host; complete check in browser".into(),
                        ),
                    );
                }
            }
        });
    }
    {
        let session = Rc::clone(&session);
        let extras = Rc::clone(&extras);
        let ui = ui.clone();
        some_ui.on_cf_done(move || {
            if let (Some(dir), Some(id)) = (
                extras
                    .borrow()
                    .data_dir
                    .as_ref()
                    .map(|d| d.join("sidecar/cloudflare")),
                session.borrow().cf_id.clone(),
            ) {
                let _ = cloudflare_bridge::write_result(
                    &dir,
                    &id,
                    &cloudflare_bridge::Outcome::Passed {
                        user_agent: "Mozilla/5.0 CloudStream".into(),
                        cookies: Vec::new(),
                    },
                );
            }
            if let Some(ui) = ui.upgrade() {
                ui.set_show_cf(false);
            }
        });
    }
    {
        let session = Rc::clone(&session);
        let extras = Rc::clone(&extras);
        let ui = ui.clone();
        some_ui.on_cf_cancel(move || {
            if let (Some(dir), Some(id)) = (
                extras
                    .borrow()
                    .data_dir
                    .as_ref()
                    .map(|d| d.join("sidecar/cloudflare")),
                session.borrow().cf_id.clone(),
            ) {
                let _ = cloudflare_bridge::write_result(
                    &dir,
                    &id,
                    &cloudflare_bridge::Outcome::Declined("viewer cancelled".into()),
                );
            }
            if let Some(ui) = ui.upgrade() {
                ui.set_show_cf(false);
            }
        });
    }
}
