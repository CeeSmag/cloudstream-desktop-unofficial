//! Headless software-renderer smoke & visual-contract tests for every redesigned Slint screen.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{
    Image, ModelRc, PhysicalSize, PlatformError, Rgb8Pixel, SharedPixelBuffer, SharedString,
    VecModel,
};
use ui_slint::{
    AvailablePlugin, CloudStream, EpisodeItem, PluginItem, PosterItem, RepositoryItem, ShelfSection,
};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 800;

struct HeadlessPlatform {
    window: Rc<MinimalSoftwareWindow>,
}

impl Platform for HeadlessPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }
}

fn render_frame(window: &MinimalSoftwareWindow, name: &str) -> Vec<Rgb8Pixel> {
    slint::platform::update_timers_and_animations();
    window.request_redraw();
    let mut buffer = vec![Rgb8Pixel::new(0, 0, 0); (WIDTH * HEIGHT) as usize];
    window.draw_if_needed(|renderer| {
        renderer.render(&mut buffer, WIDTH as usize);
    });
    if let Ok(dir) = std::env::var("CS_SCREENSHOT_DIR") {
        write_ppm(Path::new(&dir).join(format!("{name}.ppm")), &buffer);
    }
    buffer
}

fn write_ppm(path: std::path::PathBuf, pixels: &[Rgb8Pixel]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut bytes = format!("P6\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
    bytes.reserve(pixels.len() * 3);
    for px in pixels {
        bytes.push(px.r);
        bytes.push(px.g);
        bytes.push(px.b);
    }
    let _ = std::fs::write(path, bytes);
}

fn assert_non_blank(buffer: &[Rgb8Pixel], screen: &str) {
    let first = buffer.first().copied().unwrap_or(Rgb8Pixel::new(0, 0, 0));
    let distinct = buffer.iter().filter(|px| **px != first).count();
    assert!(
        distinct > 2_000,
        "screen '{screen}' rendered nearly blank ({distinct} non-background pixels)"
    );
}

fn sample_artwork(r: u8, g: u8, b: u8) -> Image {
    let mut buf = SharedPixelBuffer::<Rgb8Pixel>::new(64, 96);
    for (idx, px) in buf.make_mut_slice().iter_mut().enumerate() {
        let y = (idx / 64) as u8;
        px.r = r.saturating_add(y / 3);
        px.g = g.saturating_add(y / 4);
        px.b = b.saturating_add(y / 2);
    }
    Image::from_rgb8(buf)
}

fn strings(items: &[&str]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        items
            .iter()
            .map(|s| SharedString::from(*s))
            .collect::<Vec<_>>(),
    ))
}

fn sample_posters() -> ModelRc<PosterItem> {
    let items = [
        ("Dune: Part Two", "2024 · Movie", 0.62, (48, 72, 130)),
        ("Severance", "S2 E4 · 2025", 0.35, (28, 92, 108)),
        ("Andor", "2022 · Series", 0.0, (112, 54, 46)),
        ("Frieren", "2023 · Anime", 0.85, (64, 58, 124)),
        ("The Bear", "2024 · Series", 0.0, (36, 78, 96)),
    ];
    ModelRc::new(VecModel::from(
        items
            .into_iter()
            .map(|(title, sub, prog, (r, g, b))| PosterItem {
                title: title.into(),
                subtitle: sub.into(),
                provider: "StreamFlix".into(),
                url: "https://example.invalid/item".into(),
                poster: "".into(),
                poster_headers: "".into(),
                media_key: title.into(),
                data: "".into(),
                image: sample_artwork(r, g, b),
                progress: prog,
            })
            .collect::<Vec<_>>(),
    ))
}

