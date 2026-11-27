//! Unit and race tests for [`HostCircuit`] and [`host_of`].

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};

use super::*;

#[test]
fn host_of_extracts_lowercase_host_ignoring_port_path_and_query() {
    assert_eq!(
        host_of("https://RezeSubs.COM:443/subs/en.vtt?token=1#top").as_deref(),
        Some("rezesubs.com")
    );
    assert_eq!(
        host_of("http://user:pass@Sub.Example.org/a.srt").as_deref(),
        Some("sub.example.org")
    );
    assert_eq!(host_of("http://[::1]:8080/a.srt").as_deref(), Some("::1"));
    assert_eq!(host_of("not-a-url"), None);
    assert_eq!(host_of("https:///no-host.vtt"), None);
}

#[test]
fn host_circuit_trips_exactly_once_at_threshold() {
    let circuit = HostCircuit::with_threshold(3);
    let url = "https://rezesubs.com/subs/a.vtt";
    assert!(!circuit.is_tripped(url));
    assert_eq!(circuit.record_failure(url), None);
    assert!(!circuit.is_tripped(url));
    assert_eq!(circuit.record_failure(url), None);
    assert!(!circuit.is_tripped(url));
    assert_eq!(
        circuit
            .record_failure("https://REZESUBS.COM/subs/b.vtt")
            .as_deref(),
        Some("rezesubs.com")
    );
    assert!(circuit.is_tripped("https://rezesubs.com/subs/c.vtt"));
    // Subsequent failures do not re-announce the trip.
    assert_eq!(
        circuit.record_failure("https://rezesubs.com/subs/d.vtt"),
        None
    );
}

#[test]
fn host_circuit_success_resets_streak_and_isolates_other_hosts() {
    let circuit = HostCircuit::with_threshold(2);
    let flaky = "https://flaky.test/1.vtt";
    let healthy = "https://healthy.test/1.vtt";
    assert_eq!(circuit.record_failure(flaky), None);
    circuit.record_success(flaky);
    assert_eq!(circuit.record_failure(flaky), None);
    assert!(!circuit.is_tripped(flaky));
    assert_eq!(circuit.record_failure(flaky).as_deref(), Some("flaky.test"));
    assert!(circuit.is_tripped(flaky));
    assert!(!circuit.is_tripped(healthy));
}

#[test]
fn host_circuit_concurrent_failures_trip_once_and_keep_healthy_host_open() {
    const THREADS: usize = 8;
    const ITERATIONS: usize = 1_000;
    let circuit = Arc::new(HostCircuit::with_threshold(3));
    let barrier = Arc::new(Barrier::new(THREADS));
    let trips = Arc::new(AtomicUsize::new(0));

    let handles: Vec<_> = (0..THREADS)
        .map(|_| {
            let circuit = Arc::clone(&circuit);
            let barrier = Arc::clone(&barrier);
            let trips = Arc::clone(&trips);
            std::thread::spawn(move || {
                barrier.wait();
                for n in 0..ITERATIONS {
                    if circuit
                        .record_failure(&format!("https://bad.test/{n}.vtt"))
                        .is_some()
                    {
                        trips.fetch_add(1, Ordering::SeqCst);
                    }
                    circuit.record_success(&format!("https://good.test/{n}.vtt"));
                    assert!(!circuit.is_tripped("https://good.test/check.vtt"));
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("thread finished");
    }
    assert_eq!(trips.load(Ordering::SeqCst), 1, "bad host must trip once");
    assert!(circuit.is_tripped("https://bad.test/after.vtt"));
    assert!(!circuit.is_tripped("https://good.test/after.vtt"));
}

#[test]
fn host_circuit_ignores_malformed_urls_and_clamps_zero_threshold() {
    assert_eq!(host_of(""), None);
    assert_eq!(host_of("http://[unclosed/a.vtt"), None);
    assert_eq!(host_of("https://  /a.vtt"), None);

    let circuit = HostCircuit::with_threshold(0);
    assert!(!circuit.is_tripped("not-a-url"));
    circuit.record_success("not-a-url");
    assert_eq!(circuit.record_failure("not-a-url"), None);
    // Clamped threshold of 1 trips on the very first failure.
    assert_eq!(
        circuit.record_failure("https://once.test/a.vtt").as_deref(),
        Some("once.test")
    );
    assert!(circuit.is_tripped("https://once.test/b.vtt"));
    circuit.record_success("https://once.test/c.vtt");
    assert!(!circuit.is_tripped("https://once.test/d.vtt"));
}
