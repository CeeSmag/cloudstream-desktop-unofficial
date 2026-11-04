/// One chapter of the video: a named stretch that starts at `time_ms` and lasts until the next
/// chapter starts (or the video ends).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chapter {
    /// Name given by the file, or `Chapter N` when it has none.
    pub title: String,
    /// Where the chapter starts, in milliseconds.
    pub time_ms: i64,
}
