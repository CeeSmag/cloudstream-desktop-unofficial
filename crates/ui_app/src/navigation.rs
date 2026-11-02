/// Top-level destinations in the bottom bar, in display order.
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
    /// All tabs in bottom-bar order.
    pub(crate) const ALL: [Tab; 5] = [
        Tab::Home,
        Tab::Search,
        Tab::Library,
        Tab::Downloads,
        Tab::Settings,
    ];

    /// Label shown under the icon.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Tab::Home => "Home",
            Tab::Search => "Search",
            Tab::Library => "Library",
            Tab::Downloads => "Downloads",
            Tab::Settings => "Settings",
        }
    }

    /// Glyph used as the icon.
    pub(crate) fn glyph(self) -> &'static str {
        match self {
            Tab::Home => "\u{2302}",
            Tab::Search => "\u{1F50D}",
            Tab::Library => "\u{2605}",
            Tab::Downloads => "\u{2B07}",
            Tab::Settings => "\u{2699}",
        }
    }
}

/// Where a title lives: provider name and the provider's URL for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TitleRef {
    pub(crate) provider: String,
    pub(crate) url: String,
}
