use super::*;

#[test]
fn an_empty_menu_has_no_visible_rows() {
    assert_eq!(visible_rows(0, 32, 6, 1080), 0);
}

#[test]
fn a_single_row_does_not_fill_the_available_height() {
    assert_eq!(visible_rows(1, 32, 6, 1080), 1);
}

#[test]
fn visible_rows_leave_room_for_both_padding_edges() {
    assert_eq!(visible_rows(100, 32, 6, 107), 2);
}

#[test]
fn an_exact_fit_includes_the_last_whole_row() {
    assert_eq!(visible_rows(100, 32, 6, 108), 3);
}

#[test]
fn a_short_work_area_preserves_one_selectable_row() {
    assert_eq!(visible_rows(100, 32, 6, 20), 1);
}

#[test]
fn selecting_within_the_viewport_keeps_its_start() {
    assert_eq!(keep_visible(4, 6, 3, 20), 4);
}

#[test]
fn selecting_the_next_row_scrolls_one_row_down() {
    assert_eq!(keep_visible(4, 7, 3, 20), 5);
}

#[test]
fn selecting_above_the_viewport_reveals_that_row() {
    assert_eq!(keep_visible(4, 3, 3, 20), 3);
}

#[test]
fn a_stale_viewport_start_is_clamped_to_the_last_page() {
    assert_eq!(keep_visible(50, 19, 3, 20), 17);
}

#[test]
fn an_empty_menu_resets_the_viewport_start() {
    assert_eq!(keep_visible(4, 0, 3, 0), 0);
}

#[test]
fn zero_visible_rows_reset_the_viewport_start() {
    assert_eq!(keep_visible(4, 5, 0, 20), 0);
}

#[test]
fn a_single_row_cannot_scroll() {
    assert_eq!(keep_visible(4, 0, 1, 1), 0);
}

#[test]
fn wrapping_up_reveals_the_last_page() {
    let selected = keyboard_row(Some(0), 100, 0, 3, VK_UP.0).unwrap();
    assert_eq!(keep_visible(0, selected, 3, 100), 97);
}

#[test]
fn wrapping_down_reveals_the_first_page() {
    let selected = keyboard_row(Some(99), 100, 97, 3, VK_DOWN.0).unwrap();
    assert_eq!(keep_visible(97, selected, 3, 100), 0);
}

#[test]
fn the_scroll_gutter_has_no_window_row() {
    assert_eq!(row_at_point(236, 38, 6, 32, 3, 240, 4), None);
}

#[test]
fn the_scroll_indicator_has_no_window_row() {
    assert_eq!(row_at_point(238, 38, 6, 32, 3, 240, 4), None);
}

#[test]
fn the_last_pixel_before_the_scroll_gutter_keeps_its_window_row() {
    assert_eq!(row_at_point(235, 38, 6, 32, 3, 240, 4), Some(1));
}

#[test]
fn the_real_menu_inset_keeps_the_last_close_button_pixel() {
    assert_eq!(row_at_point(225, 38, 6, 32, 3, 240, 14), Some(1));
}

#[test]
fn the_real_menu_inset_starts_the_scroll_gutter() {
    assert_eq!(row_at_point(226, 38, 6, 32, 3, 240, 14), None);
}

#[test]
fn a_window_title_still_selects_the_scrolled_window() {
    let row = row_at_point(14, 38, 6, 32, 3, 240, 4).map(|row| row + 4);
    assert_eq!(row, Some(5));
}

#[test]
fn the_close_button_still_targets_the_scrolled_window() {
    let row = row_at_point(210, 38, 6, 32, 3, 240, 4).map(|row| row + 4);
    assert_eq!(close_target(row, 210, 200, &[10, 11, 12, 13, 14, 15, 16], &[15]), Some(15));
}

#[test]
fn a_bottom_menu_is_clamped_to_the_work_area_top() {
    let bar = R { x: 800, y: 900, w: 300, h: 48 };
    assert_eq!(popup_pos(960, bar, 200, 800, true, 8, (0, 120, 1920, 1080)), (860, 120));
}

#[test]
fn a_top_menu_is_clamped_to_the_work_area_bottom() {
    let bar = R { x: -1200, y: 100, w: 300, h: 48 };
    assert_eq!(popup_pos(-1000, bar, 200, 700, false, 8, (-1920, -200, 0, 800)), (-1100, 100));
}

