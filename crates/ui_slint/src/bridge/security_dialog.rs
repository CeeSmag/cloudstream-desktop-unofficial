//! Modal security warning and explicit trust-bypass workflow for plugins.

use app_services::{PluginHit, SecurityBlock};
use slint::ComponentHandle;

use crate::posters::string_model;
use crate::{CloudStream, SecurityDialog};

use super::bridge_context::BridgeContext;
use super::extension_catalog::refresh_plugins;
use super::extension_changes::{apply_set_trusted, install_plugin_trusted};

/// What confirming the open security warning modal will execute.
#[derive(Clone)]
pub(crate) enum PendingTrustAction {
    InstallTrusted { hit: Box<PluginHit> },
    TrustInstalled { file_name: String },
}

const TRUST_WARNING: &str = "Security warning: Trusting this plugin skips bytecode security verification and disables the OS filesystem sandbox for its worker process. Malicious plugins could read or modify your files or scan your local network. Only trust plugins from authors you know and trust.";

/// Connects the SecurityDialog global's confirm/cancel callbacks.
pub(crate) fn wire_security_dialog(ctx: &BridgeContext, ui: &CloudStream) {
    let dialog = ui.global::<SecurityDialog>();
    let c = ctx.clone();
    dialog.on_confirm(move || confirm_security_dialog(&c));
    let c = ctx.clone();
    dialog.on_cancel(move || {
        if let Some(ui) = c.window() {
            close_security_dialog(&c, &ui);
        }
    });
}

/// Shows the security dialog after a plugin install or trust reload was blocked.
pub(crate) fn open_security_blocked(
    ctx: &BridgeContext,
    ui: &CloudStream,
    hit: Option<PluginHit>,
    block: SecurityBlock,
) {
    let can_trust_install = block.can_trust && hit.is_some();
    ctx.session.borrow_mut().pending_trust = if can_trust_install {
        hit.map(|hit| PendingTrustAction::InstallTrusted { hit: Box::new(hit) })
    } else {
        None
    };
    let (headline, summary, warning, cancel) = blocked_copy(can_trust_install);
    let dialog = ui.global::<SecurityDialog>();
    dialog.set_plugin_name(block.plugin.clone().into());
    dialog.set_headline(headline.into());
    dialog.set_summary(summary.into());
    dialog.set_reasons(string_model(block.explanations()));
    dialog.set_warning_text(warning.into());
    dialog.set_technical_details(block.technical_details.join("\n").into());
    dialog.set_show_details(false);
    dialog.set_can_trust(can_trust_install);
    dialog.set_confirm_label("Trust & Install anyway".into());
    dialog.set_cancel_label(cancel.into());
    dialog.set_open(true);
}

fn blocked_copy(can_trust: bool) -> (&'static str, &'static str, &'static str, &'static str) {
    if can_trust {
        (
            "Blocked for security",
            "This plugin uses restricted system features and was not installed. Review what it tried to access below before deciding whether to trust it.",
            TRUST_WARNING,
            "Keep blocked",
        )
    } else {
        (
            "Blocked by core security policy",
            "This plugin attempted a restricted core system operation that CloudStream blocks even for trusted plugins, so it cannot be loaded.",
            "",
            "Close",
        )
    }
}

/// Shows the warning confirmation before turning Trust ON for an already-installed plugin.
pub(crate) fn open_trust_confirm(
    ctx: &BridgeContext,
    ui: &CloudStream,
    file_name: String,
    plugin_name: String,
) {
    ctx.session.borrow_mut().pending_trust = Some(PendingTrustAction::TrustInstalled { file_name });
    let dialog = ui.global::<SecurityDialog>();
    dialog.set_plugin_name(plugin_name.into());
    dialog.set_headline("Trust this plugin?".into());
    dialog.set_summary(
        "Trusted plugins run with reduced isolation. Confirm only if you trust this plugin's source."
            .into(),
    );
    dialog.set_reasons(string_model(Vec::new()));
    dialog.set_warning_text(TRUST_WARNING.into());
    dialog.set_technical_details("".into());
    dialog.set_show_details(false);
    dialog.set_can_trust(true);
    dialog.set_confirm_label("Trust plugin".into());
    dialog.set_cancel_label("Cancel".into());
    dialog.set_open(true);
}

/// Dismisses the dialog and snaps any cancelled Trust switch back to the stored state.
pub(crate) fn close_security_dialog(ctx: &BridgeContext, ui: &CloudStream) {
    ctx.session.borrow_mut().pending_trust = None;
    clear_dialog_props(ui);
    refresh_plugins(ctx, ui);
}

fn confirm_security_dialog(ctx: &BridgeContext) {
    let action = ctx.session.borrow_mut().pending_trust.take();
    if let Some(ui) = ctx.window() {
        clear_dialog_props(&ui);
    }
    match action {
        Some(PendingTrustAction::InstallTrusted { hit }) => install_plugin_trusted(ctx, *hit),
        Some(PendingTrustAction::TrustInstalled { file_name }) => {
            apply_set_trusted(ctx, file_name, true);
        }
        None => {}
    }
}

fn clear_dialog_props(ui: &CloudStream) {
    let dialog = ui.global::<SecurityDialog>();
    dialog.set_open(false);
    dialog.set_show_details(false);
    dialog.set_can_trust(false);
    dialog.set_technical_details("".into());
}
