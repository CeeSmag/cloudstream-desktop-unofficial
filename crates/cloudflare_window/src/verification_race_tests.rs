//! T2. Drive the event-loop decision with concurrent browser callbacks.

use std::collections::HashSet;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::*;

const WORKERS: u64 = 4;
const ATTEMPTS: u64 = 1000;

#[test]
fn reordered_browser_callbacks_only_complete_their_current_document() {
    let url = Url::parse("https://fixture.test/path").unwrap();
    let (send, receive) = mpsc::channel();
    let mut workers = Vec::new();
    for worker in 0..WORKERS {
        let send = send.clone();
        workers.push(thread::spawn(move || {
            for index in 0..ATTEMPTS {
                let request = worker * ATTEMPTS + index;
                // Each attempt reports a response from a replaced loader before its own response.
                send.send((request, DocumentId(request * 2))).unwrap();
                send.send((request, DocumentId(request * 2 + 1))).unwrap();
            }
        }));
    }
    drop(send);
    let response = MainResponse::from_headers(url.clone(), 200, |_| None);
    let mut checks: Vec<_> = (0..WORKERS * ATTEMPTS)
        .map(|request| {
            let mut verification = Verification::new(url.clone(), Vec::new());
            verification.start(DocumentId(request * 2 + 1));
            verification
        })
        .collect();
    let mut completed = HashSet::new();
    for _ in 0..WORKERS * ATTEMPTS * 2 {
        let (request, document) = receive.recv_timeout(Duration::from_secs(1)).unwrap();
        let cookies = vec![("cf_clearance".to_owned(), request.to_string())];
        let decision = checks[request as usize].response(document, &response, cookies);
        if let Decision::Passed(cookies) = decision {
            assert_eq!(document, DocumentId(request * 2 + 1));
            assert_eq!(cookies[0].1, request.to_string());
            assert!(completed.insert(request));
        }
    }
    assert_eq!(completed.len() as u64, WORKERS * ATTEMPTS);
    for worker in workers {
        worker.join().unwrap();
    }
}
