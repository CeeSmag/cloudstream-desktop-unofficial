//! `cloudstream-verify`: the Cloudflare verification helper started by CloudStream.
//!
//! Usage: `cloudstream-verify --exchange <dir> --profile <dir>`. It reads commands on stdin
//! (`cloudflare_bridge::HelperCommand`), prints statuses on stdout, and exits when stdin closes.
#![deny(unsafe_code)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod browser_view;
mod helper_app;
mod helper_events;
mod input_threads;
mod outbox;
mod page_fetches;
mod site_window;
#[cfg(windows)]
mod windows_presentation;

use std::path::PathBuf;
use std::process::ExitCode;

use tracing::Level;

fn main() -> ExitCode {
    let parsed = parse_args(std::env::args().skip(1));
    let max_level = if parsed.as_ref().is_some_and(|(_, debug)| *debug) {
        Level::DEBUG
    } else {
        Level::INFO
    };
    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::io::stdout)
        .with_max_level(max_level)
        .with_ansi(false)
        .with_target(false)
        .finish();
    if tracing::subscriber::set_global_default(subscriber).is_err() {
        return ExitCode::FAILURE;
    }
    match parsed {
        Some((config, _)) => helper_app::run(config),
        None => {
            tracing::error!("usage: cloudstream-verify --exchange <dir> --profile <dir> [--debug]");
            ExitCode::FAILURE
        }
    }
}

fn parse_args(args: impl Iterator<Item = String>) -> Option<(helper_app::HelperConfig, bool)> {
    let (mut exchange, mut profile, mut debug) = (None, None, false);
    let mut args = args;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--exchange" => exchange = args.next().map(PathBuf::from),
            "--profile" => profile = args.next().map(PathBuf::from),
            "--debug" => debug = true,
            _ => return None,
        }
    }
    Some((
        helper_app::HelperConfig {
            exchange: exchange?,
            profile: profile?,
        },
        debug,
    ))
}
