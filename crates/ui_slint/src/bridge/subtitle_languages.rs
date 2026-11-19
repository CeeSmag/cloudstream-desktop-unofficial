//! Settings > Languages: the preferred subtitle languages as removable chips and the list to add
//! another while there is room (like the old SubtitleLanguages.tsx). These languages are the
//! subtitle filter and the language tags of the player's Sources and Subtitles menus.

use app_services::MAX_PREFERRED_SUBTITLES;
use player_api::{all_languages, Language};

use crate::posters::string_model;
use crate::source_menu::abbreviation;
use crate::CloudStream;

/// Shows `chosen` as chips and offers the languages not chosen yet.
pub(crate) fn show_subtitle_languages(ui: &CloudStream, chosen: &[Language]) {
    ui.set_subtitle_chosen(string_model(chosen.iter().map(chip_label)));
    ui.set_subtitle_labels(string_model(addable(chosen)));
    ui.set_subtitle_full(chosen.len() >= MAX_PREFERRED_SUBTITLES);
    ui.set_subtitle_add_value(add_prompt(chosen).into());
}

/// "EN  English": the tag the player shows, then the name.
pub(crate) fn chip_label(language: &Language) -> String {
    format!("{}  {}", abbreviation(language.name), language.name)
}

/// The languages that can still be added, in the app's order.
pub(crate) fn addable(chosen: &[Language]) -> Vec<String> {
    all_languages()
        .iter()
        .filter(|language| !chosen.iter().any(|c| c.code() == language.code()))
        .map(|language| language.name.to_owned())
        .collect()
}

/// What the add list says before a pick.
pub(crate) fn add_prompt(chosen: &[Language]) -> String {
    if chosen.len() >= MAX_PREFERRED_SUBTITLES {
        format!("Up to {MAX_PREFERRED_SUBTITLES} languages")
    } else {
        "Choose…".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use player_api::{find_language, ENGLISH};

    use super::*;

    #[test]
    fn chip_label_shows_tag_and_name() {
        assert_eq!(chip_label(&ENGLISH), "EN  English");
    }

    #[test]
    fn addable_leaves_out_chosen_languages() {
        let list = addable(&[ENGLISH]);
        assert!(!list.iter().any(|name| name == "English"));
        assert!(list.iter().any(|name| name == "Arabic"));
        assert_eq!(list.len(), all_languages().len() - 1);
    }

    #[test]
    fn add_prompt_says_when_full() {
        let three: Vec<Language> = ["English", "Arabic", "French"]
            .iter()
            .filter_map(|name| find_language(name))
            .collect();
        assert_eq!(add_prompt(&three), "Up to 3 languages");
        assert_eq!(add_prompt(&[ENGLISH]), "Choose…");
    }
}
