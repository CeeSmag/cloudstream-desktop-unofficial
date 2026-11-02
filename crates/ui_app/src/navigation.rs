/// Top-level destinations in the top bar, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Tab {
    #[default]
    Home,
    Search,
    Library,
    Downloads,
    Settings,
}

impl Tab {
    /// All tabs in bar order.
    pub(crate) const ALL: [Tab; 5] = [
        Tab::Home,
        Tab::Search,
        Tab::Library,
        Tab::Downloads,
        Tab::Settings,
    ];

    /// Label shown in the top bar.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Tab::Home => "Home",
            Tab::Search => "Search",
            Tab::Library => "Library",
            Tab::Downloads => "Downloads",
            Tab::Settings => "Settings",
        }
    }
}

/// Where a title lives: provider name and the provider's URL for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TitleRef {
    pub(crate) provider: String,
    pub(crate) url: String,
}
