//! Headless Slint UI logic, state-binding, and callback interaction tests.
//!
//! Drives `CloudStream` via `MinimalSoftwareWindow` (zero display server required) so these
//! tests run identically on Linux and Windows CI, including `--no-default-features`.
#![allow(clippy::expect_used)]

use std::cell::RefCell;
use std::rc::Rc;

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, Model, ModelRc, PhysicalSize, PlatformError, SharedString, VecModel};
use ui_slint::{
    AvailablePlugin, BrowseFilter, CloudStream, CloudflareBannerPhase, CloudflareBannerState,
    EpisodeItem, PluginItem, PosterItem, RepoDetail, RepoList, RepositoryItem, SecurityDialog,
    ShelfSection, SourceItem, TrackChoice,
};

struct HeadlessPlatform {
    window: Rc<MinimalSoftwareWindow>,
}

impl Platform for HeadlessPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }
}

fn strings(items: &[&str]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        items
            .iter()
            .map(|s| SharedString::from(*s))
            .collect::<Vec<_>>(),
    ))
}

fn verify_provider_and_search(ui: &CloudStream) {
    ui.set_starting(false);
    ui.set_providers(strings(&["CineStream", "AnimeBox"]));
    ui.set_selected_provider("CineStream".into());
    assert_eq!(ui.get_providers().row_count(), 2);
    assert_eq!(ui.get_selected_provider().as_str(), "CineStream");

    let picked_provider = Rc::new(RefCell::new(String::new()));
    let picked_ref = Rc::clone(&picked_provider);
    ui.on_set_provider(move |p| *picked_ref.borrow_mut() = p.to_string());
    ui.invoke_set_provider("AnimeBox".into());
    assert_eq!(picked_provider.borrow().as_str(), "AnimeBox");

    let searched = Rc::new(RefCell::new(String::new()));
    let search_ref = Rc::clone(&searched);
    ui.on_search(move |q| *search_ref.borrow_mut() = q.to_string());
    ui.set_tab(2);
    ui.set_search_query("Severance".into());
    ui.invoke_search("Severance".into());
    assert_eq!(searched.borrow().as_str(), "Severance");
}

fn verify_details_and_episodes(ui: &CloudStream) {
    let bookmark_toggles = Rc::new(RefCell::new(0usize));
    let bm_ref = Rc::clone(&bookmark_toggles);
    ui.on_toggle_bookmark(move || *bm_ref.borrow_mut() += 1);
    ui.invoke_toggle_bookmark();
    assert_eq!(*bookmark_toggles.borrow(), 1);

    ui.set_episodes(ModelRc::new(VecModel::from(vec![
        EpisodeItem {
            name: "Pilot".into(),
            episode_label: "Episode 1".into(),
            data: "ep-1".into(),
            watched: true,
            progress: 1.0,
            description: "First".into(),
            index: 0,
            poster: "".into(),
            image: slint::Image::default(),
        },
        EpisodeItem {
            name: "Second".into(),
            episode_label: "Episode 2".into(),
            data: "ep-2".into(),
            watched: false,
            progress: 0.4,
            description: "Second".into(),
            index: 1,
            poster: "".into(),
            image: slint::Image::default(),
        },
    ])));
    assert_eq!(ui.get_episodes().row_count(), 2);

    let played_ep = Rc::new(RefCell::new(-1i32));
    let ep_ref = Rc::clone(&played_ep);
    ui.on_play_episode(move |idx| *ep_ref.borrow_mut() = idx);
    ui.invoke_play_episode(1);
    assert_eq!(*played_ep.borrow(), 1);
}

