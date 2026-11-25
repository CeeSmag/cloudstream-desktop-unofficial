//! Race test for the install worker: the gate must be open by the time the UI hears the end.

use std::path::{Path, PathBuf};

use webview_runtime::{
    FetchRequest, FixedProbe, InstallerParts, RuntimeFacts, SetupError, SetupFetcher, SetupRunner,
    SignatureVerifier,
};

use super::*;
use crate::background_work::EventPump;

struct FailingDownload;

impl SetupFetcher for FailingDownload {
    fn fetch(&self, _: FetchRequest<'_>, _: &mut dyn FnMut(u64, u64)) -> Result<(), SetupError> {
        Err(SetupError::Download("offline".into()))
    }
}

struct Unused;

impl SignatureVerifier for Unused {
    fn describe(&self, _: &Path) -> Result<String, SetupError> {
        Err(SetupError::StillMissing)
    }
}

impl SetupRunner for Unused {
    fn run(&self, _: &Path) -> Result<i32, SetupError> {
        Err(SetupError::StillMissing)
    }
}

fn missing() -> Arc<dyn RuntimeProbe> {
    Arc::new(FixedProbe(RuntimeFacts::Windows {
        major: Some(10),
        versions: Vec::new(),
    }))
}

fn installer(data: PathBuf) -> RuntimeInstaller {
    let parts = InstallerParts {
        fetcher: Arc::new(FailingDownload),
        verifier: Arc::new(Unused),
        runner: Arc::new(Unused),
        probe: missing(),
    };
    RuntimeInstaller::new(parts, data)
}

/// Waits (without sleeping) for the install's last phase.
fn final_phase(pump: &EventPump) -> InstallPhase {
    loop {
        for event in pump.drain() {
            if let UiEvent::WebView(WebViewEvent::Phase(phase)) = event {
                if phase.is_finished() {
                    return phase;
                }
            }
        }
        std::thread::yield_now();
    }
}

/// "Try again" is pressed the moment the failure shows: the worker must already have opened the
/// gate, or the press would be refused. 300 installs on real threads.
#[test]
fn race_try_again_right_after_a_failure_always_finds_the_gate_open() {
    let data = std::env::temp_dir().join(format!("cs_wv2_bridge_{}", std::process::id()));
    let gate = InstallGate::new();
    let board = Arc::new(StatusBoard::new());
    let pump = EventPump::new();
    for round in 0..300 {
        let job = InstallJob {
            installer: installer(data.clone()),
            probe: missing(),
            board: Arc::clone(&board),
            cancel: CancelFlag::new(),
            run: gate
                .try_enter()
                .expect("the gate is open for each new round"),
        };
        let sender = pump.sender();
        let worker = std::thread::spawn(move || job.run(&sender));
        assert_eq!(
            final_phase(&pump),
            InstallPhase::Failed(SetupError::Download("offline".into())),
            "round {round}"
        );
        assert!(
            gate.try_enter().is_some(),
            "round {round}: Try again was refused"
        );
        worker.join().unwrap();
    }
    assert_eq!(board.current(), Some(RuntimeStatus::Missing));
    let _ = std::fs::remove_dir_all(&data);
}
