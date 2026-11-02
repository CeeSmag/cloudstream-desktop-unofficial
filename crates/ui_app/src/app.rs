use std::time::{Duration, Instant};

use eframe::egui;

use crate::actions::{Dispatcher, UiAction};
use crate::app_model::AppModel;
use crate::navigation::Tab;
use crate::screens::player_screen::{self, PlayerScene};
use crate::screens::{
    details_screen, downloads_screen, home_screen, library_screen, search_screen, settings_screen,
};
use crate::task_runner::TaskRunner;
use crate::ui_services::UiServices;
use crate::video_presenter::VideoPresenter;
use crate::widgets::{notice_bar, now_playing_bar, stream_picker, top_bar};
use crate::{event_handler, theme};

/// How often the player is polled while something is playing.
const PLAYBACK_POLL: Duration = Duration::from_millis(500);
/// Width of the floating now-playing pill.
const NOW_PLAYING_WIDTH: f32 = 620.0;

/// Opens the main window and runs until it is closed.
pub fn run_desktop_app(services: UiServices) -> Result<(), String> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("CloudStream")
            .with_inner_size([1180.0, 780.0])
            .with_min_inner_size([760.0, 560.0]),
        ..eframe::NativeOptions::default()
    };
    eframe::run_native(
        "CloudStream",
        options,
        Box::new(move |creation| {
            egui_extras::install_image_loaders(&creation.egui_ctx);
            theme::apply(&creation.egui_ctx);
            Ok(Box::new(CloudStreamApp::new(
                creation.egui_ctx.clone(),
                services,
            )))
        }),
    )
    .map_err(|error| error.to_string())
}

/// The root egui application.
struct CloudStreamApp {
    model: AppModel,
    dispatcher: Dispatcher,
    services: UiServices,
    last_poll: Instant,
    presenter: VideoPresenter,
    window_fullscreen: bool,
}

impl CloudStreamApp {
    fn new(context: egui::Context, services: UiServices) -> Self {
        let dispatcher = Dispatcher::new(services.clone(), TaskRunner::new(context));
        let mut model = AppModel::default();
        dispatcher.start_up(&mut model);
        Self {
            model,
            dispatcher,
            services,
            last_poll: Instant::now(),
            presenter: VideoPresenter::default(),
            window_fullscreen: false,
        }
    }

    fn apply_background_results(&mut self) {
        for event in self.dispatcher.drain_events() {
            event_handler::apply(&mut self.model, &self.dispatcher, event);
        }
    }

    fn poll_player(&mut self, context: &egui::Context) {
        let Some(playback) = &self.services.playback else {
            return;
        };
        if self.last_poll.elapsed() < PLAYBACK_POLL {
            return;
        }
        self.last_poll = Instant::now();
        self.model.playback = playback.tick();
        if self.model.playback.state != player_api::PlaybackState::Idle {
            context.request_repaint_after(PLAYBACK_POLL);
        }
    }

    fn draw_top_bar(&mut self, ui: &mut egui::Ui, actions: &mut Vec<UiAction>) {
        let frame = egui::Frame::new().fill(theme::BACKGROUND);
        egui::Panel::top("top_navigation")
            .exact_size(top_bar::HEIGHT)
            .frame(frame)
            .show(ui, |ui| {
                if let Some(tab) = top_bar::show(ui, self.model.tab) {
                    actions.push(UiAction::SwitchTab(tab));
                }
            });
        if !self.model.notices.is_empty() {
            let frame = egui::Frame::new()
                .fill(theme::BACKGROUND)
                .inner_margin(egui::Margin::symmetric(24, 4));
            egui::Panel::top("notices").frame(frame).show(ui, |ui| {
                if let Some(index) = notice_bar::show(ui, &self.model.notices) {
                    actions.push(UiAction::DismissNotice(index));
                }
            });
        }
    }

