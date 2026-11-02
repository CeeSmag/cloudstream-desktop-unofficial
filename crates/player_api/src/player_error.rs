/// Why a player operation failed.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum PlayerError {
    /// The player library could not be found or loaded.
    #[error("player library unavailable: {0}")]
    Unavailable(String),
    /// The player rejected a command.
    #[error("player command '{command}' failed: {reason}")]
    Command {
        /// Command or property name.
        command: String,
        /// Reason reported by the player.
        reason: String,
    },
}
