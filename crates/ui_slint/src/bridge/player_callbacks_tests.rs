use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PointerEventButton, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, PhysicalSize, PlatformError};

use super::super::focus_screens::{current_screen, FocusScreen};
use super::super::session::Session;
use super::wire_leave_player;
use crate::background_work::EventPump;
use crate::request_tickets::RequestKind;
use crate::ui_events::UiEvent;
use crate::CloudStream;

struct HeadlessPlatform(Rc<MinimalSoftwareWindow>);

impl Platform for HeadlessPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.0.clone())
    }
}

#[test]
fn player_back_exits_fullscreen_before_showing_maximized_details() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    window.set_size(PhysicalSize::new(1280, 800));
    slint::platform::set_platform(Box::new(HeadlessPlatform(window))).expect("headless window");
    let ui = CloudStream::new().expect("player UI");
    ui.set_starting(false);
    ui.set_details_title("Sample Title".into());
    let session = Rc::new(RefCell::new(Session::default()));
    let weak = ui.as_weak();
    let leaving = Rc::clone(&session);
    wire_leave_player(&ui, move || {
        let ui = weak.unwrap();
        assert!(!ui.window().is_fullscreen());
        assert!(!ui.get_fullscreen());
        leaving.borrow_mut().abandon_open();
        ui.set_player_open(false);
    });

    for (fullscreen, maximized) in [(true, false), (true, true), (false, false), (false, true)] {
        ui.window().set_maximized(maximized);
        ui.window().set_fullscreen(fullscreen);
        ui.set_fullscreen(fullscreen);
        ui.set_player_open(true);
        if fullscreen && !maximized {
            verify_back_cancels_queued_and_late_open_ready(&ui, &session);
        } else {
            click_player_back(&ui);
        }
        assert_eq!(current_screen(&ui), Some(FocusScreen::Details));
        assert_eq!(ui.window().is_maximized(), fullscreen || maximized);
    }
}

fn click_player_back(ui: &CloudStream) {
    let position = LogicalPosition::new(52.0, 45.0);
    let button = PointerEventButton::Left;
    ui.window()
        .dispatch_event(WindowEvent::PointerPressed { position, button });
    ui.window()
        .dispatch_event(WindowEvent::PointerReleased { position, button });
}

fn verify_back_cancels_queued_and_late_open_ready(ui: &CloudStream, session: &RefCell<Session>) {
    let pump = EventPump::new();
    let ticket = session.borrow_mut().tickets.issue(RequestKind::Open);
    pump.sender().send(UiEvent::OpenReady(ticket, 0));

    let (finished, completions) = mpsc::channel();
    let workers = (0..4)
        .map(|_| {
            let sender = pump.sender();
            let finished = finished.clone();
            thread::spawn(move || {
                for index in 0..512 {
                    sender.send(UiEvent::OpenReady(ticket, index));
                }
                finished.send(()).expect("test is receiving completions");
            })
        })
        .collect::<Vec<_>>();
    click_player_back(ui);
    pump.sender().send(UiEvent::OpenReady(ticket, 0));
    for _ in &workers {
        completions
            .recv_timeout(Duration::from_secs(1))
            .expect("open reply producer finished");
    }
    for worker in workers {
        worker.join().expect("open reply producer succeeded");
    }

    let events = pump.drain();
    assert_eq!(events.len(), 2 + 4 * 512);
    assert!(events.iter().all(|event| event
        .ticket()
        .is_some_and(|ticket| !session.borrow().tickets.is_current(ticket))));
}
