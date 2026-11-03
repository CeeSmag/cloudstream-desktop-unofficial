//! What the discovery reports and leaves behind: subtitles delivered, failures counted, and the
//! video closed at any moment.

use super::*;

#[test]
fn subtitles_found_after_playback_started_are_downloaded_and_reach_the_player_as_local_files() {
    let given = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&given);
    let first = update(vec![link("https://a/ok", 720)], false);
    let late = LinkSearchUpdate {
        subtitles: vec![SubtitleTrack {
            lang: "French".to_owned(),
            url: "https://subs.test/fr.srt".to_owned(),
            headers: None,
        }],
        done: true,
        success: true,
        ..LinkSearchUpdate::default()
    };
    let give: SubtitleSink =
        Arc::new(move |subtitles| sink.lock().expect("lock").extend(subtitles.to_vec()));
    let (job, search, _, _, subtitles) = job_with_subtitles(vec![first, late], Vec::new(), give);
    job.session.mark_selected(0);

    job.run_search(search);

    let deadline = Instant::now() + Duration::from_secs(10);
    while given.lock().expect("lock").is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let given = given.lock().expect("lock");
    assert_eq!(given.len(), 1, "the subtitle never reached the player");
    assert_eq!(given[0].lang, "French");
    assert!(
        !given[0].url.starts_with("http") && std::path::Path::new(&given[0].url).exists(),
        "the player must get a local file, not {}",
        given[0].url
    );
    assert_eq!(
        subtitles
            .host
            .calls
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
}

#[test]
fn every_failed_check_is_counted_by_reason_and_host() {
    let script = vec![update(
        vec![
            link("https://dead.test/a", 720),
            link("https://dead.test/b", 480),
            link("https://alive.test/c", 1080),
        ],
        true,
    )];
    let (job, search, _, _) = job(script, vec!["https://alive.test/c"]);
    job.run_search(search);
    job.run_checks();
    assert_eq!(job.failures.total(), 2);
    assert!(job.failures.hosts().starts_with("dead.test=2"));
    assert!(job.failures.reasons().contains("cannot download <url>"));
}

/// The viewer closes the video at every point of its search and checks: both threads must stop, the
/// search must be told to stop at most once, and nothing may bring the closed video's list back.
#[test]
fn closing_at_any_moment_stops_the_search_and_the_checks_and_never_revives_the_session() {
    for delay_micros in [0u64, 20, 100, 400, 1_500, 6_000, 25_000, 90_000] {
        let script = (0..6)
            .map(|n| update(vec![link(&format!("https://h{n}.test/v"), 720)], n == 5))
            .collect();
        let (job, search, gateway, _) = job(script, vec!["https://h1.test/v"]);
        let job = Arc::new(job);
        let (searching, checking) = (Arc::clone(&job), Arc::clone(&job));
        let handles = vec![
            std::thread::spawn(move || searching.run_search(search)),
            std::thread::spawn(move || checking.run_checks()),
        ];

        std::thread::sleep(Duration::from_micros(delay_micros));
        job.session.clear();
        job.gate.cancel();

        crate::race_tests::join_within(handles, crate::race_tests::TIME_LIMIT);
        assert!(
            job.session.snapshot().is_none(),
            "closing after {delay_micros} us: the search brought the closed video's list back"
        );
        assert!(
            *gateway.cancelled.lock().expect("lock") <= 1,
            "closing after {delay_micros} us: the search was stopped more than once"
        );
    }
}
