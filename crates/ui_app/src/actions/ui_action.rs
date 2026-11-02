use core_model::{InstalledExtension, PluginListing};

use crate::app_model::PlayTarget;
use crate::navigation::{Tab, TitleRef};

/// Something the user asked for.
#[derive(Debug, Clone)]
pub(crate) enum UiAction {
    SwitchTab(Tab),
    SelectProvider(String),
    RefreshHome,
    Search(String),
    OpenTitle(TitleRef),
    CloseTitle,
    ToggleBookmark,
    Play(PlayTarget),
    PlayLink(usize),
    DismissStreamChoice,
    SetPaused(bool),
    SeekBy(i64),
    StopPlayback,
    AddRepository(String),
    RemoveRepository(String),
    LoadRepositoryPlugins(String),
    InstallPlugin {
        repository_url: String,
        listing: PluginListing,
    },
    UninstallExtension(InstalledExtension),
    SetTrusted {
        extension: InstalledExtension,
        trusted: bool,
    },
    SetProviderEnabled {
        provider: String,
        enabled: bool,
    },
    DismissNotice(usize),
}
