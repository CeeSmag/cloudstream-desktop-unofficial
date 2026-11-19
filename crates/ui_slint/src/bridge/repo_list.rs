//! Extensions > Repositories tab: repository search & sort, plus the per-repository plugins view.

use std::collections::HashMap;

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::plugin_filter::{filter_plugins, language_choices, PluginQuery, PluginView};
use crate::posters::string_model;
use crate::repository_filter::{
    filter_repositories, repository_detail, RepositoryQuery, RepositorySort,
};
use crate::{CloudStream, RepoDetail, RepoList, RepositoryItem};

use super::bridge_context::BridgeContext;
use super::extension_catalog::available_row;
use super::plugin_list::sort_of;

/// Plugins shown at first inside a repository and added by each "Show more".
const PAGE: usize = 50;
const ALL_LANGUAGES: &str = "All languages";

pub(crate) fn wire_repo_list(ctx: &BridgeContext, ui: &CloudStream) {
    let repos = ui.global::<RepoList>();
    let c = ctx.clone();
    repos.on_changed(move || show_repositories(&c));
    let c = ctx.clone();
    repos.on_open(move |url| open_repo_detail(&c, url.to_string()));

    let detail = ui.global::<RepoDetail>();
    let c = ctx.clone();
    detail.on_changed(move || {
        c.session.borrow_mut().repo_detail_page = PAGE;
        show_repo_detail(&c);
    });
    let c = ctx.clone();
    detail.on_more(move || {
        c.session.borrow_mut().repo_detail_page += PAGE;
        show_repo_detail(&c);
    });
    let c = ctx.clone();
    detail.on_close(move || close_repo_detail(&c));
}

/// Applies search and sort to the Repositories list and updates `RepoList` counts.
pub(crate) fn show_repositories(ctx: &BridgeContext) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let list = ui.global::<RepoList>();
    let query = RepositoryQuery {
        text: list.get_search().to_string(),
        sort: repo_sort_of(&list.get_sort()),
    };
    let session = ctx.session.borrow();
    let total = session.repositories.len();
    let found = filter_repositories(&session.repositories, &query);
    let counts = plugin_counts_by_repo(&session.available);
    let loading = ui.get_plugins_loading();
    let rows: Vec<RepositoryItem> = found
        .iter()
        .map(|repo| RepositoryItem {
            name: repo.name.clone().into(),
            url: repo.url.clone().into(),
            detail: repository_detail(repo).into(),
            plugin_count: plugin_count_badge(counts.get(repo.url.as_str()).copied(), loading)
                .into(),
        })
        .collect();
    list.set_total_count(i32::try_from(total).unwrap_or(i32::MAX));
    list.set_count_text(repo_count_text(found.len(), total, !query.text.trim().is_empty()).into());
    ui.set_repositories(ModelRc::new(VecModel::from(rows)));
}

/// Opens the plugins view for one clicked repository.
pub(crate) fn open_repo_detail(ctx: &BridgeContext, url: String) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let (name, languages) = {
        let session = ctx.session.borrow();
        let name = session
            .repositories
            .iter()
            .find(|repo| repo.url == url)
            .map(|repo| repo.name.clone())
            .unwrap_or_else(|| url.clone());
        let repo_hits: Vec<_> = session
            .available
            .iter()
            .filter(|hit| hit.repository_url == url)
            .cloned()
            .collect();
        (name, language_labels(&repo_hits))
    };
    let detail = ui.global::<RepoDetail>();
    detail.set_open_url(url.into());
    detail.set_name(name.into());
    detail.set_search("".into());
    detail.set_languages(string_model(languages));
    detail.set_language(ALL_LANGUAGES.into());
    detail.set_sort("Sort: Name".into());
    detail.set_view(0);
    ctx.session.borrow_mut().repo_detail_page = PAGE;
    show_repo_detail(ctx);
}

/// Refreshes an already-open repository detail view after a catalog reload or install/remove.
pub(crate) fn sync_open_repo_detail(ctx: &BridgeContext) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let open_url = ui.global::<RepoDetail>().get_open_url().to_string();
    if open_url.is_empty() {
        return;
    }
    let meta = {
        let session = ctx.session.borrow();
        let name = session
            .repositories
            .iter()
            .find(|repo| repo.url == open_url)
            .map(|repo| repo.name.clone());
        name.map(|name| {
            let repo_hits: Vec<_> = session
                .available
                .iter()
                .filter(|hit| hit.repository_url == open_url)
                .cloned()
                .collect();
            (name, language_labels(&repo_hits))
        })
    };
    let Some((name, languages)) = meta else {
        close_repo_detail(ctx);
        return;
    };
    let detail = ui.global::<RepoDetail>();
    detail.set_name(name.into());
    detail.set_languages(string_model(languages));
    show_repo_detail(ctx);
}

