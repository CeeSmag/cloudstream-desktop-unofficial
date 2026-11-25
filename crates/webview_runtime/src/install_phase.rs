//! Where an install is, for the dialog, and how often to tell it.

use crate::{RuntimeStatus, SetupError};

/// One step of the install, as the dialog shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallPhase {
    /// Downloading Microsoft's setup.
    Downloading {
        /// Bytes received.
        done: u64,
        /// Bytes expected, 0 when the server did not say.
        total: u64,
    },
    /// Checking the setup's signature.
    Verifying,
    /// Microsoft's setup is running (it cannot be stopped from here).
    Installing,
    /// Finished; the runtime is installed.
    Done(RuntimeStatus),
    /// Stopped, with the reason.
    Failed(SetupError),
}

/// `done` of `total` as a whole percent (0-100), or `None` when the total is unknown.
pub fn percent(done: u64, total: u64) -> Option<u8> {
    if total == 0 {
        return None;
    }
    let share = u128::from(done.min(total)) * 100 / u128::from(total);
    u8::try_from(share).ok()
}

impl InstallPhase {
    /// The phase an install's result ends in.
    pub fn finished(result: Result<RuntimeStatus, SetupError>) -> Self {
        match result {
            Ok(status) => Self::Done(status),
            Err(error) => Self::Failed(error),
        }
    }

    /// Whether the viewer can still stop the install (the setup has not started yet).
    pub fn can_cancel(&self) -> bool {
        matches!(self, Self::Downloading { .. } | Self::Verifying)
    }

    /// Whether the install is over.
    pub fn is_finished(&self) -> bool {
        matches!(self, Self::Done(_) | Self::Failed(_))
    }

    /// The download's percent, `None` outside the download or when its size is unknown.
    pub fn percent(&self) -> Option<u8> {
        match self {
            Self::Downloading { done, total } => percent(*done, *total),
            _ => None,
        }
    }

    /// What the dialog would show differently: the phase, and in the download its percent (or,
    /// when the size is unknown, each whole MiB).
    fn shown_as(&self) -> (u8, u64) {
        const MIB: u64 = 1024 * 1024;
        match self {
            Self::Downloading { done, total } => match percent(*done, *total) {
                Some(share) => (0, u64::from(share)),
                None => (0, 1_000 + done / MIB),
            },
            Self::Verifying => (1, 0),
            Self::Installing => (2, 0),
            Self::Done(_) => (3, 0),
            Self::Failed(_) => (4, 0),
        }
    }
}

/// Lets a phase through only when the dialog would show something new, so a fast download does
/// not send one update per 64 KiB.
#[derive(Debug, Default)]
pub struct ProgressThrottle {
    last: Option<(u8, u64)>,
}

impl ProgressThrottle {
    /// A throttle that lets the first phase through.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `phase` should be sent.
    pub fn admit(&mut self, phase: &InstallPhase) -> bool {
        let shown = phase.shown_as();
        let new = self.last != Some(shown);
        self.last = Some(shown);
        new
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;

    fn downloading(done: u64, total: u64) -> InstallPhase {
        InstallPhase::Downloading { done, total }
    }

    #[test]
    fn percents_are_whole_and_capped() {
        assert_eq!(percent(0, 200), Some(0));
        assert_eq!(percent(1, 200), Some(0));
        assert_eq!(percent(2, 200), Some(1));
        assert_eq!(percent(199, 200), Some(99));
        assert_eq!(percent(200, 200), Some(100));
        assert_eq!(percent(500, 200), Some(100));
        assert_eq!(percent(5, 0), None);
        assert_eq!(percent(u64::MAX, u64::MAX), Some(100));
    }

    #[test]
    fn only_the_download_has_a_percent() {
        assert_eq!(downloading(50, 100).percent(), Some(50));
        assert_eq!(downloading(50, 0).percent(), None);
        assert_eq!(InstallPhase::Verifying.percent(), None);
        assert_eq!(InstallPhase::Installing.percent(), None);
    }

    #[test]
    fn cancelling_is_possible_until_the_setup_runs() {
        assert!(downloading(0, 0).can_cancel());
        assert!(InstallPhase::Verifying.can_cancel());
        assert!(!InstallPhase::Installing.can_cancel());
        assert!(!InstallPhase::Done(RuntimeStatus::NotNeeded).can_cancel());
        assert!(!InstallPhase::Failed(SetupError::Cancelled).can_cancel());
    }

    #[test]
    fn results_end_the_install() {
        let done = InstallPhase::finished(Ok(RuntimeStatus::NotNeeded));
        assert_eq!(done, InstallPhase::Done(RuntimeStatus::NotNeeded));
        let failed = InstallPhase::finished(Err(SetupError::StillMissing));
        assert_eq!(failed, InstallPhase::Failed(SetupError::StillMissing));
        assert!(done.is_finished() && failed.is_finished());
        for phase in [
            downloading(1, 2),
            InstallPhase::Verifying,
            InstallPhase::Installing,
        ] {
            assert!(!phase.is_finished(), "{phase:?}");
        }
    }

    #[test]
    fn the_throttle_sends_each_new_percent_once() {
        let mut throttle = ProgressThrottle::new();
        let sent: Vec<u64> = [0, 1, 2, 3, 100, 101, 199, 200]
            .into_iter()
            .filter(|done| throttle.admit(&downloading(*done, 200)))
            .collect();
        assert_eq!(sent, [0, 2, 100, 199, 200]);
    }

    #[test]
    fn the_throttle_sends_each_mib_when_the_size_is_unknown() {
        let mut throttle = ProgressThrottle::new();
        let sent: Vec<u64> = [0, MIB - 1, MIB, MIB + 5, 2 * MIB]
            .into_iter()
            .filter(|done| throttle.admit(&downloading(*done, 0)))
            .collect();
        assert_eq!(sent, [0, MIB, 2 * MIB]);
    }

    #[test]
    fn the_throttle_sends_every_change_of_phase() {
        let mut throttle = ProgressThrottle::new();
        let phases = [
            downloading(100, 100),
            InstallPhase::Verifying,
            InstallPhase::Verifying,
            InstallPhase::Installing,
            InstallPhase::Done(RuntimeStatus::NotNeeded),
            InstallPhase::Failed(SetupError::Cancelled),
        ];
        let sent = phases.iter().filter(|phase| throttle.admit(phase)).count();
        assert_eq!(sent, 5);
    }

    #[test]
    fn a_full_download_and_the_unknown_size_count_are_told_apart() {
        let mut throttle = ProgressThrottle::new();
        assert!(throttle.admit(&downloading(100, 100)));
        // 100 % shown, then a restart with unknown size at 0 MiB must still be sent.
        assert!(throttle.admit(&downloading(0, 0)));
    }
}
