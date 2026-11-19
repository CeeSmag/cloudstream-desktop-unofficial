//! Default HTTP User-Agent for stream requests (check, player, proxy).

/// Same Chrome desktop string CloudStream Android uses when a link names no User-Agent
/// (`MainAPI.USER_AGENT`). Keep check, mpv and the stream proxy on this so a host that
/// accepts the Android player is not failed by a `libmpv` probe.
pub const DEFAULT_STREAM_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/149.0.0.0 Safari/537.36";
