//! Home keeps its place: leaving it (title page, another tab) and coming back shows the same
//! vertical and sideways scroll; the Home tab pressed on Home and new Home content go to the top.
//!
//! Headless software renderer; one test per process because Slint's platform is set once.

use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter, WindowEvent};
use slint::{
    ComponentHandle, Image, LogicalPosition, Model, ModelRc, PhysicalSize, PlatformError,
    Rgb8Pixel, SharedPixelBuffer, VecModel,
};
use ui_slint::home_scroll::{reset_home_scroll, wire_home_scroll};
use ui_slint::{CloudStream, HomeExpanded, HomeScroll, PosterItem, ShelfSection};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 800;
/// Posters per row: wider than the window, so rows scroll sideways.
const POSTERS_PER_ROW: usize = 14;
const SHELVES: usize = 6;
/// Longest wait for animations (Home scroll 220 ms, top bar tab highlight) to settle.
const SETTLE_LIMIT: Duration = Duration::from_secs(2);

struct HeadlessPlatform(Rc<MinimalSoftwareWindow>);

impl Platform for HeadlessPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.0.clone())
    }
}

fn render(window: &MinimalSoftwareWindow) -> Vec<Rgb8Pixel> {
    slint::platform::update_timers_and_animations();
    window.request_redraw();
    let mut buffer = vec![Rgb8Pixel::new(0, 0, 0); (WIDTH * HEIGHT) as usize];
    window.draw_if_needed(|renderer| {
        renderer.render(&mut buffer, WIDTH as usize);
    });
    buffer
}

fn artwork(seed: usize) -> Image {
    let mut buf = SharedPixelBuffer::<Rgb8Pixel>::new(32, 48);
    let shade = ((seed * 37) % 200) as u8;
    for (idx, px) in buf.make_mut_slice().iter_mut().enumerate() {
        let y = (idx / 32) as u8;
        *px = Rgb8Pixel::new(shade, 255 - shade, y.saturating_mul(4));
    }
    Image::from_rgb8(buf)
}

fn posters(row: usize) -> ModelRc<PosterItem> {
    let items = (0..POSTERS_PER_ROW)
        .map(|i| PosterItem {
            title: format!("Title {row}-{i}").into(),
            subtitle: "2024 · Movie".into(),
            media_key: format!("{row}-{i}").into(),
            image: artwork(row * POSTERS_PER_ROW + i),
            ..PosterItem::default()
        })
        .collect::<Vec<_>>();
    ModelRc::new(VecModel::from(items))
}

fn populate_home(ui: &CloudStream) {
    ui.set_starting(false);
    ui.set_continue_items(posters(99));
    let shelves = (0..SHELVES)
        .map(|s| ShelfSection {
            title: format!("Shelf {s}").into(),
            items: posters(s),
        })
        .collect::<Vec<_>>();
    ui.set_home_shelves(ModelRc::new(VecModel::from(shelves)));
    ui.set_hero_title("Hero title".into());
    ui.set_hero_subtitle("2024 · Resume".into());
    ui.set_hero_image(artwork(7));
    ui.set_hero_row(0);
    ui.set_continue_row(1);
    ui.set_shelf_row_base(2);
    ui.set_grid_columns(6);
}

/// A wheel turn at (`x`, `y`), then the pointer leaves so no tile stays highlighted.
fn wheel(window: &MinimalSoftwareWindow, (x, y): (f32, f32), (delta_x, delta_y): (f32, f32)) {
    let position = LogicalPosition::new(x, y);
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerScrolled {
        position,
        delta_x,
        delta_y,
    });
    window.dispatch_event(WindowEvent::PointerExited);
}

fn row_offsets(ui: &CloudStream) -> Vec<f32> {
    ui.global::<HomeScroll>().get_row_x().iter().collect()
}

fn assert_same_frame(actual: &[Rgb8Pixel], expected: &[Rgb8Pixel], what: &str) {
    let differing = actual.iter().zip(expected).filter(|(a, b)| a != b).count();
    assert_eq!(differing, 0, "{what}: {differing} pixels differ");
}

