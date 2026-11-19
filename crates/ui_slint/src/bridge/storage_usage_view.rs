//! Settings > Storage: how much of the limit the data folder uses, and by what (like the old
//! StorageCapCard's bar and breakdown). Measured on a worker thread when Settings opens, after a
//! cleanup and when the limit changes.

use app_services::{measure_storage, StorageUsage};

use crate::background_work::spawn_work;
use crate::posters::string_model;
use crate::source_menu::file_size_text;
use crate::ui_events::UiEvent;
use crate::CloudStream;

use super::bridge_context::BridgeContext;

const GB: u64 = 1024 * 1024 * 1024;

/// Measures the data folder in the background; the answer fills the Storage card.
pub(crate) fn measure_storage_usage(ctx: &BridgeContext) {
    let Some(dir) = ctx.extras.borrow().data_dir.clone() else {
        return;
    };
    let sender = ctx.sender();
    spawn_work("storage-usage", move || {
        sender.send(UiEvent::StorageMeasured(measure_storage(&dir)));
    });
}

pub(crate) fn show_storage_usage(ctx: &BridgeContext, ui: &CloudStream, usage: &StorageUsage) {
    let cap_gb = ctx.services.settings.storage_cap_gb().unwrap_or(2);
    let cap_bytes = u64::from(cap_gb) * GB;
    ui.set_storage_fraction(usage_fraction(usage.total_bytes, cap_bytes));
    ui.set_storage_over(usage.total_bytes > cap_bytes);
    ui.set_storage_label(usage_label(usage.total_bytes, cap_gb).into());
    ui.set_storage_rows(string_model(usage_rows(usage)));
}

/// How full the limit is, 0 to 1.
pub(crate) fn usage_fraction(total_bytes: u64, cap_bytes: u64) -> f32 {
    if cap_bytes == 0 {
        return 0.0;
    }
    (total_bytes as f64 / cap_bytes as f64).clamp(0.0, 1.0) as f32
}

/// "1.2 GB of 2 GB used", with a warning when over.
pub(crate) fn usage_label(total_bytes: u64, cap_gb: u32) -> String {
    let used = file_size_text(total_bytes);
    if total_bytes > u64::from(cap_gb) * GB {
        format!(
            "{used} of {cap_gb} GB used: over the limit. Clean up removes what can be made again."
        )
    } else {
        format!("{used} of {cap_gb} GB used")
    }
}

/// "Saved sources  ·  12.0 MB" for each listed folder that holds anything.
pub(crate) fn usage_rows(usage: &StorageUsage) -> Vec<String> {
    usage
        .folders
        .iter()
        .filter(|folder| folder.bytes > 0)
        .map(|folder| format!("{}  ·  {}", folder.label, file_size_text(folder.bytes)))
        .collect()
}

#[cfg(test)]
mod tests {
    use app_services::FolderUsage;

    use super::*;

    #[test]
    fn usage_label_and_fraction_say_how_full_the_limit_is() {
        assert_eq!(usage_label(512 * 1024 * 1024, 2), "512.0 MB of 2 GB used");
        assert!(usage_label(3 * GB, 2).contains("over the limit"));
        assert!((usage_fraction(GB, 2 * GB) - 0.5).abs() < 1e-6);
        assert!((usage_fraction(5 * GB, 2 * GB) - 1.0).abs() < 1e-6);
        assert!(usage_fraction(1, 0).abs() < 1e-6);
    }

    #[test]
    fn usage_rows_list_folders_that_hold_something() {
        let usage = StorageUsage {
            total_bytes: 0,
            folders: vec![
                FolderUsage {
                    label: "Logs",
                    bytes: 2048,
                },
                FolderUsage {
                    label: "Updates",
                    bytes: 0,
                },
            ],
        };
        assert_eq!(usage_rows(&usage), ["Logs  ·  2.0 KB"]);
    }
}
