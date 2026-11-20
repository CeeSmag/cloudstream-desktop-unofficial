//! Regression tests proving no screen, overlay, or dialog caps `CloudStream`'s window max size.
//!
//! In Slint 1.19, `WindowAdapter::update_window_properties` receives the root `Window`'s
//! `LayoutConstraints`. If any active subtree emits a finite `max` width or height, `winit` calls
//! `set_max_inner_size` and shrinks/unmaximizes a maximized desktop window.

use std::cell::Cell;
use std::rc::{Rc, Weak};

use slint::platform::software_renderer::{RepaintBufferType, SoftwareRenderer};
use slint::platform::{
    LayoutConstraints, Platform, Renderer, WindowAdapter, WindowEvent, WindowProperties,
};
use slint::{
    ComponentHandle, LogicalSize, ModelRc, PhysicalSize, PlatformError, Rgb8Pixel, SharedString,
    VecModel, Window, WindowSize,
};
use ui_slint::{CloudStream, HomeExpanded, PosterItem, RepoDetail, ShelfSection};

const MAXIMIZED_WIDTH: u32 = 1920;
const MAXIMIZED_HEIGHT: u32 = 1080;
const EXPECTED_MIN: LogicalSize = LogicalSize::new(760.0, 560.0);
const EXPECTED_PREFERRED: LogicalSize = LogicalSize::new(1180.0, 780.0);

struct RecordingWindowAdapter {
    window: Window,
    renderer: SoftwareRenderer,
    size: Cell<PhysicalSize>,
    constraints: Cell<Option<LayoutConstraints>>,
}

impl RecordingWindowAdapter {
    fn new(size: PhysicalSize) -> Rc<Self> {
        let adapter = Rc::new_cyclic(|weak: &Weak<Self>| Self {
            window: Window::new(weak.clone()),
            renderer: SoftwareRenderer::new_with_repaint_buffer_type(RepaintBufferType::NewBuffer),
            size: Cell::new(size),
            constraints: Cell::new(None),
        });
        adapter.window.dispatch_event(WindowEvent::Resized {
            size: size.to_logical(1.0),
        });
        adapter
    }

    fn render_and_read_constraints(&self) -> Option<LayoutConstraints> {
        slint::platform::update_timers_and_animations();
        let size = self.size.get();
        let mut buffer = vec![Rgb8Pixel::new(0, 0, 0); (size.width * size.height) as usize];
        self.renderer.render(&mut buffer, size.width as usize);
        self.constraints.get()
    }
}

impl WindowAdapter for RecordingWindowAdapter {
    fn window(&self) -> &Window {
        &self.window
    }

    fn size(&self) -> PhysicalSize {
        self.size.get()
    }

    fn set_size(&self, size: WindowSize) {
        let physical = size.to_physical(1.0);
        self.size.set(physical);
        self.window.dispatch_event(WindowEvent::Resized {
            size: physical.to_logical(1.0),
        });
    }

    fn renderer(&self) -> &dyn Renderer {
        &self.renderer
    }

    fn update_window_properties(&self, properties: WindowProperties<'_>) {
        self.constraints.set(Some(properties.layout_constraints()));
    }
}

struct RecordingPlatform {
    adapter: Rc<RecordingWindowAdapter>,
}

impl Platform for RecordingPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.adapter.clone())
    }
}

fn assert_unbounded_window(adapter: &RecordingWindowAdapter, state: &str) {
    let actual = adapter
        .render_and_read_constraints()
        .map(|c| (c.min, c.preferred, c.max));
    assert_eq!(
        actual,
        Some((Some(EXPECTED_MIN), EXPECTED_PREFERRED, None)),
        "state '{state}' altered window constraints (a finite max shrinks a maximized window)"
    );
}

fn sample_poster() -> PosterItem {
    PosterItem {
        title: "Sample Title".into(),
        subtitle: "2026 · Movie".into(),
        provider: "Demo".into(),
        url: "https://example.invalid/title".into(),
        poster: "".into(),
        poster_headers: "".into(),
        media_key: "sample".into(),
        data: "".into(),
        image: slint::Image::default(),
        progress: 0.4,
    }
}

