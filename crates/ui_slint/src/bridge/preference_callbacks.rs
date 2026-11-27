//! Settings > Preferences: playback switches and languages.

use app_services::UiServices;
use player_api::{all_languages, Language};
use slint::SharedString;

use crate::source_menu::parse_preferred_resolution_label;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::subtitle_languages::show_subtitle_languages;

/// Sends the saved audio and subtitle preferences (honoring the auto-select toggle) to the player.
pub(crate) fn apply_player_language_preferences(services: &UiServices) {
    let Some(playback) = &services.playback else {
        return;
    };
    match services.settings.preferred_audio() {
        Ok(audio) => playback.set_preferred_audio(audio),
        Err(error) => tracing::warn!(%error, "could not read preferred audio language"),
    }
    let auto = services.settings.auto_select_subtitles().unwrap_or(true);
    let subtitles = if auto {
        services
            .settings
            .preferred_subtitles()
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "could not read preferred subtitle languages");
                Vec::new()
            })
    } else {
        Vec::new()
    };
    playback.set_preferred_subtitles(&subtitles);
}

pub(crate) fn wire_preferences(ctx: &BridgeContext, ui: &CloudStream) {
    wire_switches(ctx, ui);
    let c = ctx.clone();
    ui.on_set_preferred_resolution(move |label| set_preferred_resolution(&c, label));
    let c = ctx.clone();
    ui.on_set_audio(move |label| set_audio(&c, label));
    let c = ctx.clone();
    ui.on_set_subtitle_add(move |label| add_subtitle_language(&c, label));
    let c = ctx.clone();
    ui.on_remove_subtitle(move |at| {
        let mut chosen = c
            .services
            .settings
            .preferred_subtitles()
            .unwrap_or_default();
        if let Some(at) = usize::try_from(at).ok().filter(|at| *at < chosen.len()) {
            chosen.remove(at);
            set_subtitle_languages(&c, &chosen);
        }
    });
}

fn wire_switches(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_set_autoplay(move |on| {
        saved("autoplay", c.services.settings.set_autoplay_next(on));
        c.window().inspect(|ui| ui.set_autoplay_next(on));
    });
    let c = ctx.clone();
    ui.on_set_skip_lookup(move |on| {
        saved("skip lookup", c.services.settings.set_skip_lookup(on));
        c.window().inspect(|ui| ui.set_skip_lookup(on));
    });
    let c = ctx.clone();
    ui.on_set_stream_proxy(move |on| {
        saved("stream proxy", c.services.settings.set_stream_proxy(on));
        c.window().inspect(|ui| ui.set_stream_proxy(on));
    });
    let c = ctx.clone();
    ui.on_set_auto_subtitles(move |on| {
        saved(
            "auto subtitles",
            c.services.settings.set_auto_select_subtitles(on),
        );
        apply_player_language_preferences(&c.services);
        c.window().inspect(|ui| ui.set_auto_subtitles(on));
    });
}

fn set_preferred_resolution(ctx: &BridgeContext, label: SharedString) {
    let Some(resolution) = parse_preferred_resolution_label(&label) else {
        tracing::warn!(%label, "unknown preferred resolution choice ignored");
        return;
    };
    tracing::debug!(?resolution, "preferred resolution changed");
    saved(
        "preferred resolution",
        ctx.services.settings.set_preferred_resolution(resolution),
    );
    if let Some(ui) = ctx.window() {
        ui.set_preferred_resolution_value(label);
    }
}

fn set_audio(ctx: &BridgeContext, label: SharedString) {
    let language = find_language(&label);
    saved(
        "audio language",
        ctx.services.settings.set_preferred_audio(language),
    );
    if let Some(playback) = &ctx.services.playback {
        playback.set_preferred_audio(language);
    }
    if let Some(ui) = ctx.window() {
        ui.set_audio_value(label);
    }
}

fn add_subtitle_language(ctx: &BridgeContext, label: SharedString) {
    let Some(language) = find_language(&label) else {
        return;
    };
    let mut chosen = ctx
        .services
        .settings
        .preferred_subtitles()
        .unwrap_or_default();
    let known = chosen.iter().any(|c| c.code() == language.code());
    if known || chosen.len() >= app_services::MAX_PREFERRED_SUBTITLES {
        return;
    }
    chosen.push(language);
    set_subtitle_languages(ctx, &chosen);
}

fn set_subtitle_languages(ctx: &BridgeContext, languages: &[Language]) {
    saved(
        "subtitle languages",
        ctx.services.settings.set_preferred_subtitles(languages),
    );
    apply_player_language_preferences(&ctx.services);
    if let Some(ui) = ctx.window() {
        show_subtitle_languages(&ui, languages);
    }
}

fn find_language(label: &str) -> Option<Language> {
    all_languages()
        .iter()
        .find(|language| language.name == label)
        .copied()
}

/// Logs a setting that could not be saved; the switch still shows the viewer's choice for this run.
pub(crate) fn saved<E: std::fmt::Display>(setting: &str, result: Result<(), E>) {
    if let Err(error) = result {
        tracing::warn!(setting, %error, "a setting could not be saved");
    }
}
