//! `cloudstream-verify`: the Cloudflare verification helper started by CloudStream.
//!
//! Usage: `cloudstream-verify --exchange <dir> --profile <dir> [--max-windows <n>]`. It reads
//! commands on stdin (`cloudflare_bridge::HelperCommand`), prints statuses on stdout, and exits
//! when stdin closes.
#![deny(unsafe_code)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod browser_view;
mod context_reaper;
mod helper_app;
mod helper_events;
mod input_threads;
mod outbox;
mod page_fetches;
mod site_window;
mod verification;
mod waiters;
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
            tracing::error!("usage: cloudstream-verify --exchange <dir> --profile <dir> [--max-windows <n>] [--debug]");
            ExitCode::FAILURE
        }
    }
}

fn parse_args(args: impl Iterator<Item = String>) -> Option<(helper_app::HelperConfig, bool)> {
    let (mut exchange, mut profile, mut debug, mut max_windows) = (None, None, false, None);
    let mut args = args;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--exchange" => exchange = args.next().map(PathBuf::from),
            "--profile" => profile = args.next().map(PathBuf::from),
            "--debug" => debug = true,
            "--max-windows" => max_windows = Some(args.next()?.parse().ok()?),
            _ => return None,
        }
    }
    Some((
        helper_app::HelperConfig {
            exchange: exchange?,
            profile: profile?,
            max_windows: max_windows.unwrap_or(helper_app::DEFAULT_MAX_WINDOWS),
        },
        debug,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Option<(helper_app::HelperConfig, bool)> {
        parse_args(args.iter().map(|arg| (*arg).to_owned()))
    }

    #[test]
    fn parse_args_reads_the_window_limit() {
        let (config, _) = parse(&["--exchange", "e", "--profile", "p", "--max-windows", "2"])
            .expect("valid arguments");
        assert_eq!(config.max_windows, 2);
    }

    #[test]
    fn parse_args_without_a_window_limit_uses_the_default() {
        let (config, _) = parse(&["--exchange", "e", "--profile", "p"]).expect("valid");
        assert_eq!(config.max_windows, helper_app::DEFAULT_MAX_WINDOWS);
    }

    #[test]
    fn parse_args_refuses_a_window_limit_that_is_not_a_number() {
        assert!(parse(&["--exchange", "e", "--profile", "p", "--max-windows", "lots"]).is_none());
    }
}