/// Filters, sorts, and displays the plugins of the currently opened repository.
pub(crate) fn show_repo_detail(ctx: &BridgeContext) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let detail = ui.global::<RepoDetail>();
    let open_url = detail.get_open_url().to_string();
    if open_url.is_empty() {
        return;
    }
    let query = detail_query(&detail, &open_url);
    let session = ctx.session.borrow();
    let total_in_repo = session
        .available
        .iter()
        .filter(|hit| hit.repository_url == open_url)
        .count();
    let installed_in_repo = session
        .available
        .iter()
        .filter(|hit| {
            hit.repository_url == open_url
                && session
                    .installed_times
                    .contains_key(&hit.plugin.internal_name)
        })
        .count();
    let found = filter_plugins(&session.available, &session.installed_times, &query);
    let page = session.repo_detail_page.max(PAGE);
    let rows: Vec<_> = found
        .iter()
        .take(page)
        .map(|hit| available_row(hit, &session.installed_times, false))
        .collect();
    detail.set_summary(repo_summary(total_in_repo, installed_in_repo).into());
    detail.set_has_more(found.len() > rows.len());
    detail.set_count_text(if total_in_repo == 0 {
        String::new().into()
    } else {
        format!("{} of {}", found.len(), total_in_repo).into()
    });
    detail.set_plugins(ModelRc::new(VecModel::from(rows)));
}

/// Closes the per-repository detail view and returns to the Repositories list.
pub(crate) fn close_repo_detail(ctx: &BridgeContext) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let detail = ui.global::<RepoDetail>();
    detail.set_open_url("".into());
    detail.set_name("".into());
    detail.set_summary("".into());
    detail.set_count_text("".into());
    detail.set_has_more(false);
    detail.set_plugins(ModelRc::new(VecModel::from(Vec::new())));
}

fn detail_query(detail: &RepoDetail<'_>, open_url: &str) -> PluginQuery {
    let language = detail.get_language();
    PluginQuery {
        text: detail.get_search().to_string(),
        language: if language == ALL_LANGUAGES {
            String::new()
        } else {
            language.to_string()
        },
        repository_url: open_url.to_owned(),
        view: match detail.get_view() {
            1 => PluginView::NotInstalled,
            2 => PluginView::Installed,
            _ => PluginView::All,
        },
        sort: sort_of(&detail.get_sort()),
    }
}

fn language_labels(hits: &[app_services::PluginHit]) -> Vec<String> {
    let mut labels = vec![ALL_LANGUAGES.to_owned()];
    labels.extend(language_choices(hits));
    labels
}

fn plugin_counts_by_repo(hits: &[app_services::PluginHit]) -> HashMap<&str, usize> {
    let mut counts = HashMap::new();
    for hit in hits {
        *counts.entry(hit.repository_url.as_str()).or_insert(0) += 1;
    }
    counts
}

fn plugin_count_badge(count: Option<usize>, loading: bool) -> String {
    if loading {
        return String::new();
    }
    let count = count.unwrap_or(0);
    if count == 1 {
        "1 plugin".to_owned()
    } else {
        format!("{count} plugins")
    }
}

fn repo_count_text(shown: usize, total: usize, filtered: bool) -> String {
    if total == 0 {
        return String::new();
    }
    if filtered {
        format!("{shown} of {total}")
    } else if total == 1 {
        "1 repository".to_owned()
    } else {
        format!("{total} repositories")
    }
}

fn repo_summary(total: usize, installed: usize) -> String {
    let noun = if total == 1 { "plugin" } else { "plugins" };
    if installed > 0 {
        format!("{total} {noun} · {installed} installed")
    } else {
        format!("{total} {noun}")
    }
}

/// The repository sort named by a drop-down label.
pub(crate) fn repo_sort_of(label: &str) -> RepositorySort {
    match label {
        "Sort: Name" => RepositorySort::Name,
        _ => RepositorySort::DateAdded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_sort_of_parses_dropdown_labels() {
        assert_eq!(repo_sort_of("Sort: Date added"), RepositorySort::DateAdded);
        assert_eq!(repo_sort_of("Sort: Name"), RepositorySort::Name);
        assert_eq!(repo_sort_of("anything else"), RepositorySort::DateAdded);
    }

    #[test]
    fn repo_count_and_summary_text_format_cleanly() {
        assert_eq!(repo_count_text(0, 0, false), "");
        assert_eq!(repo_count_text(1, 1, false), "1 repository");
        assert_eq!(repo_count_text(4, 4, false), "4 repositories");
        assert_eq!(repo_count_text(2, 4, true), "2 of 4");
        assert_eq!(repo_summary(1, 0), "1 plugin");
        assert_eq!(repo_summary(12, 3), "12 plugins · 3 installed");
    }
}