fn populate_settings(ui: &CloudStream) {
    ui.set_providers(strings(&["StreamFlix", "CineVault", "AnimePulse"]));
    ui.set_selected_provider("StreamFlix".into());
    ui.set_update_badge("Update v0.5.0".into());
    ui.set_cache_size_labels(strings(&["128 MB", "256 MB", "512 MB", "1024 MB"]));
    ui.set_cache_size_value("256 MB".into());
    ui.set_ahead_labels(strings(&["60%", "75%", "85%"]));
    ui.set_ahead_value("75%".into());
    ui.set_storage_labels(strings(&["1 GB", "2 GB", "5 GB", "10 GB"]));
    ui.set_storage_value("2 GB".into());
    ui.set_storage_fraction(0.64);
    ui.set_storage_label("1.28 GB of 2 GB used".into());
    ui.set_storage_rows(strings(&[
        "Artwork cache · 740 MB",
        "Saved sources · 412 MB",
        "Subtitles · 128 MB",
    ]));
    ui.set_worker_labels(strings(&["1", "2", "Unlimited"]));
    ui.set_worker_value("2".into());
    ui.set_update_label("Up to date · v0.4.8 installed".into());
    ui.set_audio_labels(strings(&["Stream default", "English", "Japanese"]));
    ui.set_audio_value("English".into());
    ui.set_subtitle_labels(strings(&["Spanish", "French", "German"]));
    ui.set_subtitle_chosen(strings(&["EN  English", "AR  Arabic"]));
}

fn populate_extensions(ui: &CloudStream) {
    ui.set_repositories(ModelRc::new(VecModel::from(vec![RepositoryItem {
        name: "Official Main Repository".into(),
        url: "https://example.invalid/repo.json".into(),
    }])));
    ui.set_available_plugins(ModelRc::new(VecModel::from(vec![
        AvailablePlugin {
            key: "k1".into(),
            name: "StreamFlix".into(),
            detail: "v14 · EN · Official Main Repository".into(),
            description: "High-bitrate movies and series with multi-language subtitles.".into(),
            installed: true,
        },
        AvailablePlugin {
            key: "k2".into(),
            name: "AnimePulse".into(),
            detail: "v9 · EN/JA · Official Main Repository".into(),
            description: "Subbed and dubbed anime catalog with AniSkip metadata.".into(),
            installed: false,
        },
    ])));
    ui.set_plugins(ModelRc::new(VecModel::from(vec![
        PluginItem {
            name: "StreamFlix".into(),
            detail: "v14 · Official Main Repository".into(),
            file_name: "streamflix.cs3".into(),
            trusted: false,
        },
        PluginItem {
            name: "CineVault".into(),
            detail: "v7 · Local".into(),
            file_name: "cinevault.cs3".into(),
            trusted: true,
        },
    ])));
}

fn populate_home_and_details(ui: &CloudStream) {
    let posters = sample_posters();
    ui.set_continue_items(posters.clone());
    ui.set_home_shelves(ModelRc::new(VecModel::from(vec![
        ShelfSection {
            title: "Trending Now".into(),
            items: posters.clone(),
        },
        ShelfSection {
            title: "Top Rated Cinema".into(),
            items: posters.clone(),
        },
    ])));
    ui.set_browse_items(posters.clone());
    ui.set_search_items(posters);
    ui.set_hero_title("Dune: Part Two".into());
    ui.set_hero_subtitle("2024 · Resume at 1:18:42 · 4K HDR".into());
    ui.set_hero_image(sample_artwork(44, 62, 118));
    ui.set_hero_row(0);
    ui.set_continue_row(1);
    ui.set_shelf_row_base(2);
    ui.set_grid_columns(6);
    ui.set_wide_window(true);
}

