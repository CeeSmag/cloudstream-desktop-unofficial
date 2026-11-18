//! Threads that turn stdin lines and new fetch files into events for the loop.

use std::collections::HashSet;
use std::io::BufRead;
use std::path::PathBuf;
use std::time::Duration;

use cloudflare_bridge::{parse_command, pending_fetches};
use tao::event_loop::EventLoopProxy;

use crate::helper_events::HelperEvent;

/// How often the exchange folder is looked at for browser fetches.
const FETCH_POLL: Duration = Duration::from_millis(150);

/// Starts the stdin reader and the fetch-file watcher.
pub(crate) fn start_input_threads(proxy: &EventLoopProxy<HelperEvent>, exchange: PathBuf) {
    let commands = proxy.clone();
    let reader = std::thread::Builder::new()
        .name("helper-stdin".into())
        .spawn(move || read_commands(&commands));
    let fetches = proxy.clone();
    let watcher = std::thread::Builder::new()
        .name("helper-fetches".into())
        .spawn(move || watch_fetches(&fetches, &exchange));
    if let Err(error) = reader.and(watcher) {
        tracing::error!(%error, "could not start the helper's input threads");
    }
}

fn read_commands(proxy: &EventLoopProxy<HelperEvent>) {
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else {
            break;
        };
        match parse_command(&line) {
            Some(command) => {
                if proxy.send_event(HelperEvent::Command(command)).is_err() {
                    return;
                }
            }
            None => tracing::warn!("ignored a line the helper does not understand"),
        }
    }
    let _closing = proxy.send_event(HelperEvent::InputClosed);
}

fn watch_fetches(proxy: &EventLoopProxy<HelperEvent>, exchange: &std::path::Path) {
    let mut seen = HashSet::new();
    loop {
        for request in pending_fetches(exchange, &mut seen) {
            if proxy.send_event(HelperEvent::Fetch(request)).is_err() {
                return;
            }
        }
        std::thread::sleep(FETCH_POLL);
    }
}
