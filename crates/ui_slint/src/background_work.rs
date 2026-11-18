//! Runs work off the UI thread and channels its results back.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::ui_events::UiEvent;

/// The UI thread's end of the result channel.
pub(crate) struct EventPump {
    tx: Sender<UiEvent>,
    rx: Receiver<UiEvent>,
}

impl EventPump {
    pub(crate) fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self { tx, rx }
    }

    /// A sender that worker threads can own.
    pub(crate) fn sender(&self) -> UiSender {
        UiSender {
            tx: self.tx.clone(),
        }
    }

    /// Everything that arrived since the last call, oldest first.
    pub(crate) fn drain(&self) -> Vec<UiEvent> {
        self.rx.try_iter().collect()
    }
}

/// Worker threads hand their results to the UI through this.
#[derive(Clone)]
pub(crate) struct UiSender {
    tx: Sender<UiEvent>,
}

impl UiSender {
    /// Hands `event` to the UI. When the window is already gone nobody is left to show it, so the
    /// event is dropped on purpose (logged at debug level).
    pub(crate) fn send(&self, event: UiEvent) {
        if let Err(error) = self.tx.send(event) {
            tracing::debug!(event = ?error.0, "UI is gone; dropping a background result");
        }
    }

    /// Shows a one-line notice.
    pub(crate) fn notice(&self, text: impl Into<String>) {
        self.send(UiEvent::Notice(text.into()));
    }
}

/// Runs `work` on its own named thread.
pub(crate) fn spawn_work(name: &'static str, work: impl FnOnce() + Send + 'static) {
    if let Err(error) = thread::Builder::new().name(name.into()).spawn(work) {
        tracing::error!(%error, thread = name, "could not start a background thread");
    }
}
