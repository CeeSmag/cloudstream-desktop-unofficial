//! The player's Sources menu on screen: its rows, its filters and their toggles.

use app_services::{stream_kind, LinkHealth};
use core_model::PlayableLink;
use slint::{ModelRc, SharedString, VecModel};

use crate::source_menu::{abbreviation, source_menu, SourceList, SourceMenu, Tier};
use crate::{CloudStream, FilterChip, SourceItem, SourceMenuInfo};

use super::bridge_context::BridgeContext;

/// The resolution toggles, in the order the panel shows them.
const TIERS: [(Tier, &str); 2] = [(Tier::Hd, "1080p"), (Tier::Uhd, "2160p")];

pub(crate) fn wire_source_picker(ctx: &BridgeContext, ui: &CloudStream) {
    let c = ctx.clone();
    ui.on_toggle_source_tier(move |at| {
        let tier = usize::try_from(at)
            .ok()
            .and_then(|at| TIERS.get(at))
            .map(|(tier, _)| *tier);
        if let Some(tier) = tier {
            c.session.borrow_mut().source_filter.toggle_tier(tier);
            show_sources(&c);
        }
    });
    let c = ctx.clone();
    ui.on_toggle_source_subtitle(move |at| {
        let language = usize::try_from(at)
            .ok()
            .and_then(|at| preferred_subtitles(&c).get(at).copied());
        if let Some(language) = language {
            c.session
                .borrow_mut()
                .source_filter
                .toggle_subtitle(language.name);
            show_sources(&c);
        }
    });
}

/// Rebuilds the menu from the open video's sources and the viewer's filters.
pub(crate) fn show_sources(ctx: &BridgeContext) {
    let Some(ui) = ctx.window() else {
        return;
    };
    let preferred = preferred_subtitles(ctx);
    let session = ctx.session.borrow();
    let open = session.live.snapshot();
    let list = SourceList {
        links: &open.links,
        subtitles: &open.subtitles,
        selected: session.selected,
        preferred: &preferred,
    };
    let menu = source_menu(&list, &session.source_filter);
    let badge = active_source_badge(&open.links, session.selected);
    let filter = session.source_filter.clone();
    drop(session);
    ui.set_player_source_badge(badge.into());
    ui.set_sources(ModelRc::new(VecModel::from(
        menu.rows.iter().map(source_item).collect::<Vec<_>>(),
    )));
    let subtitles = preferred
        .iter()
        .map(|language| FilterChip {
            label: abbreviation(language.name).into(),
            count: SharedString::new(),
            on: filter.subtitles.iter().any(|name| name == language.name),
        })
        .collect();
    ui.set_source_info(menu_info(&menu, &filter.tiers, subtitles));
}

fn menu_info(menu: &SourceMenu, toggled: &[Tier], subtitles: Vec<FilterChip>) -> SourceMenuInfo {
    let tiers = TIERS
        .iter()
        .map(|(tier, label)| FilterChip {
            label: (*label).into(),
            count: match tier {
                Tier::Hd => menu.hd_count,
                Tier::Uhd => menu.uhd_count,
            }
            .to_string()
            .into(),
            on: toggled.contains(tier),
        })
        .collect::<Vec<_>>();
    SourceMenuInfo {
        total: count(menu.total),
        resolution_shown: count(menu.resolution_shown),
        subtitle_shown: count(menu.subtitle_shown),
        tiers: ModelRc::new(VecModel::from(tiers)),
        subtitles: ModelRc::new(VecModel::from(subtitles)),
    }
}

fn source_item(row: &crate::source_menu::SourceRow) -> SourceItem {
    SourceItem {
        index: count(row.index),
        label: row.label.clone().into(),
        kind: row.kind.clone().into(),
        size: row.size.clone().into(),
        flags: ModelRc::new(VecModel::from(
            row.flags.iter().map(SharedString::from).collect::<Vec<_>>(),
        )),
        checking: row.checking,
        working: row.working,
        selected: row.selected,
    }
}

fn count(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// The viewer's preferred subtitle languages (the subtitle filter's toggles), in their order.
fn preferred_subtitles(ctx: &BridgeContext) -> Vec<player_api::Language> {
    ctx.services
        .settings
        .preferred_subtitles()
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "cannot read the preferred subtitle languages");
            Vec::new()
        })
}

/// Compact quality/kind pill for the player header (e.g. `"1080p · HLS"`, `"HLS"`).
pub(crate) fn active_source_badge(
    links: &[(PlayableLink, Option<LinkHealth>)],
    selected: usize,
) -> String {
    let Some((link, _)) = links.get(selected) else {
        return String::new();
    };
    let kind = stream_kind(link);
    match (link.quality > 0, kind.is_empty()) {
        (true, false) => format!("{}p · {kind}", link.quality),
        (true, true) => format!("{}p", link.quality),
        (false, _) => kind,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn sample_link(quality: i32, link_type: &str) -> (PlayableLink, Option<LinkHealth>) {
        (
            PlayableLink {
                source: "Test".into(),
                name: "Test".into(),
                url: "https://example.invalid/video.m3u8".into(),
                referer: String::new(),
                quality,
                headers: HashMap::new(),
                link_type: link_type.into(),
                kid: None,
                key: None,
            },
            None,
        )
    }

    #[test]
    fn active_source_badge_formats_quality_and_kind() {
        let links = [sample_link(1080, "M3U8"), sample_link(0, "M3U8")];
        assert_eq!(active_source_badge(&links, 0), "1080p · HLS");
        assert_eq!(active_source_badge(&links, 1), "HLS");
        assert_eq!(active_source_badge(&links, 9), "");
    }
}
