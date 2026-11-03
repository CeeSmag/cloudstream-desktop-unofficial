//! Player-neutral contract: what the app asks of a video player and what it reports back.
#![forbid(unsafe_code)]

mod language;
mod media_track;
mod playback_request;
mod playback_status;
mod player_error;
mod video_frame;
mod video_player;

pub use language::{all_languages, find_language, Language, ENGLISH};
pub use media_track::{MediaTrack, TrackKind};
pub use playback_request::{describe_languages, PlaybackRequest, SubtitleSource};
pub use playback_status::{PlaybackState, PlaybackStatus};
pub use player_error::PlayerError;
pub use video_frame::VideoFrame;
pub use video_player::VideoPlayer;
