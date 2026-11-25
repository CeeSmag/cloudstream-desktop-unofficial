use slint::{Model, VecModel};

use super::{is_home_on_screen, is_home_top_request, store_row_offset, MAX_REMEMBERED_ROWS};

fn offsets(model: &VecModel<f32>) -> Vec<f32> {
    model.iter().collect()
}

#[test]
fn is_home_on_screen_home_tab_without_title_page_returns_true() {
    assert!(is_home_on_screen(0, false));
}

#[test]
fn is_home_on_screen_title_page_over_home_returns_false() {
    assert!(!is_home_on_screen(0, true));
}

#[test]
fn is_home_on_screen_other_tab_returns_false() {
    for tab in [1, 2, 3, 4] {
        assert!(!is_home_on_screen(tab, false), "tab {tab}");
        assert!(!is_home_on_screen(tab, true), "tab {tab} with a title page");
    }
}

#[test]
fn is_home_top_request_home_tab_on_home_returns_true() {
    assert!(is_home_top_request(true, 0));
}

#[test]
fn is_home_top_request_home_tab_from_elsewhere_returns_false() {
    assert!(!is_home_top_request(false, 0));
}

#[test]
fn is_home_top_request_other_tab_returns_false() {
    for tab in [1, 2, 3, 4, -1] {
        assert!(!is_home_top_request(true, tab), "tab {tab} on Home");
        assert!(!is_home_top_request(false, tab), "tab {tab} elsewhere");
    }
}

#[test]
fn store_row_offset_first_row_sets_value() {
    let model = VecModel::<f32>::default();
    assert!(store_row_offset(&model, 0, -120.0));
    assert_eq!(offsets(&model), vec![-120.0]);
}

#[test]
fn store_row_offset_later_row_fills_gap_with_zero() {
    let model = VecModel::<f32>::default();
    assert!(store_row_offset(&model, 3, -50.5));
    assert_eq!(offsets(&model), vec![0.0, 0.0, 0.0, -50.5]);
}

#[test]
fn store_row_offset_known_row_overwrites_in_place() {
    let model = VecModel::from(vec![-10.0, -20.0, -30.0]);
    assert!(store_row_offset(&model, 1, -99.0));
    assert_eq!(offsets(&model), vec![-10.0, -99.0, -30.0]);
}

#[test]
fn store_row_offset_negative_row_is_refused() {
    let model = VecModel::from(vec![-10.0]);
    assert!(!store_row_offset(&model, -1, -5.0));
    assert_eq!(offsets(&model), vec![-10.0]);
}

#[test]
fn store_row_offset_row_at_cap_is_refused() {
    let model = VecModel::<f32>::default();
    let cap = i32::try_from(MAX_REMEMBERED_ROWS).unwrap();
    assert!(!store_row_offset(&model, cap, -5.0));
    assert_eq!(model.row_count(), 0);
}

#[test]
fn store_row_offset_last_row_below_cap_is_kept() {
    let model = VecModel::<f32>::default();
    let last = i32::try_from(MAX_REMEMBERED_ROWS - 1).unwrap();
    assert!(store_row_offset(&model, last, -5.0));
    assert_eq!(model.row_count(), MAX_REMEMBERED_ROWS);
    assert_eq!(model.row_data(MAX_REMEMBERED_ROWS - 1), Some(-5.0));
}

#[test]
fn store_row_offset_not_a_number_is_refused() {
    let model = VecModel::from(vec![-10.0]);
    for x in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(!store_row_offset(&model, 0, x));
        assert!(!store_row_offset(&model, 2, x));
    }
    assert_eq!(offsets(&model), vec![-10.0]);
}