    fn draw_content(&mut self, ui: &mut egui::Ui, actions: &mut Vec<UiAction>) {
        let frame = egui::Frame::new().fill(theme::BACKGROUND);
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            if let Some(view) = self.model.current_details().cloned() {
                details_screen::show(ui, &self.model, &view, actions);
                return;
            }
            match self.model.tab {
                Tab::Home => home_screen::show(ui, &self.model, actions),
                Tab::Search => search_screen::show(ui, &mut self.model, actions),
                Tab::Library => library_screen::show(ui, &self.model, actions),
                Tab::Downloads => downloads_screen::show(ui),
                Tab::Settings => {
                    settings_screen::show(ui, &mut self.model, &self.services, actions)
                }
            }
        });
    }

    /// The glass pill with playback controls, floating above the content like Apple TV's mini player.
    fn draw_now_playing(&self, context: &egui::Context, actions: &mut Vec<UiAction>) {
        let Some(title) = self.model.now_playing_title.as_deref() else {
            return;
        };
        if self.model.playback.state == player_api::PlaybackState::Idle || self.model.player.open {
            return;
        }
        egui::Area::new(egui::Id::unique("now_playing"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -20.0))
            .show(context, |ui| {
                egui::Frame::new()
                    .fill(theme::GLASS)
                    .corner_radius(26)
                    .inner_margin(egui::Margin::symmetric(18, 12))
                    .shadow(egui::Shadow {
                        offset: [0, 8],
                        blur: 28,
                        spread: 0,
                        color: egui::Color32::from_black_alpha(160),
                    })
                    .show(ui, |ui| {
                        ui.set_width(NOW_PLAYING_WIDTH);
                        actions.extend(now_playing_bar::show(
                            ui,
                            title,
                            &self.model.playback,
                            self.embedded(),
                        ));
                    });
            });
    }

    fn embedded(&self) -> bool {
        self.services
            .playback
            .as_ref()
            .is_some_and(|playback| playback.is_embedded())
    }

    /// Draws the video screen instead of the normal ones.
    fn draw_player(&mut self, ui: &mut egui::Ui, actions: &mut Vec<UiAction>) {
        let context = ui.ctx().clone();
        if let Some(playback) = &self.services.playback {
            let size = ui.max_rect().size() * context.pixels_per_point();
            self.presenter
                .update(&context, playback, (size.x as u32, size.y as u32));
        }
        context.request_repaint_after(std::time::Duration::from_millis(16));
        let title = self.model.now_playing_title.clone().unwrap_or_default();
        let frame = egui::Frame::new().fill(egui::Color32::BLACK);
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            let scene = PlayerScene {
                title: &title,
                status: &self.model.playback,
                presenter: &self.presenter,
            };
            player_screen::show(ui, &mut self.model.player, &scene, actions);
        });
    }

    /// Closes the video screen once playback is over, and tells the user when it failed.
    fn close_player_when_finished(&mut self) {
        if !self.model.player.open {
            return;
        }
        match &self.model.playback.state {
            player_api::PlaybackState::Failed(reason) => {
                let text = format!("Playback failed: {reason}");
                self.model.notify(crate::notice::Notice::error(text));
            }
            player_api::PlaybackState::Ended => {}
            _ => return,
        }
        if self.model.playback.state != player_api::PlaybackState::Loading {
            self.model.player.open = false;
            self.model.player.fullscreen = false;
            self.presenter.reset();
        }
    }

    /// Keeps the OS window in step with the model's fullscreen flag.
    fn sync_fullscreen(&mut self, context: &egui::Context) {
        if self.model.player.fullscreen != self.window_fullscreen {
            self.window_fullscreen = self.model.player.fullscreen;
            context.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.window_fullscreen));
        }
    }

    fn draw_stream_choice(&self, context: &egui::Context, actions: &mut Vec<UiAction>) {
        if let Some(pending) = &self.model.pending_streams {
            actions.extend(stream_picker::show(
                context,
                &pending.target.item.title,
                &pending.links,
            ));
        }
    }
}

impl eframe::App for CloudStreamApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        self.apply_background_results();
        self.poll_player(&context);
        let mut actions = Vec::new();
        if self.model.player.open && self.embedded() {
            self.draw_player(ui, &mut actions);
        } else {
            self.draw_top_bar(ui, &mut actions);
            self.draw_content(ui, &mut actions);
        }
        self.draw_now_playing(&context, &mut actions);
        self.draw_stream_choice(&context, &mut actions);
        for action in actions {
            self.dispatcher.perform(&mut self.model, action);
        }
        self.close_player_when_finished();
        self.sync_fullscreen(&context);
    }
}
