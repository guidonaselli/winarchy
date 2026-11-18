use super::*;

const A: MonRect = (0, 0, 1920, 1080);
const B: MonRect = (1920, 0, 3840, 1080);
const C: MonRect = (3840, 0, 5760, 1080);
const PREVIOUS: [(isize, MonRect, f32); 3] = [(10, A, 1.0), (20, B, 1.0), (30, C, 1.0)];

#[test]
fn removing_the_middle_monitor_ends_the_reassigned_dock_session() {
    let cleanup = reconcile_interaction(&PREVIOUS, &[(A, 1.0), (C, 1.0)], Some(20), Some((200, 20)), 200);
    assert_eq!((cleanup.end_keyboard, cleanup.dismiss_menu, cleanup.restore_previous), (true, true, true));
}

#[test]
fn removing_the_middle_monitor_ends_the_removed_dock_session() {
    let cleanup = reconcile_interaction(&PREVIOUS, &[(A, 1.0), (C, 1.0)], Some(30), None, 30);
    assert_eq!((cleanup.end_keyboard, cleanup.dismiss_menu, cleanup.restore_previous), (true, false, true));
}

#[test]
fn removing_another_monitor_preserves_an_unchanged_owner() {
    let cleanup = reconcile_interaction(&PREVIOUS, &[(A, 1.0), (C, 1.0)], Some(10), Some((100, 10)), 100);
    assert_eq!((cleanup.end_keyboard, cleanup.dismiss_menu, cleanup.restore_previous), (false, false, false));
}

#[test]
fn reordering_monitors_ends_the_dock_session_at_the_changed_index() {
    let cleanup = reconcile_interaction(&PREVIOUS, &[(B, 1.0), (A, 1.0), (C, 1.0)], Some(10), None, 10);
    assert_eq!((cleanup.end_keyboard, cleanup.dismiss_menu, cleanup.restore_previous), (true, false, true));
}

#[test]
fn changing_the_owner_dpi_ends_its_session_and_menu() {
    let cleanup = reconcile_interaction(&PREVIOUS, &[(A, 1.0), (B, 1.25), (C, 1.0)], Some(20), Some((200, 20)), 20);
    assert_eq!((cleanup.end_keyboard, cleanup.dismiss_menu, cleanup.restore_previous), (true, true, true));
}

#[test]
fn changing_the_owner_rectangle_ends_its_session() {
    let cleanup = reconcile_interaction(&PREVIOUS, &[(A, 1.0), ((1920, 0, 3840, 1200), 1.0), (C, 1.0)], Some(20), None, 20);
    assert_eq!((cleanup.end_keyboard, cleanup.dismiss_menu, cleanup.restore_previous), (true, false, true));
}

#[test]
fn adding_another_monitor_preserves_the_owner_session_and_menu() {
    let cleanup = reconcile_interaction(&PREVIOUS[..2], &[(A, 1.0), (B, 1.0), (C, 1.0)], Some(20), Some((200, 20)), 200);
    assert_eq!((cleanup.end_keyboard, cleanup.dismiss_menu, cleanup.restore_previous), (false, false, false));
}

#[test]
fn a_mouse_menu_on_a_changed_monitor_closes_without_restoring_focus() {
    let cleanup = reconcile_interaction(&PREVIOUS, &[(A, 1.0), (C, 1.0)], None, Some((200, 20)), 200);
    assert_eq!((cleanup.end_keyboard, cleanup.dismiss_menu, cleanup.restore_previous), (false, true, false));
}

#[test]
fn an_external_foreground_prevents_restoring_a_changed_owner_session() {
    let cleanup = reconcile_interaction(&PREVIOUS, &[(A, 1.0), (C, 1.0)], Some(20), Some((200, 20)), 999);
    assert_eq!((cleanup.end_keyboard, cleanup.dismiss_menu, cleanup.restore_previous), (true, true, false));
}

#[test]
fn a_foreground_menu_of_another_owner_does_not_restore_the_keyboard_session() {
    let cleanup = reconcile_interaction(&PREVIOUS, &[(A, 1.0), (C, 1.0)], Some(20), Some((300, 30)), 300);
    assert_eq!((cleanup.end_keyboard, cleanup.dismiss_menu, cleanup.restore_previous), (true, true, false));
}

#[test]
fn a_changed_mouse_menu_does_not_end_an_unchanged_keyboard_session() {
    let cleanup = reconcile_interaction(&PREVIOUS, &[(A, 1.0), (C, 1.0)], Some(10), Some((200, 20)), 200);
    assert_eq!((cleanup.end_keyboard, cleanup.dismiss_menu, cleanup.restore_previous), (false, true, false));
}

#[test]
fn an_unknown_keyboard_owner_is_ended() {
    let cleanup = reconcile_interaction(&PREVIOUS, &[(A, 1.0), (B, 1.0), (C, 1.0)], Some(99), None, 99);
    assert_eq!((cleanup.end_keyboard, cleanup.dismiss_menu, cleanup.restore_previous), (true, false, true));
}

#[test]
fn scale_changes_within_tolerance_preserve_the_session() {
    let cleanup = reconcile_interaction(&PREVIOUS, &[(A, 1.0), (B, 1.005), (C, 1.0)], Some(20), Some((200, 20)), 200);
    assert_eq!((cleanup.end_keyboard, cleanup.dismiss_menu, cleanup.restore_previous), (false, false, false));
}

#[test]
fn scale_changes_above_tolerance_end_the_session() {
    let cleanup = reconcile_interaction(&PREVIOUS, &[(A, 1.0), (B, 1.011), (C, 1.0)], Some(20), None, 20);
    assert_eq!((cleanup.end_keyboard, cleanup.dismiss_menu, cleanup.restore_previous), (true, false, true));
}

#[test]
fn empty_monitor_and_interaction_state_needs_no_cleanup() {
    let cleanup = reconcile_interaction(&[], &[], None, None, 999);
    assert_eq!((cleanup.end_keyboard, cleanup.dismiss_menu, cleanup.restore_previous), (false, false, false));
}

#[test]
fn keyboard_foreground_ownership_accepts_the_dock() {
    assert_eq!(owns_keyboard_foreground(20, None, 20), true);
}

#[test]
fn keyboard_foreground_ownership_accepts_its_menu() {
    assert_eq!(owns_keyboard_foreground(20, Some((200, 20)), 200), true);
}

#[test]
fn keyboard_foreground_ownership_rejects_another_docks_menu() {
    assert_eq!(owns_keyboard_foreground(20, Some((200, 30)), 200), false);
}

#[test]
fn foreground_recheck_prevents_restoring_after_another_app_takes_focus() {
    let cleanup = reconcile_interaction(&PREVIOUS, &[(A, 1.0), (C, 1.0)], Some(20), Some((200, 20)), 200);
    assert_eq!((cleanup.restore_previous, owns_keyboard_foreground(20, Some((200, 20)), 999)), (true, false));
}
