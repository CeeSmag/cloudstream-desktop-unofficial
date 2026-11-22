use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::*;

#[test]
fn the_first_request_of_an_origin_owns_the_prompt_and_the_rest_are_quiet() {
    let mut owners = PromptOwners::default();
    assert_eq!(owners.join("https://a.test", "a1"), Role::Owner);
    assert_eq!(owners.join("https://a.test", "a2"), Role::Quiet);
    assert_eq!(
        owners.join("https://b.test", "b1"),
        Role::Owner,
        "origins are independent"
    );
    assert!(owners.is_quiet("a2") && !owners.is_quiet("a1") && !owners.is_quiet("b1"));
    assert!(owners.has_owner("https://a.test"));
}

#[test]
fn a_finished_group_never_promotes_its_followers_and_a_new_request_starts_a_new_group() {
    let mut owners = PromptOwners::default();
    owners.join("https://a.test", "a1");
    owners.join("https://a.test", "a2");
    owners.finish("a1");
    assert!(
        owners.is_quiet("a2"),
        "a follower stays quiet for its own answer"
    );
    assert_eq!(owners.join("https://a.test", "a3"), Role::Owner);
    owners.finish("a2");
    assert!(!owners.is_quiet("a2"));
    assert!(
        owners.has_owner("https://a.test"),
        "the new group is untouched"
    );
}

#[test]
fn a_withdrawn_owner_hands_the_prompt_to_the_oldest_follower() {
    let mut owners = PromptOwners::default();
    owners.join("https://a.test", "a1");
    owners.join("https://a.test", "a2");
    owners.join("https://a.test", "a3");
    assert_eq!(
        owners.withdraw("a2"),
        None,
        "a follower leaving changes nothing for the prompt"
    );
    assert_eq!(owners.withdraw("a1"), Some("a3".to_owned()));
    assert!(!owners.is_quiet("a3"), "the new owner speaks");
    assert_eq!(owners.withdraw("a3"), None);
    assert!(!owners.has_owner("https://a.test"));
    assert_eq!(owners.withdraw("never-seen"), None);
}

#[test]
fn dismissing_the_owner_dismisses_the_group_and_leaves_the_followers_to_the_caller() {
    let mut owners = PromptOwners::default();
    owners.join("https://a.test", "a1");
    owners.join("https://a.test", "a2");
    owners.join("https://a.test", "a3");
    assert_eq!(
        owners.dissolve("a2"),
        Vec::<String>::new(),
        "only the owner dismisses the group"
    );
    assert_eq!(owners.dissolve("a1"), ["a2", "a3"]);
    assert!(
        owners.is_quiet("a2"),
        "still quiet while the caller retires them"
    );
    assert!(!owners.has_owner("https://a.test"));
}

#[derive(Clone, Copy)]
enum Op {
    Join(u64),
    Finish(u64),
    Withdraw(u64),
}

const WORKERS: u64 = 4;
const PER_WORKER: u64 = 1000;

fn origin_of(id: u64) -> String {
    format!("https://o{}.test", id % 3)
}

/// Every worker opens requests and ends them (even ids finish, odd ids are withdrawn), so the
/// consumer sees the workers' steps in whatever order the threads interleave.
fn feed() -> (mpsc::Receiver<Op>, Vec<thread::JoinHandle<()>>) {
    let (send, receive) = mpsc::channel();
    let workers = (0..WORKERS)
        .map(|worker| {
            let send = send.clone();
            thread::spawn(move || {
                for index in 0..PER_WORKER {
                    let id = worker * PER_WORKER + index;
                    send.send(Op::Join(id)).unwrap();
                    let end = if id.is_multiple_of(2) {
                        Op::Finish(id)
                    } else {
                        Op::Withdraw(id)
                    };
                    send.send(end).unwrap();
                }
            })
        })
        .collect();
    (receive, workers)
}

fn assert_one_speaker(owners: &PromptOwners, live: &HashMap<String, HashSet<String>>) {
    for ids in live.values() {
        let speakers = ids.iter().filter(|id| !owners.is_quiet(id)).count();
        assert!(speakers <= 1, "two requests speak for one origin: {ids:?}");
    }
}

/// Several threads open and close requests on three origins while one consumer applies them in the
/// order they arrive. After every step each origin has at most one request that speaks for it, and a
/// request that has its final answer is never promoted.
#[test]
fn reordered_requests_never_give_an_origin_two_speakers() {
    let (receive, workers) = feed();
    let mut owners = PromptOwners::default();
    let mut live: HashMap<String, HashSet<String>> = HashMap::new();
    let mut ended: HashSet<String> = HashSet::new();
    while let Ok(op) = receive.recv_timeout(Duration::from_secs(2)) {
        let (id, promoted) = match op {
            Op::Join(id) => {
                owners.join(&origin_of(id), &id.to_string());
                live.entry(origin_of(id))
                    .or_default()
                    .insert(id.to_string());
                (id, None)
            }
            Op::Finish(id) => {
                owners.finish(&id.to_string());
                (id, None)
            }
            Op::Withdraw(id) => (id, owners.withdraw(&id.to_string())),
        };
        if !matches!(op, Op::Join(_)) {
            ended.insert(id.to_string());
            live.values_mut().for_each(|ids| {
                ids.remove(&id.to_string());
            });
        }
        assert!(
            promoted.is_none_or(|next| !ended.contains(&next)),
            "a finished request is never promoted"
        );
        assert_one_speaker(&owners, &live);
    }
    for worker in workers {
        worker.join().unwrap();
    }
}
