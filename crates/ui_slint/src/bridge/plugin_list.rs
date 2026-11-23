//! The Extensions > Plugins tab's catalog list: search, language, installed / not installed,
//! sort (Name, Language, Size, Date added), and a page at a time.

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::plugin_filter::{
    catalog_count_text, filter_plugins, language_choices, PluginQuery, PluginSort, PluginView,
};
use crate::posters::string_model;
use crate::{CloudStream, PluginList};

use super::bridge_context::BridgeContext;
use super::extension_catalog::available_row;

/// Plugins shown at first and added by each "Show more".
const PAGE: usize = 50;
const ALL_LANGUAGES: &str = "All languages";

pub(crate) fn wire_plugin_list(ctx: &BridgeContext, ui: &CloudStream) {
    let list = ui.global::<PluginList>();
    let c = ctx.clone();
    list.on_changed(move || {
        c.session.borrow_mut().plugin_page = PAGE;
        show_available(&c);
    });
    let c = ctx.clone();
    list.on_more(move || {
        c.session.borrow_mut().plugin_page += PAGE;
        show_available(&c);
    });
}

/// Fills the language list from a new catalog and shows its first page.
pub(crate) fn show_new_catalog(ctx: &BridgeContext, ui: &CloudStream) {
    let mut labels = vec![ALL_LANGUAGES.to_owned()];
    labels.extend(language_choices(&ctx.session.borrow().available));
    ui.global::<PluginList>()
        .set_languages(string_model(labels));
    ctx.session.borrow_mut().plugin_page = PAGE;
    show_available(ctx);
}

/// Shows the plugins that fit the viewer's search and filters, one page more at a time.
pub(crate) fn show_available(ctx: &BridgeContext) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let query = query_of(&ui.global::<PluginList>());
    let session = ctx.session.borrow();
    let found = filter_plugins(&session.available, &session.installed_times, &query);
    let rows: Vec<_> = found
        .iter()
        .take(session.plugin_page)
        .map(|hit| available_row(hit, &session.installed_times, true))
        .collect();
    let list = ui.global::<PluginList>();
    list.set_has_more(found.len() > rows.len());
    list.set_count_text(
        catalog_count_text(
            found.len(),
            session.available.len(),
            session.not_video_hidden,
        )
        .into(),
    );
    ui.set_available_plugins(ModelRc::new(VecModel::from(rows)));
}

fn query_of(list: &PluginList<'_>) -> PluginQuery {
    let language = list.get_language();
    PluginQuery {
        text: list.get_search().to_string(),
        language: if language == ALL_LANGUAGES {
            String::new()
        } else {
            language.to_string()
        },
        repository_url: String::new(),
        view: match list.get_view() {
            1 => PluginView::NotInstalled,
            2 => PluginView::Installed,
            _ => PluginView::All,
        },
        sort: sort_of(&list.get_sort()),
    }
}

/// The sort a drop-down label names.
pub(crate) fn sort_of(label: &str) -> PluginSort {
    match label {
        "Sort: Language" => PluginSort::Language,
        "Sort: Size" => PluginSort::Size,
        "Sort: Date added" => PluginSort::DateAdded,
        _ => PluginSort::Name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_of_reads_the_drop_down_labels() {
        assert_eq!(sort_of("Sort: Size"), PluginSort::Size);
        assert_eq!(sort_of("Sort: Language"), PluginSort::Language);
        assert_eq!(sort_of("Sort: Date added"), PluginSort::DateAdded);
        assert_eq!(sort_of("Sort: Name"), PluginSort::Name);
        assert_eq!(sort_of("anything else"), PluginSort::Name);
    }
}
