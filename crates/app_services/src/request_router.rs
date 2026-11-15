use player_api::PlaybackRequest;

/// Decides, stream by stream, what the player really opens: the stream's own address, or another
/// one that leads to it (a local proxy that adds the headers the host wants).
pub trait RequestRouter: Send + Sync {
    /// The request the player should be given for `request`; a copy of it when nothing changes.
    fn route(&self, request: &PlaybackRequest) -> PlaybackRequest;

    /// The stream is over (stopped, or the player could not start it): let go of whatever
    /// [`RequestRouter::route`] set up for it.
    fn release(&self);
}
