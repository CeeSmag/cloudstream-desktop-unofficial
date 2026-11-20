//! Connects [`RepositoryDiscovery`] to `app_services` and the Extensions screen.

use app_services::{ExtensionService, RepositoryService};
use core_model::RepositoryEntry;

use crate::background_work::UiSender;
use crate::repository_discovery::{DiscoveryUpdate, PollOutcome};
use crate::ui_events::UiEvent;
use crate::CloudStream;

use super::bridge_context::BridgeContext;
use super::repo_list::show_repositories;

/// Starts or extends background polling for repositories registered by loaded extensions.
pub(crate) fn watch_discovered_repositories(ctx: &BridgeContext) {
    let services = ctx.owned_services();
    let sender = ctx.sender();
    let extensions = services.extensions;
    let repositories = services.repositories;
    ctx.session.borrow().repository_discovery.trigger_with(
        move || poll_and_import(&extensions, &repositories),
        move |update| send_update(&sender, update),
    );
}

/// Shows repositories just imported from extensions and informs the viewer.
pub(crate) fn show_discovered_repositories(
    ctx: &BridgeContext,
    ui: &CloudStream,
    added: usize,
    repositories: Vec<RepositoryEntry>,
) {
    ctx.session.borrow_mut().repositories = repositories;
    show_repositories(ctx);
    let notice = if added == 1 {
        "Added 1 repository from plugins".to_owned()
    } else {
        format!("Added {added} repositories from plugins")
    };
    ui.set_notice(notice.into());
}

fn poll_and_import(
    extensions: &ExtensionService,
    repositories: &RepositoryService,
) -> Result<PollOutcome, String> {
    let found = extensions
        .take_added_repositories()
        .map_err(|error| error.to_string())?;
    if found.is_empty() {
        return Ok(PollOutcome::Empty);
    }
    let added = repositories
        .import_discovered(&found)
        .map_err(|error| error.to_string())?;
    let list = if added > 0 {
        repositories.list().map_err(|error| error.to_string())?
    } else {
        Vec::new()
    };
    Ok(PollOutcome::Found {
        added,
        repositories: list,
    })
}

fn send_update(sender: &UiSender, update: DiscoveryUpdate) {
    match update {
        DiscoveryUpdate::Added {
            added,
            repositories,
        } => sender.send(UiEvent::RepositoriesDiscovered {
            added,
            repositories,
        }),
        DiscoveryUpdate::ReloadCatalog => sender.send(UiEvent::ReloadExtensionsCatalog),
    }
}
