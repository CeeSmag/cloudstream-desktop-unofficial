//! Headless software-renderer smoke & visual-contract tests for every redesigned Slint screen.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter, WindowEvent};
use slint::{
    ComponentHandle, Image, LogicalPosition, Model, ModelRc, PhysicalSize, PlatformError,
    Rgb8Pixel, SharedPixelBuffer, SharedString, VecModel,
};
use ui_slint::{
    AvailablePlugin, BrowseFilter, CastItem, CloudStream, DetailsMeta, EpisodeItem, FilterChip,
    HomeExpanded, PaginationItem, PluginItem, PluginList, PosterItem, RepoDetail, RepoList,
    RepositoryItem, SeekMark, SeekPreview, ShelfSection, SourceItem, SourceMenuInfo, TrackChoice,
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

fn sample_repositories() -> ModelRc<RepositoryItem> {
    ModelRc::new(VecModel::from(vec![
        RepositoryItem {
            name: "Official Main Repository".into(),
            url: "https://raw.githubusercontent.com/cloudstream/main/repo.json".into(),
            detail: "raw.githubusercontent.com · Added 2026-11-19".into(),
            plugin_count: "42 plugins".into(),
        },
        RepositoryItem {
            name: "Anime Community Repo".into(),
            url: "https://anime-repo.example/repo.json".into(),
            detail: "anime-repo.example · Added 2026-11-12".into(),
            plugin_count: "18 plugins".into(),
        },
        RepositoryItem {
            name: "Cinema Archive".into(),
            url: "https://cinema.example/repo.json".into(),
            detail: "cinema.example · Added 2026-10-03".into(),
            plugin_count: "9 plugins".into(),
        },
    ]))
}

fn populate_extensions(ui: &CloudStream) {
    let repos = ui.global::<RepoList>();
    repos.set_total_count(3);
    repos.set_count_text("3 repositories".into());
    ui.set_repositories(sample_repositories());
    let plugins_list = ui.global::<PluginList>();
    plugins_list.set_count_text("2 of 69".into());
    ui.set_available_plugins(ModelRc::new(VecModel::from(vec![
        AvailablePlugin {
            key: "k1".into(),
            name: "StreamFlix".into(),
            detail: "v14 · Official Main Repository · en · 148.2 KB".into(),
            description: "High-bitrate movies and series with multi-language subtitles.".into(),
            installed: true,
        },
        AvailablePlugin {
            key: "k2".into(),
            name: "AnimePulse".into(),
            detail: "v9 · Official Main Repository · ja · 94.5 KB".into(),
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

fn populate_repo_detail(ui: &CloudStream) {
    let detail = ui.global::<RepoDetail>();
    detail.set_open_url("https://raw.githubusercontent.com/cloudstream/main/repo.json".into());
    detail.set_name("Official Main Repository".into());
    detail.set_summary("42 plugins · 1 installed".into());
    detail.set_count_text("2 of 42".into());
    detail.set_languages(strings(&["All languages", "en", "ja"]));
    detail.set_plugins(ModelRc::new(VecModel::from(vec![
        AvailablePlugin {
            key: "k1".into(),
            name: "StreamFlix".into(),
            detail: "v14 · en · 148.2 KB".into(),
            description: "High-bitrate movies and series with multi-language subtitles.".into(),
            installed: true,
        },
        AvailablePlugin {
            key: "k2".into(),
            name: "AnimePulse".into(),
            detail: "v9 · ja · 94.5 KB".into(),
            description: "Subbed and dubbed anime catalog with AniSkip metadata.".into(),
            installed: false,
        },
    ])));
}

fn sample_browse_pages() -> ModelRc<PaginationItem> {
    let slot = |label: &str, page: i32, active: bool, ellipsis: bool| PaginationItem {
        label: label.into(),
        page,
        active,
        ellipsis,
    };
    ModelRc::new(VecModel::from(vec![
        slot("1", 1, false, false),
        slot("2", 2, true, false),
        slot("3", 3, false, false),
        slot("4", 4, false, false),
        slot("5", 5, false, false),
        slot("…", 0, false, true),
        slot("167", 167, false, false),
    ]))
}

fn populate_home_and_details(ui: &CloudStream) {
    let posters = sample_posters();
    ui.set_continue_items(posters.clone());
    ui.set_home_shelves(ModelRc::new(VecModel::from(vec![
        ShelfSection {
            title: "Popular Movies".into(),
            items: posters.clone(),
        },
        ShelfSection {
            title: "Popular TV Shows".into(),
            items: posters.clone(),
        },
    ])));
    ui.set_browse_items(posters.clone());
    let browse = ui.global::<BrowseFilter>();
    browse.set_current_page(2);
    browse.set_total_pages(167);
    browse.set_pages(sample_browse_pages());
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

fn verify_home_views(ui: &CloudStream, window: &MinimalSoftwareWindow) {
    ui.set_focus_visible(true);
    ui.set_focus_row(0);
    ui.set_focus_col(0);
    assert_non_blank(&render_frame(window, "02_home"), "home");

    ui.set_hero_title("".into());
    ui.set_continue_items(ModelRc::new(VecModel::from(Vec::<PosterItem>::new())));
    ui.set_focus_visible(false);
    assert_non_blank(&render_frame(window, "02a_home_shelves"), "home_shelves");

    let mut expanded_items: Vec<PosterItem> = sample_posters().iter().collect();
    expanded_items.extend(sample_posters().iter());
    let expanded = ui.global::<HomeExpanded>();
    expanded.set_shelf_index(0);
    expanded.set_count_text("Page 1 · 10 titles".into());
    expanded.set_items(ModelRc::new(VecModel::from(expanded_items)));
    expanded.set_page(1);
    expanded.set_has_prev(false);
    expanded.set_has_next(true);
    assert_non_blank(&render_frame(window, "02b_home_expanded"), "home_expanded");
    expanded.set_shelf_index(-1);
}

fn sample_episode_thumb(r: u8, g: u8, b: u8) -> Image {
    let mut buf = SharedPixelBuffer::<Rgb8Pixel>::new(128, 72);
    for (idx, px) in buf.make_mut_slice().iter_mut().enumerate() {
        let x = (idx % 128) as u8;
        let y = (idx / 128) as u8;
        px.r = r.saturating_add(x / 5);
        px.g = g.saturating_add(y / 2);
        px.b = b.saturating_add(x / 4);
    }
    Image::from_rgb8(buf)
}

fn sample_details_episodes() -> ModelRc<EpisodeItem> {
    let ep =
        |idx: i32, name: &str, desc: &str, (watched, progress): (bool, f32), rgb: (u8, u8, u8)| {
            EpisodeItem {
                name: name.into(),
                code: format!("S1E{}", idx + 1).into(),
                episode_label: format!("Episode {}", idx + 1).into(),
                data: format!("ep{}", idx + 1).into(),
                watched,
                progress,
                description: desc.into(),
                season: 1,
                index: idx,
                poster: "".into(),
                image: sample_episode_thumb(rgb.0, rgb.1, rgb.2),
            }
        };
    ModelRc::new(VecModel::from(vec![
        ep(0, "A Shadow of the Past", "Galadriel is disturbed by signs of an ancient evil's return; Arondir makes an unsettling discovery.", (true, 1.0), (44, 62, 92)),
        ep(1, "Adrift", "Galadriel finds a new ally; Elrond faces a cold reception from an old friend; Nori helps a Stranger.", (false, 0.42), (34, 72, 96)),
        ep(2, "Adar", "Arondir finds himself a captive; Galadriel and Halbrand explore a legendary kingdom.", (false, 0.0), (64, 52, 76)),
        ep(3, "The Great Wave", "Queen Regent Miriel's faith is tested; Isildur finds himself at a crossroads; Elrond uncovers a secret.", (false, 0.0), (38, 66, 86)),
        ep(4, "Partings", "Nori questions her instincts; Elrond struggles to stay true to his oath; Halbrand weighs his destiny.", (false, 0.0), (78, 56, 48)),
        ep(5, "Udun", "Adar and his army march on Ostirith as the villagers make their final stand.", (false, 0.0), (52, 46, 84)),
    ]))
}

fn sample_details_cast() -> ModelRc<CastItem> {
    let member = |name: &str, role: &str, initial: &str, image: Image| CastItem {
        name: name.into(),
        role: role.into(),
        initial: initial.into(),
        photo_url: "".into(),
        image,
    };
    ModelRc::new(VecModel::from(vec![
        member(
            "Morfydd Clark",
            "Galadriel",
            "M",
            sample_artwork(56, 78, 120),
        ),
        member("Robert Aramayo", "Elrond", "R", Image::default()),
        member(
            "Charlie Vickers",
            "Halbrand",
            "C",
            sample_artwork(72, 58, 96),
        ),
    ]))
}

fn populate_details(ui: &CloudStream) {
    ui.set_details_title("The Lord of the Rings: The Rings of Power".into());
    ui.set_details_meta_info(DetailsMeta {
        media_type: "TVSERIES".into(),
        provider: "CineStream".into(),
        year: "2022".into(),
        rating: "6.8".into(),
        duration: "".into(),
    });
    ui.set_details_plot(
        "Beginning in a time of relative peace, we follow an ensemble cast of characters as they confront the re-emergence of evil to Middle-earth. From the darkest depths of the Misty Mountains, to the majestic forests of Lindon, to the breathtaking island kingdom of Numenor, these kingdoms and characters will carve out legacies that live on long after they are gone.".into(),
    );
    ui.set_details_tags(strings(&["Action", "Adventure", "Drama", "Fantasy"]));
    ui.set_details_poster(sample_artwork(48, 74, 118));
    ui.set_details_backdrop(sample_episode_thumb(36, 52, 84));
    ui.set_resume_label("Resume S3 E1 (1h 2m left)".into());
    ui.set_play_label("Play S1 E1".into());
    ui.set_show_secondary_play(true);
    ui.set_details_bookmarked(false);
    ui.set_season_labels(strings(&["Season 1", "Season 2", "Season 3"]));
    ui.set_active_season_label("Season 1".into());
    ui.set_episodes_ascending(true);
    ui.set_episode_columns(3);
    ui.set_episode_total_count(6);
    ui.set_episodes(sample_details_episodes());
    ui.set_details_cast_items(sample_details_cast());
    ui.set_details_resume_key("ep2".into());
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
    ui.set_extensions_tab(0);
    ui.global::<RepoDetail>().set_open_url("".into());
    assert_non_blank(
        &render_frame(window, "05a_settings_extensions_repos"),
        "settings_extensions_repos",
    );
    populate_repo_detail(ui);
    assert_non_blank(
        &render_frame(window, "05b_settings_extensions_repo_detail"),
        "settings_extensions_repo_detail",
    );
    ui.global::<RepoDetail>().set_open_url("".into());
    ui.set_extensions_tab(1);
    assert_non_blank(
        &render_frame(window, "05c_settings_extensions_plugins"),
        "settings_extensions_plugins",
    );
    ui.set_tab(1);
    assert_non_blank(&render_frame(window, "06_browse"), "browse");
    ui.set_tab(2);
    ui.set_search_query("Dune".into());
    assert_non_blank(&render_frame(window, "07_search"), "search");
}

fn sample_sources() -> ModelRc<SourceItem> {
    ModelRc::new(VecModel::from(vec![
        SourceItem {
            index: 0,
            label: "StreamFlix Primary · 1080p WEB-DL".into(),
            kind: "HLS".into(),
            size: "1.4 GB".into(),
            flags: strings(&["EN", "AR"]),
            checking: false,
            working: true,
            selected: true,
        },
        SourceItem {
            index: 1,
            label: "StreamFlix Backup · 2160p HDR".into(),
            kind: "MKV".into(),
            size: "4.2 GB".into(),
            flags: strings(&["EN"]),
            checking: false,
            working: true,
            selected: false,
        },
        SourceItem {
            index: 2,
            label: "Mirror CDN · 1080p".into(),
            kind: "DASH".into(),
            size: "".into(),
            flags: strings(&[]),
            checking: true,
            working: false,
            selected: false,
        },
    ]))
}

fn sample_source_menu_info() -> SourceMenuInfo {
    let chip = |label: &str, count: &str| FilterChip {
        label: label.into(),
        count: count.into(),
        on: false,
    };
    SourceMenuInfo {
        total: 3,
        resolution_shown: 3,
        subtitle_shown: 3,
        tiers: ModelRc::new(VecModel::from(vec![chip("1080p", "2"), chip("2160p", "1")])),
        subtitles: ModelRc::new(VecModel::from(vec![chip("EN", ""), chip("AR", "")])),
    }
}

fn sample_subtitle_tracks() -> ModelRc<TrackChoice> {
    let track = |id: i32, label: &str, flag: &str, selected: bool| TrackChoice {
        label: label.into(),
        id,
        flag: flag.into(),
        selected,
    };
    ModelRc::new(VecModel::from(vec![
        track(-1, "Off", "", false),
        track(1, "English (CC)", "EN", true),
        track(2, "Arabic", "AR", false),
        track(3, "French", "", false),
    ]))
}

fn sample_video_frame() -> Image {
    let (w, h) = (320u32, 200u32);
    let mut buf = SharedPixelBuffer::<Rgb8Pixel>::new(w, h);
    for (idx, px) in buf.make_mut_slice().iter_mut().enumerate() {
        let x = (idx as u32) % w;
        let y = (idx as u32) / w;
        let horizon = y > 118;
        px.r = if horizon {
            (40 + x / 6).min(255) as u8
        } else {
            (32 + (y * 140 / 118)).min(255) as u8
        };
        px.g = if horizon {
            (62 + (y - 118) / 2).min(255) as u8
        } else {
            (58 + (x / 5)).min(255) as u8
        };
        px.b = if horizon {
            (108 + x / 4).min(255) as u8
        } else {
            (116 + (118 - y) / 2).min(255) as u8
        };
    }
    Image::from_rgb8(buf)
}

fn sample_skip_marks() -> ModelRc<SeekMark> {
    let mark = |start: f32, width: f32, kind: i32| SeekMark { start, width, kind };
    ModelRc::new(VecModel::from(vec![
        mark(0.02, 0.04, 1), // Recap (sky)
        mark(0.07, 0.06, 0), // Intro (amber)
        mark(0.87, 0.08, 2), // Credits (emerald)
        mark(0.96, 0.03, 3), // Preview (fuchsia)
    ]))
}

fn populate_player(ui: &CloudStream) {
    ui.set_player_open(true);
    ui.set_video_frame(sample_video_frame());
    ui.set_player_title("Severance · S2 E4 — Woe's Hollow".into());
    ui.set_player_source_badge("1080p · HLS".into());
    ui.set_duration_ms(3_120_000.0);
    ui.set_position_ms(690_000.0);
    ui.set_position_text("11:30".into());
    ui.set_duration_text("52:00".into());
    ui.set_buffered_fraction(0.46);
    ui.set_chapter_notches(ModelRc::new(VecModel::from(vec![
        0.07, 0.13, 0.24, 0.35, 0.47, 0.58, 0.69, 0.78, 0.87, 0.96,
    ])));
    ui.set_skip_marks(sample_skip_marks());
    ui.set_has_previous_episode(false);
    ui.set_has_next_episode(false);
    ui.set_skip_label("Skip Intro".into());
    ui.set_sources(sample_sources());
    ui.set_source_info(sample_source_menu_info());
    ui.set_subtitle_tracks(sample_subtitle_tracks());
}

fn dispatch_scroll(window: &MinimalSoftwareWindow, x: f32, y: f32) {
    let pos = LogicalPosition::new(x, y);
    window.dispatch_event(WindowEvent::PointerMoved { position: pos });
    window.dispatch_event(WindowEvent::PointerScrolled {
        position: pos,
        delta_x: 0.0,
        delta_y: 60.0,
    });
}

fn verify_player_scroll_isolation(ui: &CloudStream, window: &MinimalSoftwareWindow) {
    let wheel_hits = Rc::new(RefCell::new(0usize));
    let hits_ref = wheel_hits.clone();
    ui.on_wheel_volume(move |_| *hits_ref.borrow_mut() += 1);
    ui.on_preview_seek(|_| SeekPreview {
        time: "28:15".into(),
        chapter: "Woe's Hollow".into(),
        part: "Credits".into(),
    });

    ui.set_show_sources(false);
    ui.set_player_open_panel(0);
    // Hover over the seek bar so 09a_player captures the vertical cursor line, thumb & tooltip.
    window.dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(720.0, 713.0),
    });
    assert_non_blank(&render_frame(window, "09a_player"), "player");
    dispatch_scroll(window, 400.0, 400.0);
    assert_eq!(*wheel_hits.borrow(), 1, "scrolling video adjusts volume");

    ui.set_show_sources(true);
    ui.set_sources_loading(true);
    ui.set_sources_status("Searching & checking… 2 of 3".into());
    assert_non_blank(
        &render_frame(window, "09b_player_sources"),
        "player_sources",
    );
    ui.set_sources_loading(false);
    ui.set_sources_status("".into());
    dispatch_scroll(window, 1040.0, 260.0);
    assert_eq!(*wheel_hits.borrow(), 1, "Sources hover blocks wheel volume");
    dispatch_scroll(window, 400.0, 400.0);
    assert_eq!(*wheel_hits.borrow(), 2, "video outside Sources still works");

    verify_track_drawer_scroll(ui, window, &wheel_hits);
}

fn verify_track_drawer_scroll(
    ui: &CloudStream,
    window: &MinimalSoftwareWindow,
    wheel_hits: &Rc<RefCell<usize>>,
) {
    ui.set_show_sources(false);
    ui.set_player_open_panel(2);
    ui.set_subtitles_loading(true);
    ui.set_subtitles_status("Loading subtitles… (2 of 5)".into());
    ui.set_player_waiting("Buffering 45%".into());
    ui.set_player_feedback_icon("volume".into());
    ui.set_player_feedback("Volume 75%".into());
    ui.set_player_feedback_level(0.75);
    ui.set_player_feedback_count(1);
    assert_non_blank(&render_frame(window, "09c_player_tracks"), "player_tracks");
    ui.set_subtitles_loading(false);
    ui.set_subtitles_status("".into());
    dispatch_scroll(window, 1080.0, 180.0);
    assert_eq!(
        *wheel_hits.borrow(),
        2,
        "Subtitles hover blocks wheel volume"
    );
    dispatch_scroll(window, 400.0, 400.0);
    assert_eq!(
        *wheel_hits.borrow(),
        3,
        "video outside Subtitles still works"
    );
}

fn verify_overlays(ui: &CloudStream, window: &MinimalSoftwareWindow) {
    populate_details(ui);
    assert_non_blank(&render_frame(window, "08_details"), "details");
    ui.set_details_title("".into());
    populate_player(ui);
    verify_player_scroll_isolation(ui, window);
    ui.set_player_open_panel(0);
    ui.set_player_waiting("".into());
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

    verify_home_views(&ui, &window);
    verify_settings_and_browse(&ui, &window);
    verify_overlays(&ui, &window);
    verify_callbacks(&ui, &worker_seen);
}

fn verify_callbacks(ui: &CloudStream, worker_seen: &Rc<RefCell<String>>) {
    ui.invoke_set_worker_limit("Unlimited".into());
    assert_eq!(worker_seen.borrow().as_str(), "Unlimited");

    let opened_repo = Rc::new(RefCell::new(String::new()));
    let opened_ref = opened_repo.clone();
    ui.global::<RepoList>()
        .on_open(move |url| *opened_ref.borrow_mut() = url.to_string());
    ui.global::<RepoList>()
        .invoke_open("https://anime-repo.example/repo.json".into());
    assert_eq!(
        opened_repo.borrow().as_str(),
        "https://anime-repo.example/repo.json"
    );

    let expanded_shelf = Rc::new(RefCell::new(-1));
    let shelf_ref = expanded_shelf.clone();
    ui.global::<HomeExpanded>()
        .on_open_shelf(move |idx| *shelf_ref.borrow_mut() = idx);
    ui.global::<HomeExpanded>().invoke_open_shelf(1);
    assert_eq!(*expanded_shelf.borrow(), 1);

    let browse_page = Rc::new(RefCell::new(0));
    let page_ref = browse_page.clone();
    ui.global::<BrowseFilter>()
        .on_go_to_page(move |p| *page_ref.borrow_mut() = p);
    ui.global::<BrowseFilter>().invoke_go_to_page(5);
    assert_eq!(*browse_page.borrow(), 5);

    let vol_seen = Rc::new(RefCell::new(-1.0_f32));
    let vol_ref = vol_seen.clone();
    ui.on_set_volume(move |v| *vol_ref.borrow_mut() = v);
    ui.invoke_set_volume(65.0);
    assert!((*vol_seen.borrow() - 65.0).abs() < 0.01);
    verify_details_callbacks(ui);
}

fn verify_details_callbacks(ui: &CloudStream) {
    let sort_toggled = Rc::new(RefCell::new(0usize));
    let sort_ref = sort_toggled.clone();
    ui.on_toggle_episode_sort(move || *sort_ref.borrow_mut() += 1);
    ui.invoke_toggle_episode_sort();
    assert_eq!(*sort_toggled.borrow(), 1);

    let range_seen = Rc::new(RefCell::new((-1, -1)));
    let macro_ref = range_seen.clone();
    ui.on_select_episode_macro(move |m| macro_ref.borrow_mut().0 = m);
    let sub_ref = range_seen.clone();
    ui.on_select_episode_range(move |r| sub_ref.borrow_mut().1 = r);
    ui.invoke_select_episode_macro(2);
    ui.invoke_select_episode_range(3);
    assert_eq!(*range_seen.borrow(), (2, 3));
}
