use super::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_END, VK_HOME, VK_NEXT, VK_PRIOR};

#[test]
fn down_without_a_selection_starts_at_the_first_visible_row() {
    assert_eq!(keyboard_row(None, 20, 5, 4, VK_DOWN.0), Some(5));
}

#[test]
fn up_without_a_selection_starts_at_the_last_visible_row() {
    assert_eq!(keyboard_row(None, 20, 5, 4, VK_UP.0), Some(8));
}

#[test]
fn down_wraps_from_the_last_row_to_the_first() {
    assert_eq!(keyboard_row(Some(19), 20, 16, 4, VK_DOWN.0), Some(0));
}

#[test]
fn up_wraps_from_the_first_row_to_the_last() {
    assert_eq!(keyboard_row(Some(0), 20, 0, 4, VK_UP.0), Some(19));
}

#[test]
fn home_selects_the_first_row_outside_the_visible_page() {
    assert_eq!(keyboard_row(Some(8), 20, 5, 4, VK_HOME.0), Some(0));
}

#[test]
fn end_selects_the_last_row_outside_the_visible_page() {
    assert_eq!(keyboard_row(Some(5), 20, 5, 4, VK_END.0), Some(19));
}

#[test]
fn page_down_advances_by_the_visible_row_count() {
    assert_eq!(keyboard_row(Some(6), 20, 5, 4, VK_NEXT.0), Some(10));
}

#[test]
fn page_up_retreats_by_the_visible_row_count() {
    assert_eq!(keyboard_row(Some(6), 20, 5, 4, VK_PRIOR.0), Some(2));
}

#[test]
fn page_down_stops_at_the_last_row() {
    assert_eq!(keyboard_row(Some(18), 20, 16, 4, VK_NEXT.0), Some(19));
}

#[test]
fn page_up_stops_at_the_first_row() {
    assert_eq!(keyboard_row(Some(1), 20, 0, 4, VK_PRIOR.0), Some(0));
}

#[test]
fn page_down_without_a_selection_starts_at_the_last_visible_row() {
    assert_eq!(keyboard_row(None, 20, 5, 4, VK_NEXT.0), Some(8));
}

#[test]
fn page_up_without_a_selection_starts_at_the_first_visible_row() {
    assert_eq!(keyboard_row(None, 20, 5, 4, VK_PRIOR.0), Some(5));
}

#[test]
fn navigation_continues_from_a_selection_outside_the_visible_page() {
    assert_eq!(keyboard_row(Some(2), 20, 10, 4, VK_DOWN.0), Some(3));
}

#[test]
fn page_navigation_continues_from_a_selection_outside_the_visible_page() {
    assert_eq!(keyboard_row(Some(15), 20, 0, 4, VK_PRIOR.0), Some(11));
}

#[test]
fn an_empty_menu_has_no_keyboard_selection() {
    for key in [VK_DOWN, VK_UP, VK_HOME, VK_END, VK_NEXT, VK_PRIOR] {
        assert_eq!(keyboard_row(None, 0, 0, 0, key.0), None);
    }
}