/// Renders until the frame equals `expected` or the limit passes; returns the last frame.
fn render_until(window: &MinimalSoftwareWindow, expected: &[Rgb8Pixel]) -> Vec<Rgb8Pixel> {
    let started = Instant::now();
    loop {
        let frame = render(window);
        if frame == expected || started.elapsed() > SETTLE_LIMIT {
            return frame;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Renders until two frames in a row are the same (animations done) or the limit passes.
fn render_settled(window: &MinimalSoftwareWindow) -> Vec<Rgb8Pixel> {
    let started = Instant::now();
    let mut previous = render(window);
    loop {
        std::thread::sleep(Duration::from_millis(20));
        let frame = render(window);
        if frame == previous || started.elapsed() > SETTLE_LIMIT {
            return frame;
        }
        previous = frame;
    }
}

/// Scrolls a row sideways and Home down; returns the frame showing that place.
fn scroll_home(ui: &CloudStream, window: &MinimalSoftwareWindow) -> Vec<Rgb8Pixel> {
    wheel(window, (640.0, 620.0), (-600.0, 0.0));
    render(window);
    assert!(
        row_offsets(ui).iter().any(|x| *x < 0.0),
        "a row scrolled sideways is remembered: {:?}",
        row_offsets(ui)
    );
    wheel(window, (12.0, 500.0), (0.0, -700.0));
    let frame = render_settled(window);
    assert!(
        ui.global::<HomeScroll>().get_page_y() < 0.0,
        "Home's vertical scroll is remembered"
    );
    frame
}

fn verify_title_page_round_trip(
    ui: &CloudStream,
    window: &MinimalSoftwareWindow,
    at: &[Rgb8Pixel],
) {
    ui.set_details_title("Some title".into());
    render(window);
    ui.set_details_title("".into());
    // What `restore_focus` does after a closed title page: the nav row, which on Home alone
    // would scroll to the top.
    ui.set_focus_row(-1);
    assert_same_frame(&render(window), at, "back from a title page");
}

fn verify_tab_round_trip(ui: &CloudStream, window: &MinimalSoftwareWindow, at: &[Rgb8Pixel]) {
    for tab in [4, 1, 2, 3] {
        ui.set_tab(tab);
        render(window);
        ui.set_tab(0);
        assert_same_frame(
            &render_until(window, at),
            at,
            &format!("back from tab {tab}"),
        );
    }
}

fn verify_expanded_shelf_round_trip(ui: &CloudStream, window: &MinimalSoftwareWindow) {
    let expanded = ui.global::<HomeExpanded>();
    expanded.set_items(posters(42));
    expanded.set_shelf_index(1);
    render(window);
    wheel(window, (12.0, 500.0), (0.0, -300.0));
    let at = render_settled(window);
    ui.set_tab(4);
    render(window);
    ui.set_tab(0);
    assert_same_frame(
        &render_until(window, &at),
        &at,
        "expanded shelf, back from Settings",
    );
    expanded.set_shelf_index(-1);
    render(window);
}

fn verify_reset_while_away(ui: &CloudStream, window: &MinimalSoftwareWindow, top: &[Rgb8Pixel]) {
    ui.set_tab(4);
    render(window);
    reset_home_scroll(ui);
    ui.set_tab(0);
    assert_same_frame(
        &render_until(window, top),
        top,
        "new Home content shown at the top",
    );
    assert!(row_offsets(ui).iter().all(|x| *x == 0.0));
}

fn verify_reset_on_home(ui: &CloudStream, window: &MinimalSoftwareWindow, top: &[Rgb8Pixel]) {
    scroll_home(ui, window);
    reset_home_scroll(ui);
    assert_same_frame(&render_until(window, top), top, "Home tab on Home");
    assert_eq!(ui.global::<HomeScroll>().get_page_y(), 0.0);
    assert!(
        row_offsets(ui).iter().all(|x| *x == 0.0),
        "rows back at their start: {:?}",
        row_offsets(ui)
    );
}

#[test]
fn home_scroll_leaving_and_returning_keeps_place_and_reset_goes_to_top() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    window.set_size(PhysicalSize::new(WIDTH, HEIGHT));
    slint::platform::set_platform(Box::new(HeadlessPlatform(window.clone())))
        .expect("headless platform");
    let ui = CloudStream::new().expect("CloudStream window");
    wire_home_scroll(&ui);
    populate_home(&ui);
    let top = render_settled(&window);

    let scrolled = scroll_home(&ui, &window);
    assert!(scrolled != top, "scrolling moved Home");
    verify_title_page_round_trip(&ui, &window, &scrolled);
    verify_tab_round_trip(&ui, &window, &scrolled);
    verify_expanded_shelf_round_trip(&ui, &window);
    verify_reset_while_away(&ui, &window, &top);
    verify_reset_on_home(&ui, &window, &top);
}
