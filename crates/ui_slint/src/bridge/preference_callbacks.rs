//! Settings > Preferences: playback switches and languages.

use player_api::{all_languages, Language};
use slint::SharedString;

use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::initial_settings::language_chips;

pub(crate) fn wire_preferences(ctx: &BridgeContext, ui: &CloudStream) {
    wire_switches(ctx, ui);
    let c = ctx.clone();
    ui.on_set_audio(move |label| set_audio(&c, label));
    let c = ctx.clone();
    ui.on_set_subtitle_add(move |label| add_subtitle_language(&c, label));
    let c = ctx.clone();
    ui.on_clear_subtitles(move || set_subtitle_languages(&c, &[]));
}

fn wire_switches(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_set_keep_ready(move |on| {
        saved(
            "keep video ready",
            c.services.settings.set_keep_video_ready(on),
        );
        c.window().inspect(|ui| ui.set_keep_video_ready(on));
    });
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
        c.window().inspect(|ui| ui.set_auto_subtitles(on));
    });
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
    if !chosen.iter().any(|known| known.code() == language.code()) {
        chosen.push(language);
        chosen.truncate(app_services::MAX_PREFERRED_SUBTITLES);
    }
    set_subtitle_languages(ctx, &chosen);
    if let Some(ui) = ctx.window() {
        ui.set_subtitle_add_value(label);
    }
}

fn set_subtitle_languages(ctx: &BridgeContext, languages: &[Language]) {
    saved(
        "subtitle languages",
        ctx.services.settings.set_preferred_subtitles(languages),
    );
    if let Some(playback) = &ctx.services.playback {
        playback.set_preferred_subtitles(languages);
    }
    if let Some(ui) = ctx.window() {
        ui.set_subtitle_chips(language_chips(languages).into());
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