fn populate_details(ui: &CloudStream) {
    ui.set_details_title("Severance".into());
    ui.set_details_meta("2022–2025 · Drama, Sci-Fi · 8.7 ★ · 2 Seasons".into());
    ui.set_details_plot(
        "Mark leads a team of office workers whose memories have been surgically divided between their work and personal lives.".into(),
    );
    ui.set_details_cast("Adam Scott · Britt Lower · Zach Cherry · Tramell Tillman".into());
    ui.set_details_tags(strings(&["Sci-Fi", "Thriller", "Drama", "Mystery"]));
    ui.set_details_poster(sample_artwork(28, 92, 108));
    ui.set_resume_label("Resume S2 E4".into());
    ui.set_season_labels(strings(&["All seasons", "Season 1", "Season 2"]));
    ui.set_episodes(ModelRc::new(VecModel::from(vec![
        EpisodeItem {
            name: "Hello, Ms. Cobel".into(),
            code: "S2 E1".into(),
            data: "ep1".into(),
            watched: true,
            progress: 1.0,
            description: "Mark returns to work under different circumstances.".into(),
            season: 2,
            index: 0,
        },
        EpisodeItem {
            name: "Woe's Hollow".into(),
            code: "S2 E4".into(),
            data: "ep4".into(),
            watched: false,
            progress: 0.42,
            description: "The team ventures outside the severed floor.".into(),
            season: 2,
            index: 3,
        },
    ])));
    ui.set_details_resume_key("ep4".into());
}

fn verify_settings_and_browse(ui: &CloudStream, window: &MinimalSoftwareWindow) {
    ui.set_focus_visible(false);
    ui.set_tab(4);
    ui.set_settings_page_index(0);
    assert_non_blank(
        &render_frame(window, "03_settings_general"),
        "settings_general",
    );
    ui.set_settings_page_index(1);
    assert_non_blank(
        &render_frame(window, "04_settings_preferences"),
        "settings_preferences",
    );
    ui.set_settings_page_index(2);
    assert_non_blank(
        &render_frame(window, "05_settings_extensions"),
        "settings_extensions",
    );
    ui.set_tab(1);
    assert_non_blank(&render_frame(window, "06_browse"), "browse");
    ui.set_tab(2);
    ui.set_search_query("Dune".into());
    assert_non_blank(&render_frame(window, "07_search"), "search");
}

fn verify_overlays(ui: &CloudStream, window: &MinimalSoftwareWindow) {
    populate_details(ui);
    assert_non_blank(&render_frame(window, "08_details"), "details");
    ui.set_details_title("".into());
    ui.set_player_open(true);
    ui.set_player_title("Severance · S2 E4 — Woe's Hollow".into());
    ui.set_duration_ms(3_120_000.0);
    ui.set_position_ms(1_310_000.0);
    ui.set_position_text("21:50".into());
    ui.set_duration_text("52:00".into());
    ui.set_buffered_fraction(0.62);
    ui.set_has_next_episode(true);
    ui.set_skip_label("Skip Intro".into());
    assert_non_blank(&render_frame(window, "09_player"), "player");
    ui.set_player_open(false);
    ui.set_tab(4);
    ui.set_settings_page_index(0);
    ui.set_show_wipe_confirm(true);
    assert_non_blank(&render_frame(window, "10_wipe_dialog"), "wipe_dialog");
}

#[test]
fn redesigned_tv_screens_render_without_panics_or_blank_frames() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    window.set_size(PhysicalSize::new(WIDTH, HEIGHT));
    slint::platform::set_platform(Box::new(HeadlessPlatform {
        window: window.clone(),
    }))
    .expect("set headless slint platform");

    let ui = CloudStream::new().expect("create CloudStream window");
    let worker_seen = Rc::new(RefCell::new(String::new()));
    let worker_ref = worker_seen.clone();
    ui.on_set_worker_limit(move |val| *worker_ref.borrow_mut() = val.to_string());

    assert_non_blank(&render_frame(&window, "01_startup"), "startup");
    ui.set_starting(false);
    populate_settings(&ui);
    populate_extensions(&ui);
    populate_home_and_details(&ui);

    ui.set_focus_visible(true);
    ui.set_focus_row(0);
    ui.set_focus_col(0);
    assert_non_blank(&render_frame(&window, "02_home"), "home");
    verify_settings_and_browse(&ui, &window);
    verify_overlays(&ui, &window);

    ui.invoke_set_worker_limit("Unlimited".into());
    assert_eq!(worker_seen.borrow().as_str(), "Unlimited");
}
