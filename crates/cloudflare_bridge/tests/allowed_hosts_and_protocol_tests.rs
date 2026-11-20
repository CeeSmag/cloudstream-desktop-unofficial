//! Unit and concurrent race tests for `cloudflare_bridge` allowed hosts and protocol codecs.

use std::sync::Arc;
use std::thread;

use cloudflare_bridge::{
    format_command, format_fetch_result, format_result, format_status, parse_command,
    parse_fetch_request, parse_request, parse_status, AllowedHosts, FetchOutcome, HelperCommand,
    HelperStatus, Outcome, VerifyRequest,
};

#[test]
fn allowed_hosts_handles_normalization_and_concurrent_updates() {
    let hosts = Arc::new(AllowedHosts::default());
    let mut handles = Vec::new();
    for t in 0..8usize {
        let h = Arc::clone(&hosts);
        handles.push(thread::spawn(move || {
            for i in 0..300usize {
                let host = format!("SITE-{}.Example.COM", (t + i) % 16);
                h.allow(&host);
                assert!(h.is_allowed(&host.to_ascii_lowercase()));
            }
        }));
    }
    for handle in handles {
        handle.join().expect("thread finished");
    }
    assert!(hosts.is_allowed("site-0.example.com"));
    assert!(!hosts.is_allowed("never-added.example.com"));
}

#[test]
fn protocol_commands_statuses_and_exchange_formats_round_trip() {
    let req_val = VerifyRequest {
        id: "req-1".into(),
        host: "site.example".into(),
        url: "https://site.example/page".into(),
    };
    let cmd = HelperCommand::Verify(req_val.clone());
    let line = format_command(&cmd);
    assert_eq!(parse_command(line.trim()), Some(cmd));
    assert_eq!(
        parse_command(format_command(&HelperCommand::Quit).trim()),
        Some(HelperCommand::Quit)
    );

    let status = HelperStatus::Passed {
        host: "site.example".into(),
    };
    let sline = format_status(&status);
    assert_eq!(parse_status(sline.trim()), Some(status));

    let parsed = parse_request("id=req-2\nhost=site.example\nurl=https://site.example/video\n");
    assert_eq!(
        parsed,
        Some(VerifyRequest {
            id: "req-2".into(),
            host: "site.example".into(),
            url: "https://site.example/video".into(),
        })
    );
    assert!(parse_request("id=bad\nhost=site.example\nurl=ftp://not-http.example\n").is_none());

    let res_text = format_result(&Outcome::Declined("cancelled".into()));
    assert!(res_text.contains("cancelled"));
    let wait_text = format_result(&Outcome::Waiting("waiting".into()));
    assert!(wait_text.contains("waiting"));

    let fetch_res = format_fetch_result(&FetchOutcome::Failed {
        reason: "timeout".into(),
        gone: false,
    });
    assert!(fetch_res.contains("timeout"));

    assert!(parse_fetch_request("not-a-valid-fetch").is_none());
}
