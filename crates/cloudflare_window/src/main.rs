//! `cloudstream-verify`: the Cloudflare verification helper started by CloudStream.
//!
//! Usage: `cloudstream-verify --exchange <dir> --profile <dir>`. It reads commands on stdin
//! (`cloudflare_bridge::HelperCommand`), prints statuses on stdout, and exits when stdin closes.
#![forbid(unsafe_code)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod browser_view;
mod helper_app;
mod helper_events;
mod input_threads;
mod outbox;
mod page_fetches;
mod site_window;

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::io::stdout)
        .with_ansi(false)
        .with_target(false)
        .finish();
    if tracing::subscriber::set_global_default(subscriber).is_err() {
        return ExitCode::FAILURE;
    }
    match parse_args(std::env::args().skip(1)) {
        Some(config) => helper_app::run(config),
        None => {
            tracing::error!("usage: cloudstream-verify --exchange <dir> --profile <dir>");
            ExitCode::FAILURE
        }
    }
}

fn parse_args(args: impl Iterator<Item = String>) -> Option<helper_app::HelperConfig> {
    let (mut exchange, mut profile) = (None, None);
    let mut args = args;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--exchange" => exchange = args.next().map(PathBuf::from),
            "--profile" => profile = args.next().map(PathBuf::from),
            _ => return None,
        }
    }
    Some(helper_app::HelperConfig {
        exchange: exchange?,
        profile: profile?,
    })
}
