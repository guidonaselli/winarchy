use super::*;

#[test]
fn eight_small_wheel_deltas_scroll_three_rows_only_at_a_full_detent() {
    let mut remainder = 0;
    let mut results = Vec::new();
    for _ in 0..8 {
        let result = wheel_scroll(remainder, 15, 3, 7);
        remainder = result.0;
        results.push(result);
    }
    assert_eq!(results, [(15, 0), (30, 0), (45, 0), (60, 0), (75, 0), (90, 0), (105, 0), (0, 3)]);
}

#[test]
fn reversing_the_wheel_cancels_the_pending_fraction_before_scrolling() {
    let first = wheel_scroll(0, 90, 3, 7);
    let second = wheel_scroll(first.0, -30, 3, 7);
    let third = wheel_scroll(second.0, -180, 3, 7);
    assert_eq!([first, second, third], [(90, 0), (60, 0), (0, -3)]);
}

#[test]
fn a_double_detent_scrolls_twice_the_configured_lines() {
    assert_eq!(wheel_scroll(0, 240, 3, 7), (0, 6));
}

#[test]
fn page_scrolling_uses_the_visible_page_in_both_directions() {
    assert_eq!(
        [wheel_scroll(0, 120, usize::MAX, 7), wheel_scroll(0, -240, usize::MAX, 7)],
        [(0, 7), (0, -14)]
    );
}

#[test]
fn zero_configured_lines_scroll_no_rows_and_preserve_the_fraction() {
    assert_eq!(wheel_scroll(45, 120, 0, 7), (45, 0));
}

#[test]
fn a_partial_detent_is_retained_after_scrolling_in_both_directions() {
    assert_eq!([wheel_scroll(0, 135, 3, 7), wheel_scroll(0, -135, 3, 7)], [(15, 3), (-15, -3)]);
}
