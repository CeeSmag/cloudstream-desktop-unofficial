//! The content policy on [`ExtensionService`]: support of installed extensions and their content
//! types filled in from the catalog.

use content_policy::RepositoryId;
use core_model::InstalledExtension;
use extension_repo::plugin_path_in;

use crate::content_support::{judge_installed, support_of, InstalledStatus, Support};
use crate::{ExtensionService, PluginHit, ServiceError};

impl ExtensionService {
    /// Installed extensions with whether each is supported (worked out now from its recorded types).
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub fn installed_with_status(&self) -> Result<Vec<InstalledStatus>, ServiceError> {
        Ok(self
            .installed()?
            .into_iter()
            .map(|extension| {
                let support = support_of(judge_installed(&self.policy, &extension).as_ref());
                InstalledStatus { extension, support }
            })
            .collect())
    }

    /// Records the listing's content types for installed extensions that have none yet (installed
    /// before types were recorded), matching each by repository and internal name. One that turns
    /// out not to be a video plugin is unloaded at once and stops being offered. Returns how many
    /// got their types.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn backfill_declared_types(&self, catalog: &[PluginHit]) -> usize {
        let Ok(installed) = self.installed() else {
            return 0;
        };
        let mut filled = 0;
        for extension in installed.iter().filter(|e| e.declared_types.is_none()) {
            let Some(hit) = listing_of(catalog, extension) else {
                continue;
            };
            let types = &hit.plugin.tv_types;
            if let Err(error) = self
                .installed
                .set_declared_types_if_missing(&extension.file_name, types)
            {
                tracing::warn!(extension = %extension.file_name, %error, "could not record the content types");
                continue;
            }
            filled += 1;
            let judged = InstalledExtension {
                declared_types: Some(types.clone()),
                ..extension.clone()
            };
            if let Support::NotVideo(reason) =
                support_of(judge_installed(&self.policy, &judged).as_ref())
            {
                tracing::info!(extension = %extension.file_name, %reason, "unloading: not a video plugin");
                self.unload_refused(&extension.file_name);
            }
        }
        filled
    }

    fn unload_refused(&self, file_name: &str) {
        self.unregister(file_name);
        match plugin_path_in(self.installer.extensions_dir(), file_name) {
            Ok(path) => {
                if let Err(error) = self.gateway.unload_extension(&path) {
                    tracing::warn!(extension = %file_name, %error, "could not unload a refused extension");
                }
            }
            Err(error) => {
                tracing::warn!(extension = %file_name, %error, "extension file name failed the path jail")
            }
        }
    }
}

/// The catalog listing an installed extension came from.
fn listing_of<'a>(
    catalog: &'a [PluginHit],
    extension: &InstalledExtension,
) -> Option<&'a PluginHit> {
    let repository = RepositoryId::of(extension.repository_url.as_deref()?);
    catalog.iter().find(|hit| {
        hit.plugin.internal_name == extension.internal_name
            && RepositoryId::of(&hit.repository_url) == repository
    })
}

#[cfg(test)]
#[path = "extension_content_tests.rs"]
mod tests;
