use extension_api::GatewayError;
use extension_repo::RepoError;
use player_api::PlayerError;
use storage::StorageError;

use crate::security_block::SecurityBlock;

/// Why a use case failed. `Display` is written for people; details are logged where they occur.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    /// The extension runtime failed.
    #[error(transparent)]
    Gateway(#[from] GatewayError),
    /// A repository or download operation failed.
    #[error(transparent)]
    Repo(#[from] RepoError),
    /// The local database failed.
    #[error(transparent)]
    Storage(#[from] StorageError),
    /// The video player failed.
    #[error(transparent)]
    Player(#[from] PlayerError),
    /// The thing asked for does not exist (unknown provider, no streams, ...).
    #[error("{0}")]
    NotFound(String),
    /// The plugin is not a video plugin, so the app does not install or load it.
    #[error("{plugin} is {reason}")]
    NotVideo {
        /// Its name.
        plugin: String,
        /// Why ("not a video plugin (it declares Others)").
        reason: String,
    },
    /// The plugin was refused by the security sandbox or hard classloader policy.
    #[error("{} was blocked for security reasons ({})", .0.plugin, .0.short_summary())]
    SecurityBlocked(Box<SecurityBlock>),
}
