/// Why mpv stopped a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EndReason {
    /// Reached the end of the stream.
    Finished,
    /// Stopped by a command or by closing the player.
    Stopped,
    /// Playback failed with the given mpv error code.
    Failed(i32),
}

/// The few mpv events the tracker cares about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MpvEvent {
    /// Nothing happened within the wait timeout.
    Nothing,
    /// mpv is shutting down.
    Shutdown,
    /// A file started loading.
    StartFile,
    /// The file finished loading and playback begins.
    FileLoaded,
    /// A file ended.
    EndFile(EndReason),
    /// An event the app does not use.
    Other,
}

const EVENT_NONE: i32 = 0;
const EVENT_SHUTDOWN: i32 = 1;
const EVENT_START_FILE: i32 = 6;
const EVENT_END_FILE: i32 = 7;
const EVENT_FILE_LOADED: i32 = 8;

const END_REASON_EOF: i32 = 0;
const END_REASON_ERROR: i32 = 4;

/// Maps an `mpv_event_end_file` reason/error pair to an `EndReason`.
pub(crate) fn decode_end_reason(reason: i32, error: i32) -> EndReason {
    match reason {
        END_REASON_EOF => EndReason::Finished,
        END_REASON_ERROR => EndReason::Failed(error),
        _ => EndReason::Stopped,
    }
}

/// Maps a raw event id (plus end-file details when relevant) to an `MpvEvent`.
pub(crate) fn decode_event(event_id: i32, end_file: Option<(i32, i32)>) -> MpvEvent {
    match event_id {
        EVENT_NONE => MpvEvent::Nothing,
        EVENT_SHUTDOWN => MpvEvent::Shutdown,
        EVENT_START_FILE => MpvEvent::StartFile,
        EVENT_FILE_LOADED => MpvEvent::FileLoaded,
        EVENT_END_FILE => {
            let (reason, error) = end_file.unwrap_or((1, 0));
            MpvEvent::EndFile(decode_end_reason(reason, error))
        }
        _ => MpvEvent::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_reasons_map_to_finished_stopped_failed() {
        assert_eq!(decode_end_reason(0, 0), EndReason::Finished);
        assert_eq!(decode_end_reason(2, 0), EndReason::Stopped);
        assert_eq!(decode_end_reason(3, 0), EndReason::Stopped);
        assert_eq!(decode_end_reason(4, -13), EndReason::Failed(-13));
    }

    #[test]
    fn events_map_by_id() {
        assert_eq!(decode_event(0, None), MpvEvent::Nothing);
        assert_eq!(decode_event(1, None), MpvEvent::Shutdown);
        assert_eq!(decode_event(6, None), MpvEvent::StartFile);
        assert_eq!(decode_event(8, None), MpvEvent::FileLoaded);
        assert_eq!(
            decode_event(7, Some((0, 0))),
            MpvEvent::EndFile(EndReason::Finished)
        );
        assert_eq!(decode_event(99, None), MpvEvent::Other);
    }

    #[test]
    fn end_file_without_details_is_treated_as_stopped() {
        assert_eq!(decode_event(7, None), MpvEvent::EndFile(EndReason::Stopped));
    }
}
