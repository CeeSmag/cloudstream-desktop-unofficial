//! Browse fetch and Cloudflare drain helpers.

use std::cell::RefCell;

use crate::runner::{BrowseRow, EventPump, UiEvent};
use crate::CloudStream;

use super::session::{AppExtras, Session};

pub(crate) fn drain_cloudflare(
    ui: &CloudStream,
    extras: &RefCell<AppExtras>,
    session: &RefCell<Session>,
    pump: &EventPump,
) {
    let extras_guard = extras.borrow_mut();
    let Some(watch) = extras_guard.cloudflare.as_ref() else {
        return;
    };
    let mut pending = Vec::new();
    while let Ok(event) = watch.events.try_recv() {
        pending.push(event);
    }
    let data_dir = extras_guard.data_dir.clone();
    drop(extras_guard);
    for event in pending {
        match event {
            cloudflare_bridge::CloudflareEvent::Needed(request) => {
                let allowed = session.borrow().allowed_cf.contains(&request.host);
                if allowed {
                    let _ = open::that(&request.url);
                    if let Some(dir) = data_dir.as_ref() {
                        let dir = dir.join("sidecar/cloudflare");
                        let _ = cloudflare_bridge::write_result(
                            &dir,
                            &request.id,
                            &cloudflare_bridge::Outcome::Waiting(
                                "complete verification in browser".into(),
                            ),
                        );
                    }
                    ui.set_notice(
                        format!("Cloudflare: complete check for {}", request.host).into(),
                    );
                } else {
                    let _ = pump.sender().send(UiEvent::CloudflareNeeded {
                        host: request.host,
                        url: request.url,
                        id: request.id,
                    });
                }
            }
        }
    }
}

pub(crate) fn browse_tmdb(kind: &str) -> Result<Vec<BrowseRow>, String> {
    use app_services::{BrowseFilterRequest, BrowseService};
    let client = BrowseService::default();
    let request = BrowseFilterRequest {
        media_type: kind.to_owned(),
        year: None,
        sort_by: "popular".into(),
        rating: None,
        genres: Vec::new(),
        genre: None,
        page: 1,
    };
    let page = client.fetch_browse_page(&request)?;
    Ok(page
        .items
        .into_iter()
        .map(|item| BrowseRow {
            title: item.name,
            subtitle: item.media_type.clone(),
            year: item.year,
            poster: item.poster_url.unwrap_or_default(),
            url: item.id,
        })
        .collect())
}
