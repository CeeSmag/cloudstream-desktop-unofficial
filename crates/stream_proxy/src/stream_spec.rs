/// What the proxy needs to open one stream on the player's behalf.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StreamSpec {
    /// Address of the stream's master or media playlist.
    pub url: String,
    /// Headers the provider says the host needs, as (name, value) pairs.
    pub headers: Vec<(String, String)>,
    /// The referer the host expects; empty for none.
    pub referer: String,
}