fn verify_player_controls(ui: &CloudStream) {
    ui.set_player_open(true);
    ui.set_skip_label("Skip Intro".into());
    let skip_clicks = Rc::new(RefCell::new(0usize));
    let skip_ref = Rc::clone(&skip_clicks);
    ui.on_skip_segment(move || *skip_ref.borrow_mut() += 1);
    ui.invoke_skip_segment();
    assert_eq!(*skip_clicks.borrow(), 1);

    ui.set_sources(ModelRc::new(VecModel::from(vec![SourceItem {
        index: 0,
        label: "Primary 1080p".into(),
        kind: "HLS".into(),
        size: "1.2 GB".into(),
        flags: strings(&["EN"]),
        checking: false,
        working: true,
        selected: true,
    }])));
    let chosen_source = Rc::new(RefCell::new(-1i32));
    let src_ref = Rc::clone(&chosen_source);
    ui.on_select_source(move |idx| *src_ref.borrow_mut() = idx);
    ui.invoke_select_source(0);
    assert_eq!(*chosen_source.borrow(), 0);
}

fn verify_tracks_and_cloudflare(ui: &CloudStream) {
    ui.set_subtitle_tracks(ModelRc::new(VecModel::from(vec![
        TrackChoice {
            label: "Off".into(),
            id: -1,
            flag: "".into(),
            selected: false,
        },
        TrackChoice {
            label: "English".into(),
            id: 2,
            flag: "EN".into(),
            selected: true,
        },
    ])));
    let picked_sub = Rc::new(RefCell::new(-99i32));
    let sub_ref = Rc::clone(&picked_sub);
    ui.on_select_subtitle_track(move |id| *sub_ref.borrow_mut() = id);
    ui.invoke_select_subtitle_track(2);
    assert_eq!(*picked_sub.borrow(), 2);

    let banner = ui.global::<CloudflareBannerState>();
    banner.set_host("challenge.example".into());
    banner.set_phase(CloudflareBannerPhase::NeedsPermission);
    banner.set_waiting_count(2);
    banner.set_can_cycle(true);
    let cf_actions = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let allow_ref = Rc::clone(&cf_actions);
    banner.on_verify(move || allow_ref.borrow_mut().push("verify"));
    let decline_ref = Rc::clone(&cf_actions);
    banner.on_dismiss(move || decline_ref.borrow_mut().push("dismiss"));
    let cycle_ref = Rc::clone(&cf_actions);
    banner.on_next_waiting(move || cycle_ref.borrow_mut().push("next"));
    banner.invoke_verify();
    banner.invoke_next_waiting();
    banner.invoke_dismiss();
    assert_eq!(*cf_actions.borrow(), vec!["verify", "next", "dismiss"]);
    banner.set_phase(CloudflareBannerPhase::Hidden);
}

fn verify_extensions_and_shelves(ui: &CloudStream) {
    ui.set_repositories(ModelRc::new(VecModel::from(vec![RepositoryItem {
        name: "Main".into(),
        url: "https://repo.example/repo.json".into(),
        detail: "repo.example".into(),
        plugin_count: "2 plugins".into(),
    }])));
    ui.set_plugins(ModelRc::new(VecModel::from(vec![PluginItem {
        name: "CineStream".into(),
        detail: "v1".into(),
        file_name: "cinestream.cs3".into(),
        trusted: false,
        supported: true,
        note: "".into(),
    }])));
    ui.set_available_plugins(ModelRc::new(VecModel::from(vec![AvailablePlugin {
        key: "k1".into(),
        name: "CineStream".into(),
        detail: "v1".into(),
        description: "Movies".into(),
        installed: true,
    }])));
    ui.global::<RepoList>().set_total_count(1);
    ui.global::<RepoDetail>()
        .set_open_url("https://repo.example/repo.json".into());
    ui.global::<BrowseFilter>().set_current_page(3);
    assert_eq!(ui.global::<RepoList>().get_total_count(), 1);
    assert_eq!(
        ui.global::<RepoDetail>().get_open_url().as_str(),
        "https://repo.example/repo.json"
    );
    assert_eq!(ui.global::<BrowseFilter>().get_current_page(), 3);
}

