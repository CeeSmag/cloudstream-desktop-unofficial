//! What the start-up screen says and how full its bar is.
//!
//! The extension service reports progress *before* each plugin loads (`finished` = plugins done so
//! far) and once more at the end (`finished == total`). With a single plugin the plain ratio sits
//! at 0% for the whole load and then the screen vanished, so a lone plugin gets a moving bar, and
//! the screen stays up with "Loading Home" until the first shelves arrive.

/// The start-up screen's text and bar.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StartupView {
    pub(crate) label: String,
    /// 0.0 to 1.0; ignored while `indeterminate`.
    pub(crate) fraction: f32,
    /// True when no honest fraction is known: the bar animates instead.
    pub(crate) indeterminate: bool,
}

/// The view while plugins load.
pub(crate) fn plugin_loading_view(finished: usize, total: usize, current: &str) -> StartupView {
    if total == 0 {
        return StartupView {
            label: "Starting…".to_owned(),
            fraction: 0.0,
            indeterminate: true,
        };
    }
    if finished >= total {
        return StartupView {
            label: "Plugins ready".to_owned(),
            fraction: 1.0,
            indeterminate: false,
        };
    }
    let plugin = if current.is_empty() {
        "plugin"
    } else {
        current
    };
    StartupView {
        label: format!("Loading {plugin} ({} of {total})", finished + 1),
        fraction: finished as f32 / total as f32,
        indeterminate: total == 1,
    }
}

/// The view after the plugins loaded, while the first Home shelves load.
pub(crate) fn home_loading_view(provider: &str) -> StartupView {
    StartupView {
        label: format!("Loading Home from {provider}…"),
        fraction: 1.0,
        indeterminate: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_loading_view_no_plugins_is_indeterminate() {
        let view = plugin_loading_view(0, 0, "");
        assert!(view.indeterminate);
        assert_eq!(view.label, "Starting…");
    }

    #[test]
    fn plugin_loading_view_single_plugin_animates_instead_of_sitting_at_zero() {
        let view = plugin_loading_view(0, 1, "CineStream");
        assert!(view.indeterminate);
        assert_eq!(view.label, "Loading CineStream (1 of 1)");
    }

    #[test]
    fn plugin_loading_view_many_plugins_counts_from_one_and_fills_by_ratio() {
        let view = plugin_loading_view(1, 4, "Two");
        assert!(!view.indeterminate);
        assert_eq!(view.label, "Loading Two (2 of 4)");
        assert!((view.fraction - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn plugin_loading_view_final_report_is_full() {
        let view = plugin_loading_view(3, 3, "");
        assert_eq!(view.fraction, 1.0);
        assert!(!view.indeterminate);
    }

    #[test]
    fn home_loading_view_names_the_provider() {
        let view = home_loading_view("CineStream");
        assert_eq!(view.label, "Loading Home from CineStream…");
        assert!(view.indeterminate);
    }
}
