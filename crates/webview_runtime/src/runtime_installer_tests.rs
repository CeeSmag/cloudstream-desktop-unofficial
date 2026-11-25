//! T2 and race tests for [`RuntimeInstaller`], with fake download, signature, setup and registry.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Barrier, Mutex};
use std::thread;
use std::time::Duration;

use super::*;
use crate::RuntimeFacts;

const MICROSOFT: &str = "Valid|CN=Microsoft Corporation, O=Microsoft Corporation, C=US";

fn missing() -> RuntimeFacts {
    RuntimeFacts::Windows {
        major: Some(10),
        versions: Vec::new(),
    }
}

fn installed() -> RuntimeFacts {
    RuntimeFacts::Windows {
        major: Some(10),
        versions: vec!["131.0.2903.70".into()],
    }
}

/// Writes a few bytes in two steps, or fails; optionally meets the test on barriers mid-way.
#[derive(Default)]
struct FakeFetcher {
    fail: Option<SetupError>,
    /// Tells the test the download is half-way, then waits for it to let the download finish.
    pause: Option<(Sender<()>, Arc<Barrier>)>,
    calls: AtomicUsize,
}

impl SetupFetcher for FakeFetcher {
    fn fetch(
        &self,
        request: FetchRequest<'_>,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<(), SetupError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(error) = &self.fail {
            return Err(error.clone());
        }
        progress(4, 8);
        if let Some((halfway, resume)) = &self.pause {
            let _ = halfway.send(());
            resume.wait();
        }
        std::fs::write(request.dest, b"MZsetup")
            .map_err(|e| SetupError::disk("write", request.dest, &e))?;
        progress(8, 8);
        Ok(())
    }
}

struct FakeVerifier {
    answer: &'static str,
    calls: AtomicUsize,
}

impl SignatureVerifier for FakeVerifier {
    fn describe(&self, file: &Path) -> Result<String, SetupError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(file.is_file(), "the setup is checked only once downloaded");
        Ok(self.answer.to_owned())
    }
}

/// The registry, which the fake setup changes.
struct FakeRegistry(Mutex<RuntimeFacts>);

impl RuntimeProbe for FakeRegistry {
    fn facts(&self) -> RuntimeFacts {
        self.0.lock().unwrap().clone()
    }
}

struct FakeSetup {
    code: i32,
    installs: bool,
    registry: Arc<FakeRegistry>,
    cancel_while_running: Option<CancelFlag>,
    calls: AtomicUsize,
}

impl SetupRunner for FakeSetup {
    fn run(&self, file: &Path) -> Result<i32, SetupError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(std::fs::read(file).unwrap(), b"MZsetup");
        if let Some(cancel) = &self.cancel_while_running {
            cancel.cancel();
        }
        if self.installs {
            *self.registry.0.lock().unwrap() = installed();
        }
        Ok(self.code)
    }
}

struct Rig {
    fetcher: Arc<FakeFetcher>,
    verifier: Arc<FakeVerifier>,
    setup: Arc<FakeSetup>,
    data: PathBuf,
}