fn verify_home_shelves(ui: &CloudStream) {
    ui.set_home_shelves(ModelRc::new(VecModel::from(vec![ShelfSection {
        title: "Trending".into(),
        items: ModelRc::new(VecModel::from(vec![PosterItem {
            title: "Dune".into(),
            subtitle: "2024".into(),
            provider: "CineStream".into(),
            url: "https://example.invalid/dune".into(),
            poster: "".into(),
            poster_headers: "".into(),
            media_key: "dune".into(),
            image: slint::Image::default(),
            progress: 0.5,
        }])),
    }])));
    assert_eq!(ui.get_home_shelves().row_count(), 1);
}

fn verify_updates_flow(ui: &CloudStream) {
    ui.set_update_badge("Update v0.5.0".into());
    ui.set_update_label("Version 0.5.0 is ready to install (you have 0.4.8).".into());
    ui.set_update_version("v0.5.0 · 142.0 MB".into());
    ui.set_update_notes("- Faster playback\n- Bug fixes".into());
    ui.set_update_can_apply(true);
    ui.set_update_busy(false);
    ui.set_update_downloading(false);
    ui.set_update_progress(0.0);

    assert_eq!(ui.get_update_badge().as_str(), "Update v0.5.0");
    assert_eq!(ui.get_update_version().as_str(), "v0.5.0 · 142.0 MB");
    assert!(ui.get_update_notes().contains("Faster playback"));
    assert!(ui.get_update_can_apply());

    let applied = Rc::new(RefCell::new(0usize));
    let applied_ref = Rc::clone(&applied);
    ui.on_apply_update(move || *applied_ref.borrow_mut() += 1);
    ui.invoke_apply_update();
    assert_eq!(*applied.borrow(), 1);

    let checked = Rc::new(RefCell::new(0usize));
    let checked_ref = Rc::clone(&checked);
    ui.on_check_updates(move || *checked_ref.borrow_mut() += 1);
    ui.invoke_check_updates();
    assert_eq!(*checked.borrow(), 1);

    ui.set_tab(4);
    ui.set_settings_page_index(0);
    ui.set_pending_updates_scroll(true);
    ui.set_updates_focus_request(1);
    assert_eq!(ui.get_tab(), 4);
    assert_eq!(ui.get_settings_page_index(), 0);
}

/// The Performance card's rows for the verification helper and the video output.
fn verify_helper_and_video_rows(ui: &CloudStream) {
    ui.set_helper_idle_labels(strings(&["1 min", "2 min", "5 min", "15 min", "Never"]));
    ui.set_helper_idle_value("5 min".into());
    assert_eq!(ui.get_helper_idle_labels().row_count(), 5);
    let idle = Rc::new(RefCell::new(String::new()));
    let idle_ref = Rc::clone(&idle);
    ui.on_set_helper_idle(move |label| *idle_ref.borrow_mut() = label.to_string());
    ui.invoke_set_helper_idle("Never".into());
    assert_eq!(idle.borrow().as_str(), "Never");

    ui.set_helper_windows_labels(strings(&[
        "1 window",
        "2 windows",
        "4 windows",
        "8 windows",
    ]));
    ui.set_helper_windows_value("2 windows".into());
    assert_eq!(ui.get_helper_windows_labels().row_count(), 4);
    let windows = Rc::new(RefCell::new(String::new()));
    let windows_ref = Rc::clone(&windows);
    ui.on_set_helper_windows(move |label| *windows_ref.borrow_mut() = label.to_string());
    ui.invoke_set_helper_windows("8 windows".into());
    assert_eq!(windows.borrow().as_str(), "8 windows");

    ui.set_video_renderer_labels(strings(&[
        "Software (works everywhere)",
        "GPU (no picture copies)",
    ]));
    ui.set_video_renderer_value("Software (works everywhere)".into());
    assert_eq!(ui.get_video_renderer_labels().row_count(), 2);
    let output = Rc::new(RefCell::new(String::new()));
    let output_ref = Rc::clone(&output);
    ui.on_set_video_renderer(move |label| *output_ref.borrow_mut() = label.to_string());
    ui.invoke_set_video_renderer("GPU (no picture copies)".into());
    assert_eq!(output.borrow().as_str(), "GPU (no picture copies)");
}

