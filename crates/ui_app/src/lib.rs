//! Screens, widgets, theme and navigation (egui), mirroring the CloudStream layout.
#![forbid(unsafe_code)]

mod actions;
mod app;
mod app_model;
mod event_handler;
mod navigation;
mod notice;
mod screens;
mod task_runner;
mod theme;
mod ui_event;
mod ui_services;
mod widgets;

pub use app::run_desktop_app;
pub use ui_services::UiServices;