impl Rig {
    fn new(name: &str, fetcher: FakeFetcher, answer: &'static str, setup: (i32, bool)) -> Self {
        let data =
            std::env::temp_dir().join(format!("cs_wv2_install_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let registry = Arc::new(FakeRegistry(Mutex::new(missing())));
        Self {
            fetcher: Arc::new(fetcher),
            verifier: Arc::new(FakeVerifier {
                answer,
                calls: AtomicUsize::new(0),
            }),
            setup: Arc::new(FakeSetup {
                code: setup.0,
                installs: setup.1,
                registry,
                cancel_while_running: None,
                calls: AtomicUsize::new(0),
            }),
            data,
        }
    }

    fn installer(&self) -> RuntimeInstaller {
        let parts = InstallerParts {
            fetcher: self.fetcher.clone(),
            verifier: self.verifier.clone(),
            runner: self.setup.clone(),
            probe: self.setup.registry.clone(),
        };
        RuntimeInstaller::new(parts, self.data.clone())
    }

    fn run(&self, cancel: &CancelFlag) -> (Result<RuntimeStatus, SetupError>, Vec<InstallPhase>) {
        let mut phases = Vec::new();
        let result = self
            .installer()
            .install(cancel, &mut |phase| phases.push(phase));
        (result, phases)
    }

    fn calls(&self) -> (usize, usize, usize) {
        (
            self.fetcher.calls.load(Ordering::SeqCst),
            self.verifier.calls.load(Ordering::SeqCst),
            self.setup.calls.load(Ordering::SeqCst),
        )
    }

    /// No attempt folder may be left behind, whatever happened.
    fn assert_clean(&self) {
        let left = std::fs::read_dir(self.data.join(crate::SETUP_FOLDER))
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert_eq!(left, 0, "attempt folders left in {}", self.data.display());
        let _ = std::fs::remove_dir_all(&self.data);
    }
}

#[test]
fn a_good_setup_installs_the_runtime_and_reports_each_phase() {
    let rig = Rig::new("good", FakeFetcher::default(), MICROSOFT, (0, true));
    let (result, phases) = rig.run(&CancelFlag::new());
    assert_eq!(
        result,
        Ok(RuntimeStatus::Installed {
            version: "131.0.2903.70".into()
        })
    );
    assert_eq!(
        phases,
        [
            InstallPhase::Downloading { done: 0, total: 0 },
            InstallPhase::Downloading { done: 4, total: 8 },
            InstallPhase::Downloading { done: 8, total: 8 },
            InstallPhase::Verifying,
            InstallPhase::Installing,
        ]
    );
    assert_eq!(rig.calls(), (1, 1, 1));
    rig.assert_clean();
}

#[test]
fn a_failed_download_stops_before_anything_runs() {
    let fetcher = FakeFetcher {
        fail: Some(SetupError::WrongHost("example.org".into())),
        ..FakeFetcher::default()
    };
    let rig = Rig::new("download", fetcher, MICROSOFT, (0, true));
    let (result, phases) = rig.run(&CancelFlag::new());
    assert_eq!(result, Err(SetupError::WrongHost("example.org".into())));
    assert_eq!(phases, [InstallPhase::Downloading { done: 0, total: 0 }]);
    assert_eq!(rig.calls(), (1, 0, 0));
    rig.assert_clean();
}

#[test]
fn a_setup_not_signed_by_microsoft_is_never_run() {
    let rig = Rig::new("unsigned", FakeFetcher::default(), "NotSigned|", (0, true));
    let (result, phases) = rig.run(&CancelFlag::new());
    assert!(
        matches!(result, Err(SetupError::NotMicrosoft(_))),
        "{result:?}"
    );
    assert_eq!(phases.last(), Some(&InstallPhase::Verifying));
    assert_eq!(rig.calls(), (1, 1, 0));
    rig.assert_clean();
}

#[test]
fn a_failing_setup_reports_its_code() {
    let rig = Rig::new("code", FakeFetcher::default(), MICROSOFT, (5, false));
    assert_eq!(
        rig.run(&CancelFlag::new()).0,
        Err(SetupError::InstallerFailed(5))
    );
    rig.assert_clean();
}

#[test]
fn a_setup_that_says_0_but_installs_nothing_is_still_missing() {
    let rig = Rig::new("nothing", FakeFetcher::default(), MICROSOFT, (0, false));
    assert_eq!(rig.run(&CancelFlag::new()).0, Err(SetupError::StillMissing));
    rig.assert_clean();
}

#[test]
fn the_registry_decides_even_when_the_setup_complains() {
    let rig = Rig::new(
        "already",
        FakeFetcher::default(),
        MICROSOFT,
        (-2_147_219_440, true),
    );
    assert!(matches!(
        rig.run(&CancelFlag::new()).0,
        Ok(RuntimeStatus::Installed { .. })
    ));
    rig.assert_clean();
}

#[test]
fn a_cancel_raised_before_the_install_stops_it_at_once() {
    let rig = Rig::new("early", FakeFetcher::default(), MICROSOFT, (0, true));
    let cancel = CancelFlag::new();
    cancel.cancel();
    assert_eq!(rig.run(&cancel).0, Err(SetupError::Cancelled));
    assert_eq!(rig.calls(), (1, 0, 0));
    rig.assert_clean();
}

#[test]
fn a_cancel_once_the_setup_runs_is_too_late_and_ignored() {
    let mut rig = Rig::new("late", FakeFetcher::default(), MICROSOFT, (0, true));
    let cancel = CancelFlag::new();
    Arc::get_mut(&mut rig.setup).unwrap().cancel_while_running = Some(cancel.clone());
    assert!(matches!(
        rig.run(&cancel).0,
        Ok(RuntimeStatus::Installed { .. })
    ));
    rig.assert_clean();
}

/// The viewer presses Close while the download is still writing (the fetcher itself ignores the
/// flag here, like a transfer that finishes its last chunk): the setup's signature must not even
/// be read, let alone the setup run. Barriers put the cancel exactly mid-download, 100 times.
#[test]
fn race_a_cancel_during_the_download_keeps_the_setup_from_running() {
    for round in 0..100 {
        let (halfway, reached) = mpsc::channel();
        let resume = Arc::new(Barrier::new(2));
        let fetcher = FakeFetcher {
            pause: Some((halfway, Arc::clone(&resume))),
            ..FakeFetcher::default()
        };
        let rig = Rig::new(&format!("race{round}"), fetcher, MICROSOFT, (0, true));
        let cancel = CancelFlag::new();
        let installer = rig.installer();
        let worker = {
            let cancel = cancel.clone();
            thread::spawn(move || installer.install(&cancel, &mut |_| {}))
        };
        // A download that never starts must fail the test, not hang it.
        reached
            .recv_timeout(Duration::from_secs(5))
            .expect("the download never started");
        cancel.cancel();
        resume.wait();
        assert_eq!(
            worker.join().unwrap(),
            Err(SetupError::Cancelled),
            "round {round}"
        );
        assert_eq!(rig.calls(), (1, 0, 0), "round {round}");
        rig.assert_clean();
    }
}

#[test]
fn outcomes_follow_the_registry_first() {
    let version = || RuntimeStatus::Installed {
        version: "1.0.0.1".into(),
    };
    assert_eq!(outcome(0, version()), Ok(version()));
    assert_eq!(outcome(9, version()), Ok(version()));
    assert_eq!(
        outcome(9, RuntimeStatus::NotNeeded),
        Ok(RuntimeStatus::NotNeeded)
    );
    assert_eq!(
        outcome(0, RuntimeStatus::Missing),
        Err(SetupError::StillMissing)
    );
    assert_eq!(
        outcome(9, RuntimeStatus::Missing),
        Err(SetupError::InstallerFailed(9))
    );
    assert_eq!(
        outcome(0, RuntimeStatus::Unsupported),
        Err(SetupError::StillMissing)
    );
    assert_eq!(
        outcome(-1, RuntimeStatus::Unsupported),
        Err(SetupError::InstallerFailed(-1))
    );
}