fn verify_performance_card(ui: &CloudStream) {
    ui.set_perf_profile_labels(strings(&[
        "Auto (recommended)",
        "Low-end",
        "Balanced",
        "High-end",
    ]));
    ui.set_perf_profile_value("Auto (recommended)".into());
    ui.set_perf_pc_text("This PC: 8 GB RAM, 4 cores".into());
    ui.set_perf_state_text("Following the Balanced preset for this PC".into());
    ui.set_perf_warning("".into());
    ui.set_perf_can_reset(false);
    assert_eq!(ui.get_perf_profile_labels().row_count(), 4);
    assert_eq!(ui.get_perf_profile_value().as_str(), "Auto (recommended)");
    assert!(!ui.get_perf_can_reset());

    let chosen = Rc::new(RefCell::new(String::new()));
    let chosen_ref = Rc::clone(&chosen);
    ui.on_set_performance_profile(move |label| *chosen_ref.borrow_mut() = label.to_string());
    ui.invoke_set_performance_profile("Low-end".into());
    assert_eq!(chosen.borrow().as_str(), "Low-end");

    let resets = Rc::new(RefCell::new(0usize));
    let resets_ref = Rc::clone(&resets);
    ui.on_reset_performance(move || *resets_ref.borrow_mut() += 1);
    ui.invoke_reset_performance();
    assert_eq!(*resets.borrow(), 1);
}

fn verify_security_dialog(ui: &CloudStream) {
    let dialog = ui.global::<SecurityDialog>();
    assert!(!dialog.get_open());
    dialog.set_plugin_name("StreamCenter".into());
    dialog.set_headline("Blocked for security".into());
    dialog.set_summary("This plugin uses restricted system features.".into());
    dialog.set_reasons(strings(&[
        "Dynamic class loading — can load unverified code.",
        "Local network & raw sockets — can scan your local network.",
    ]));
    dialog.set_warning_text("Only trust plugins from authors you know and trust.".into());
    dialog.set_technical_details("Disallowed class 'java.lang.ClassLoader'".into());
    dialog.set_show_details(false);
    dialog.set_can_trust(true);
    dialog.set_confirm_label("Trust & Install anyway".into());
    dialog.set_cancel_label("Keep blocked".into());
    dialog.set_open(true);
    assert!(dialog.get_open());
    assert_eq!(dialog.get_reasons().row_count(), 2);

    dialog.set_show_details(true);
    assert!(dialog.get_show_details());

    let confirmed = Rc::new(RefCell::new(0usize));
    let confirm_ref = Rc::clone(&confirmed);
    dialog.on_confirm(move || *confirm_ref.borrow_mut() += 1);
    dialog.invoke_confirm();
    assert_eq!(*confirmed.borrow(), 1);

    let cancelled = Rc::new(RefCell::new(0usize));
    let cancel_ref = Rc::clone(&cancelled);
    dialog.on_cancel(move || *cancel_ref.borrow_mut() += 1);
    dialog.invoke_cancel();
    assert_eq!(*cancelled.borrow(), 1);
    dialog.set_open(false);
}

#[test]
fn headless_ui_bindings_callbacks_and_navigation_flows() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    window.set_size(PhysicalSize::new(1280, 800));
    slint::platform::set_platform(Box::new(HeadlessPlatform { window }))
        .expect("install headless platform");

    let ui = CloudStream::new().expect("instantiate CloudStream");
    verify_provider_and_search(&ui);
    verify_details_and_episodes(&ui);
    verify_player_controls(&ui);
    verify_tracks_and_cloudflare(&ui);
    verify_extensions_and_shelves(&ui);
    verify_home_shelves(&ui);
    verify_updates_flow(&ui);
    verify_performance_card(&ui);
    verify_helper_and_video_rows(&ui);
    verify_security_dialog(&ui);
}
