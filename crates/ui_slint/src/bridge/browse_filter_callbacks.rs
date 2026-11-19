//! Browse's filter row: year, popular or best rated, genres, and pagination.

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::browse_filters::{
    rating_from_label, year_from_label, year_labels, year_of_unix_ms, GENRES, SORT_LABELS,
};
use crate::posters::string_model;
use crate::{BrowseFilter, CloudStream, FilterChip};

use super::bridge_context::BridgeContext;
use super::browse_loading::fetch_browse;

pub(crate) fn wire_browse_filters(ctx: &BridgeContext, ui: &CloudStream) {
    let filter = ui.global::<BrowseFilter>();
    let year = year_of_unix_ms(ctx.services.clock.now_ms());
    filter.set_years(string_model(year_labels(year)));
    filter.set_sorts(string_model(
        SORT_LABELS.iter().map(|label| (*label).to_owned()),
    ));
    show_genres(ctx, ui);
    let c = ctx.clone();
    filter.on_changed(move || {
        let Some(ui) = c.window() else {
            return;
        };
        let filter = ui.global::<BrowseFilter>();
        {
            let mut session = c.session.borrow_mut();
            session.browse_filters.year = year_from_label(&filter.get_year());
            session.browse_filters.min_rating = rating_from_label(&filter.get_sort());
        }
        fetch_browse(&c, 1);
    });
    let c = ctx.clone();
    filter.on_toggle_genre(move |at| {
        let genre = usize::try_from(at).ok().and_then(|at| GENRES.get(at));
        let (Some(genre), Some(ui)) = (genre, c.window()) else {
            return;
        };
        c.session.borrow_mut().browse_filters.toggle_genre(genre);
        show_genres(&c, &ui);
        fetch_browse(&c, 1);
    });
    let c = ctx.clone();
    filter.on_go_to_page(move |target| go_to_browse_page(&c, target));
}

fn go_to_browse_page(ctx: &BridgeContext, target: i32) {
    let Some(ui) = ctx.window() else {
        return;
    };
    if ui.get_browse_loading() {
        return;
    }
    let Ok(page) = u32::try_from(target) else {
        return;
    };
    let (current, total) = ctx.session.borrow().browse_page;
    if page >= 1 && page <= total.max(1) && page != current {
        fetch_browse(ctx, page);
    }
}

fn show_genres(ctx: &BridgeContext, ui: &CloudStream) {
    let chosen = ctx.session.borrow().browse_filters.genres.clone();
    let chips: Vec<FilterChip> = GENRES
        .iter()
        .map(|genre| FilterChip {
            label: (*genre).into(),
            count: "".into(),
            on: chosen.iter().any(|name| name == genre),
        })
        .collect();
    ui.global::<BrowseFilter>()
        .set_genres(ModelRc::new(VecModel::from(chips)));
}
