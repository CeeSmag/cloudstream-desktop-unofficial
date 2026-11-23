use std::collections::HashSet;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::*;

fn request(id: &str, url: &str) -> VerifyRequest {
    VerifyRequest {
        id: id.into(),
        host: "site.test".into(),
        url: url.into(),
        method: "GET".into(),
        standalone: false,
        reason: Default::default(),
    }
}

#[test]
fn only_requests_for_the_same_origin_that_are_not_retries_join() {
    let check = Waiters::new(request("owner", "https://site.test/a"));
    assert!(check.joins(&request("same", "https://site.test/other/path?x=1")));
    assert!(!check.joins(&request("scheme", "http://site.test/a")));
    assert!(!check.joins(&request("host", "https://cdn.site.test/a")));
    let mut retry = request("retry", "https://site.test/a");
    retry.standalone = true;
    assert!(!check.joins(&retry), "a retry asks for a fresh check");
    let standalone_owner = Waiters::new(retry);
    assert!(!standalone_owner.joins(&request("late", "https://site.test/a")));
}

#[test]
fn retiring_the_owner_hands_the_check_to_the_oldest_waiter_and_nobody_is_answered_twice() {
    let mut check = Waiters::new(request("owner", "https://site.test/a"));
    check.join(request("first", "https://site.test/b"));
    check.join(request("second", "https://site.test/c"));
    check.join(request("first", "https://site.test/b"));
    assert_eq!(
        check.retire("owner"),
        Retired::Owner(request("owner", "https://site.test/a"))
    );
    assert_eq!(check.owner().id, "first");
    assert_eq!(check.retire("owner"), Retired::Unknown, "already retired");
    let rest: Vec<_> = check.take_all().into_iter().map(|r| r.id).collect();
    assert_eq!(rest, ["first", "second"]);
}

#[test]
fn retiring_a_waiter_or_the_last_request_says_which() {
    let mut check = Waiters::new(request("owner", "https://site.test/a"));
    check.join(request("waiter", "https://site.test/b"));
    assert_eq!(
        check.retire("waiter"),
        Retired::Waiter(request("waiter", "https://site.test/b"))
    );
    assert_eq!(
        check.retire("owner"),
        Retired::Last(request("owner", "https://site.test/a"))
    );
}

enum Op {
    Join(u64),
    Retire(u64),
}

const WORKERS: u64 = 4;
const PER_WORKER: u64 = 1000;

/// Every worker joins requests and retires every third one, so the consumer sees the steps of the
/// workers interleaved in whatever order the threads run.
fn feed() -> (mpsc::Receiver<Op>, Vec<thread::JoinHandle<()>>) {
    let (send, receive) = mpsc::channel();
    let workers = (0..WORKERS)
        .map(|worker| {
            let send = send.clone();
            thread::spawn(move || {
                for index in 0..PER_WORKER {
                    let id = worker * PER_WORKER + index;
                    send.send(Op::Join(id)).unwrap();
                    if id.is_multiple_of(3) {
                        send.send(Op::Retire(id)).unwrap();
                    }
                }
            })
        })
        .collect();
    (receive, workers)
}

fn apply(check: &mut Waiters, op: Op, answered: &mut Vec<String>) {
    match op {
        Op::Join(id) => check.join(request(&id.to_string(), "https://site.test/p")),
        Op::Retire(id) => match check.retire(&id.to_string()) {
            Retired::Owner(old) | Retired::Waiter(old) | Retired::Last(old) => {
                answered.push(old.id)
            }
            Retired::Unknown => {}
        },
    }
}

/// Whatever the order the steps arrive in, every request is answered exactly once: either when it is
/// retired or when the check ends.
#[test]
fn reordered_joins_and_retirements_answer_every_request_exactly_once() {
    let (receive, workers) = feed();
    let mut check = Waiters::new(request("owner", "https://site.test/p"));
    let mut answered: Vec<String> = Vec::new();
    while let Ok(op) = receive.recv_timeout(Duration::from_secs(2)) {
        apply(&mut check, op, &mut answered);
    }
    answered.extend(check.take_all().into_iter().map(|r| r.id));
    for worker in workers {
        worker.join().unwrap();
    }
    let unique: HashSet<_> = answered.iter().cloned().collect();
    assert_eq!(unique.len(), answered.len(), "nobody is answered twice");
    assert_eq!(
        answered.len() as u64,
        WORKERS * PER_WORKER + 1,
        "nobody is left out"
    );
}