fn check_home_and_catalog_states(ui: &CloudStream, adapter: &RecordingWindowAdapter) {
    ui.set_starting(false);
    ui.set_tab(0);
    ui.set_home_loading(false);
    assert_unbounded_window(adapter, "home_empty_no_providers");

    ui.set_providers(ModelRc::new(VecModel::from(vec![SharedString::from(
        "Demo",
    )])));
    ui.set_selected_provider("Demo".into());
    assert_unbounded_window(adapter, "home_empty_with_provider");

    ui.set_home_loading(true);
    assert_unbounded_window(adapter, "home_loading");
    ui.set_home_loading(false);

    let posters = ModelRc::new(VecModel::from(vec![sample_poster()]));
    ui.set_home_shelves(ModelRc::new(VecModel::from(vec![ShelfSection {
        title: "Popular".into(),
        items: posters.clone(),
    }])));
    ui.set_hero_title("Sample Title".into());
    assert_unbounded_window(adapter, "home_populated");

    ui.global::<HomeExpanded>().set_shelf_index(0);
    assert_unbounded_window(adapter, "home_expanded");
    ui.global::<HomeExpanded>().set_shelf_index(-1);

    ui.set_tab(1);
    ui.set_browse_loading(true);
    assert_unbounded_window(adapter, "browse_loading_empty");
    ui.set_browse_items(posters.clone());
    ui.set_browse_loading(false);
    assert_unbounded_window(adapter, "browse_populated");

    ui.set_tab(2);
    ui.set_search_loading(true);
    assert_unbounded_window(adapter, "search_loading_empty");
    ui.set_search_loading(false);
    ui.set_tab(3);
    assert_unbounded_window(adapter, "library");
}

fn check_settings_and_wipe_states(ui: &CloudStream, adapter: &RecordingWindowAdapter) {
    ui.set_tab(4);
    ui.set_settings_page_index(0);
    assert_unbounded_window(adapter, "settings_general");

    ui.set_show_wipe_confirm(true);
    assert_unbounded_window(adapter, "settings_wipe_confirm_dialog");
    ui.set_show_wipe_confirm(false);

    ui.set_settings_page_index(1);
    assert_unbounded_window(adapter, "settings_preferences");

    ui.set_settings_page_index(2);
    ui.set_extensions_tab(0);
    assert_unbounded_window(adapter, "settings_extensions_repos");

    ui.global::<RepoDetail>()
        .set_open_url("https://example.invalid/repo.json".into());
    assert_unbounded_window(adapter, "settings_extensions_repo_detail");
    ui.global::<RepoDetail>().set_open_url("".into());

    ui.set_extensions_tab(1);
    assert_unbounded_window(adapter, "settings_extensions_plugins");
}

fn check_details_and_player_states(ui: &CloudStream, adapter: &RecordingWindowAdapter) {
    ui.set_tab(0);
    ui.set_details_title("The Lord of the Rings".into());
    ui.set_details_loading(true);
    assert_unbounded_window(adapter, "details_loading");

    ui.set_details_loading(false);
    ui.set_open_busy(true);
    assert_unbounded_window(adapter, "details_loaded_opening");
    ui.set_open_busy(false);
    ui.set_details_title("".into());

    ui.set_player_open(true);
    ui.set_player_title("Demo Stream".into());
    assert_unbounded_window(adapter, "player_video");

    ui.set_player_audio_only(true);
    assert_unbounded_window(adapter, "player_audio_only");
    ui.set_player_audio_only(false);

    ui.set_player_waiting("Buffering 45%".into());
    ui.set_show_sources(true);
    assert_unbounded_window(adapter, "player_sources_and_waiting");
    ui.set_show_sources(false);
    ui.set_player_open_panel(2);
    assert_unbounded_window(adapter, "player_track_panel");
    ui.set_player_open_panel(0);
    ui.set_player_waiting("".into());
    ui.set_player_open(false);
}

fn check_banner_and_toast_states(ui: &CloudStream, adapter: &RecordingWindowAdapter) {
    ui.set_cf_host("example.invalid".into());
    for state in 1..=4 {
        ui.set_cf_state(state);
        assert_unbounded_window(adapter, "cloudflare_banner");
    }
    ui.set_cf_state(0);

    ui.set_notice("Saved to Library".into());
    assert_unbounded_window(adapter, "toast_notice");
    ui.set_notice("".into());
}

#[test]
fn cloudstream_all_screens_keep_window_max_size_unbounded() {
    let adapter = RecordingWindowAdapter::new(PhysicalSize::new(MAXIMIZED_WIDTH, MAXIMIZED_HEIGHT));
    slint::platform::set_platform(Box::new(RecordingPlatform {
        adapter: adapter.clone(),
    }))
    .expect("set recording slint platform");

    let ui = CloudStream::new().expect("create CloudStream window");
    ui.show().expect("show headless window");
    ui.set_grid_columns(10);
    ui.set_wide_window(true);

    assert_unbounded_window(&adapter, "startup_splash");
    check_home_and_catalog_states(&ui, &adapter);
    check_settings_and_wipe_states(&ui, &adapter);
    check_details_and_player_states(&ui, &adapter);
    check_banner_and_toast_states(&ui, &adapter);
}
