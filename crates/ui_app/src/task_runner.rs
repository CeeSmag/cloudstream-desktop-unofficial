use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{self, Receiver, Sender};

use crate::notice::Notice;
use crate::ui_event::UiEvent;

/// Runs blocking work off the UI thread and delivers the result as a `UiEvent`.
pub(crate) struct TaskRunner {
    sender: Sender<UiEvent>,
    receiver: Receiver<UiEvent>,
    repaint: egui::Context,
}

impl TaskRunner {
    pub(crate) fn new(repaint: egui::Context) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            repaint,
        }
    }

    /// Runs `job` on a new thread. A panic inside the job becomes an error notice instead of
    /// taking the app down.
    pub(crate) fn spawn(&self, name: &'static str, job: impl FnOnce() -> UiEvent + Send + 'static) {
        let sender = self.sender.clone();
        let repaint = self.repaint.clone();
        let started = std::thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || {
                let event = catch_unwind(AssertUnwindSafe(job)).unwrap_or_else(|_| {
                    tracing::error!(task = name, "background task panicked");
                    UiEvent::Notice(Notice::error(format!(
                        "Something went wrong ({name}). See the log."
                    )))
                });
                if sender.send(event).is_ok() {
                    repaint.request_repaint();
                }
            });
        if let Err(error) = started {
            tracing::error!(task = name, %error, "cannot start background task");
        }
    }

    /// Takes every event that arrived since the last call.
    pub(crate) fn drain(&self) -> Vec<UiEvent> {
        self.receiver.try_iter().collect()
    }
}
