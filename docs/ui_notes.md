# UI notes

Product UI is `ui_slint` (see `crates/ui_slint/README.md` for bridge layout and pitfalls).
`crates/ui_app` (egui) is reference only.

## Embedded video
`player_mpv` renders with libmpv's software render API (`vo=libmpv`) into RGBA frames.
`ui_slint` copies each new frame on a 16 ms timer (`bridge/player_polling.rs::show_latest_frame`)
into a Slint `Image` (`video-frame` on the player), so controls draw over the picture.
`VideoPlayer::set_surface_size` follows the window size; the renderer still caps width at
1280 px (`player_mpv::frame_renderer::MAX_RENDER_WIDTH`) and the GPU scales up.
If the libmpv build lacks the render API the player falls back to mpv's own window (`vo=gpu`).

## Glass / blur
Slint panels use built-in `backdrop-blur` and `drop-shadow-blur` (e.g. title page ambient
poster in `ui/details.slint`). The egui-era shrink-and-stretch glass
(`video_presenter::blur_thumbnail`, `widgets::glass`, `ambient_backdrop`) stays only in the
`ui_app` reference shell.
