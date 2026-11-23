//! The behaviour every [`PassBook`] must have, written once and run against each implementation
//! (`pass_book_contract!` below, used for [`MemoryPassBook`] here and for the disk book in its tests).

use crate::pass_fixtures::{later, now, pass, Spec};
use crate::pass_ids::PassId;

use super::*;

fn ids(book: &dyn PassBook, at: SystemTime) -> Vec<String> {
    let mut found: Vec<String> = book
        .live_passes(at)
        .iter()
        .map(|pass| pass.id().as_str().to_owned())
        .collect();
    found.sort();
    found
}

fn id(text: &str) -> PassId {
    PassId::parse(text).expect("id")
}

pub(crate) fn save_then_find(book: &dyn PassBook) {
    assert!(ids(book, now()).is_empty());
    let outcome = book.save(pass("p1", "a"), now()).expect("save");
    assert_eq!(outcome, SaveOutcome::Saved { replaced: None });
    assert_eq!(ids(book, now()), ["p1"]);
    let found = book.live_passes(now());
    assert_eq!(found.first(), Some(&pass("p1", "a")), "kept whole");
}

pub(crate) fn one_pass_per_scope_newest_wins(book: &dyn PassBook) {
    book.save(pass("p1", "a"), now()).expect("save");
    let newer = Spec::new("p2", "b").obtained(later(5)).build();
    let outcome = book.save(newer, later(5)).expect("save");
    assert_eq!(
        outcome,
        SaveOutcome::Saved {
            replaced: Some(id("p1"))
        }
    );
    let other_scope = Spec::new("p3", "c").domain("www.site.test").build();
    book.save(other_scope, later(5)).expect("save");
    assert_eq!(ids(book, later(5)), ["p2", "p3"]);
    let older = Spec::new("p4", "d").obtained(now()).build();
    assert_eq!(
        book.save(older, later(6)).expect("save"),
        SaveOutcome::Outdated
    );
    assert_eq!(
        book.save(pass("p5", "b"), later(6)).expect("save"),
        SaveOutcome::AlreadyKept(id("p2"))
    );
}

pub(crate) fn a_rejected_pass_is_never_kept_again(book: &dyn PassBook) {
    book.save(pass("p1", "a"), now()).expect("save");
    assert!(!book.reject(&id("missing"), now()).expect("reject"));
    assert!(book.reject(&id("p1"), now()).expect("reject"));
    assert!(ids(book, now()).is_empty());
    assert_eq!(
        book.save(pass("p2", "a"), later(1)).expect("save"),
        SaveOutcome::Rejected
    );
    assert!(ids(book, later(1)).is_empty());
    let report = book.prune(later(1)).expect("prune");
    assert_eq!(
        report,
        PruneReport::default(),
        "the rejection is still needed"
    );
    assert_eq!(
        book.save(pass("p3", "a"), later(2)).expect("save"),
        SaveOutcome::Rejected,
        "pruning keeps a live rejection"
    );
}

pub(crate) fn passes_end_and_are_pruned(book: &dyn PassBook) {
    book.save(Spec::new("p1", "a").lasts(600).build(), now())
        .expect("save");
    book.save(
        Spec::new("p2", "b")
            .domain("www.site.test")
            .lasts(3_600)
            .build(),
        now(),
    )
    .expect("save");
    assert_eq!(ids(book, later(539)), ["p1", "p2"]);
    assert_eq!(ids(book, later(540)), ["p2"], "p1 entered its margin");
    let report = book.prune(later(540)).expect("prune");
    assert_eq!(
        report,
        PruneReport {
            emptied: 1,
            evicted: 0
        }
    );
    assert_eq!(
        book.prune(later(540)).expect("prune"),
        PruneReport::default()
    );
    assert_eq!(
        book.save(Spec::new("p3", "c").lasts(30).build(), now())
            .expect("save"),
        SaveOutcome::Stale
    );
}

pub(crate) fn the_cap_evicts_the_oldest(book: &dyn PassBook) {
    // Made with a cap of 2.
    let hosts = ["a.site.test", "b.site.test", "c.site.test"];
    for (index, host) in hosts.into_iter().enumerate() {
        let offset = u64::try_from(index).expect("small");
        let made = Spec::new(&format!("p{index}"), &format!("v{index}"))
            .host(host)
            .domain(host)
            .obtained(later(offset))
            .build();
        book.save(made, later(offset)).expect("save");
    }
    assert_eq!(ids(book, later(3)), ["p1", "p2"]);
}

pub(crate) fn prune_evicts_over_the_cap(book: &dyn PassBook) {
    // Made with a cap of 2; the rejection list of a.site.test counts but is never evicted.
    let made = |index: u64, host: &'static str| {
        Spec::new(&format!("p{index}"), &format!("v{index}"))
            .host(host)
            .domain(host)
            .obtained(later(index))
            .build()
    };
    book.save(made(0, "a.site.test"), later(0)).expect("save");
    book.reject(&id("p0"), later(0)).expect("reject");
    book.save(made(1, "b.site.test"), later(1)).expect("save");
    assert_eq!(ids(book, later(1)), ["p1"]);
    book.save(made(2, "c.site.test"), later(2)).expect("save");
    assert_eq!(ids(book, later(2)), ["p2"], "the oldest pass made room");
    assert_eq!(book.prune(later(2)).expect("prune"), PruneReport::default());
}

/// Generates one test per contract case for a book made by `$make` (a `Fn(usize) -> impl PassBook`
/// taking the cap and a name for the case).
macro_rules! pass_book_contract {
    ($make:expr) => {
        pass_book_contract!(@case $make, save_then_find, 8);
        pass_book_contract!(@case $make, one_pass_per_scope_newest_wins, 8);
        pass_book_contract!(@case $make, a_rejected_pass_is_never_kept_again, 8);
        pass_book_contract!(@case $make, passes_end_and_are_pruned, 8);
        pass_book_contract!(@case $make, the_cap_evicts_the_oldest, 2);
        pass_book_contract!(@case $make, prune_evicts_over_the_cap, 2);
    };
    (@case $make:expr, $case:ident, $cap:expr) => {
        #[test]
        fn $case() {
            let book = ($make)($cap, stringify!($case));
            $crate::pass_book::contract_tests::$case(&book);
        }
    };
}

pub(crate) use pass_book_contract;

mod memory {
    use super::super::MemoryPassBook;

    pass_book_contract!(|cap, _name| MemoryPassBook::with_cap(cap));

    #[test]
    fn default_holds_the_standard_cap() {
        let book = MemoryPassBook::default();
        assert_eq!(book.cap, super::super::MAX_PASSES);
    }
}
