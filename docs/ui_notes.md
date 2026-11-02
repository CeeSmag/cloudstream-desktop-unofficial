# UI notes

## Embedded video
`player_mpv` renders with libmpv's software render API (`vo=libmpv`) into RGBA frames that the UI
uploads as an egui texture, so the controls are drawn *over* the video. The picture is rendered at
most 1280 px wide and scaled up by the GPU; `VideoPlayer::set_surface_size` tells it the area.
If the libmpv build lacks the render API the player falls back to mpv's own window (`vo=gpu`).
A GPU path (OpenGL render API on eframe's glow backend) would remove the CPU cost for 4K/60.

## Glass
True backdrop blur needs the framebuffer behind a panel. The crates found (checked 2026-11) pin older
egui versions than this app uses (egui 0.37): backdrop-blur-egui 0.2 -> egui 0.34, egui_glass 0.1.5 ->
0.35, kylig-fx / egui-shader-layers -> 0.36. Until one of them catches up, glass is built from:
- the player's frame shrunk to 48 px and stretched back up (GPU linear filtering = blur) behind the
  video controls (`video_presenter::blur_thumbnail`, `widgets::glass`);
- the artwork shrunk to 24x36 px and stretched across hero banners (`ambient_backdrop`).
