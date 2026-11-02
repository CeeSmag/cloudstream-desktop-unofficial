//! One module per screen. Each draws the model and pushes `UiAction`s; none calls a service.
pub(crate) mod details_screen;
pub(crate) mod downloads_screen;
mod extensions_panel;
pub(crate) mod home_screen;
pub(crate) mod library_screen;
mod poster_grid;
pub(crate) mod search_screen;
pub(crate) mod settings_screen;
