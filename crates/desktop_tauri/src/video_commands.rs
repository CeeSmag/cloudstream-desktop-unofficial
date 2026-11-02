//! Tauri commands that hand mpv's pictures to the page.

use app_services::PlaybackService;
use player_api::VideoFrame;
use tauri::ipc::Response;
use tauri::State;

use crate::app_state::AppState;

/// Bytes before the pixels: width (u32), height (u32), serial (u64), all little-endian.
const FRAME_HEADER_LEN: usize = 16;

/// Tells the player how large the picture area is, in physical pixels.
#[tauri::command]
pub async fn set_video_surface(
    width: u32,
    height: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let services = state.read();
    let playback = playback_of(&services.playback)?;
    playback.set_surface_size(width, height);
    Ok(())
}

/// Returns the newest frame as `header + RGBA`, or an empty body when there is nothing newer
/// than `last_serial`.
#[tauri::command]
pub async fn get_video_frame(
    last_serial: u64,
    state: State<'_, AppState>,
) -> Result<Response, String> {
    let services = state.read();
    let playback = playback_of(&services.playback)?;
    let body = playback
        .latest_frame(last_serial)
        .map(|frame| encode_frame(&frame))
        .unwrap_or_default();
    Ok(Response::new(body))
}

fn playback_of(
    playback: &Option<std::sync::Arc<PlaybackService>>,
) -> Result<&std::sync::Arc<PlaybackService>, String> {
    playback
        .as_ref()
        .ok_or_else(|| "Playback service unavailable".to_owned())
}

/// Serialises a frame for the page: a 16-byte header followed by the RGBA pixels.
pub(crate) fn encode_frame(frame: &VideoFrame) -> Vec<u8> {
    let mut body = Vec::with_capacity(FRAME_HEADER_LEN + frame.rgba.len());
    body.extend_from_slice(&frame.width.to_le_bytes());
    body.extend_from_slice(&frame.height.to_le_bytes());
    body.extend_from_slice(&frame.serial.to_le_bytes());
    body.extend_from_slice(&frame.rgba);
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_frame_puts_dimensions_and_serial_before_the_pixels() {
        let frame = VideoFrame {
            width: 2,
            height: 1,
            rgba: vec![1, 2, 3, 255, 4, 5, 6, 255],
            serial: 9,
        };
        let body = encode_frame(&frame);
        assert_eq!(body.len(), FRAME_HEADER_LEN + 8);
        assert_eq!(body.get(0..4), Some(&2u32.to_le_bytes()[..]));
        assert_eq!(body.get(4..8), Some(&1u32.to_le_bytes()[..]));
        assert_eq!(body.get(8..16), Some(&9u64.to_le_bytes()[..]));
        assert_eq!(body.get(16..), Some(&frame.rgba[..]));
    }
}
