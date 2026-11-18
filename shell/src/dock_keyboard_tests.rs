use super::*;

#[test]
fn keyboard_normalization_moves_a_hidden_group_to_overflow() {
    let items = [app("editor"), app("browser")];
    assert_eq!(normalize_keyboard_target(&KeyTarget::Item(GroupId::App("browser".into())), &items, 1, 1), KeyTarget::Overflow);
}

#[test]
fn keyboard_normalization_moves_a_disappeared_overflow_to_tray() {
    assert_eq!(normalize_keyboard_target(&KeyTarget::Overflow, &[app("editor")], 1, 0), KeyTarget::Tray);
}

#[test]
fn keyboard_normalization_preserves_a_visible_group() {
    let items = [app("editor"), app("browser")];
    assert_eq!(normalize_keyboard_target(&KeyTarget::Item(GroupId::App("editor".into())), &items, 1, 1), KeyTarget::Item(GroupId::App("editor".into())));
}

#[test]
fn keyboard_normalization_preserves_brand() {
    assert_eq!(normalize_keyboard_target(&KeyTarget::Brand, &[], 0, 0), KeyTarget::Brand);
}

#[test]
fn keyboard_normalization_preserves_tray() {
    assert_eq!(normalize_keyboard_target(&KeyTarget::Tray, &[], 0, 0), KeyTarget::Tray);
}

#[test]
fn keyboard_normalization_preserves_existing_overflow() {
    assert_eq!(normalize_keyboard_target(&KeyTarget::Overflow, &[app("editor")], 0, 1), KeyTarget::Overflow);
}

fn app(key: &str) -> Item {
    Item { key: key.into(), exe: key.into(), aumid: None, wins: vec![21], active: false, other_ws: false, pin: None }
}

#[test]
fn keyboard_targets_without_apps_keep_brand_and_tray() {
    assert_eq!(keyboard_targets(&[], 0, 0), vec![KeyTarget::Brand, KeyTarget::Tray]);
}

#[test]
fn keyboard_targets_follow_visible_apps_in_dock_order() {
    let items = [app("editor"), app("browser")];
    assert_eq!(keyboard_targets(&items, 2, 0), vec![KeyTarget::Brand, KeyTarget::Item(GroupId::App("editor".into())), KeyTarget::Item(GroupId::App("browser".into())), KeyTarget::Tray]);
}

#[test]
fn keyboard_targets_replace_hidden_apps_with_one_overflow_target() {
    let items = [app("editor"), app("browser"), app("terminal")];
    assert_eq!(keyboard_targets(&items, 1, 2), vec![KeyTarget::Brand, KeyTarget::Item(GroupId::App("editor".into())), KeyTarget::Overflow, KeyTarget::Tray]);
}

#[test]
fn keyboard_targets_can_show_only_overflow_between_brand_and_tray() {
    assert_eq!(keyboard_targets(&[app("editor")], 0, 1), vec![KeyTarget::Brand, KeyTarget::Overflow, KeyTarget::Tray]);
}

#[test]
fn keyboard_targets_use_pin_identity_even_without_a_running_window() {
    let items = [Item { key: "pin:calculator".into(), exe: String::new(), aumid: None, wins: vec![], active: false, other_ws: false, pin: Some("Calculator.lnk".into()) }];
    assert_eq!(keyboard_targets(&items, 1, 0), vec![KeyTarget::Brand, KeyTarget::Item(GroupId::Pin("Calculator.lnk".into())), KeyTarget::Tray]);
}

#[test]
fn keyboard_step_moves_right_through_visible_apps() {
    let targets = keyboard_targets(&[app("editor"), app("browser")], 2, 0);
    assert_eq!(keyboard_step(&targets, &KeyTarget::Item(GroupId::App("editor".into())), 1), KeyTarget::Item(GroupId::App("browser".into())));
}

#[test]
fn keyboard_step_moves_left_through_overflow() {
    let targets = keyboard_targets(&[app("editor"), app("browser")], 1, 1);
    assert_eq!(keyboard_step(&targets, &KeyTarget::Tray, -1), KeyTarget::Overflow);
}

#[test]
fn keyboard_step_wraps_right_from_tray_to_brand() {
    let targets = keyboard_targets(&[], 0, 0);
    assert_eq!(keyboard_step(&targets, &KeyTarget::Tray, 1), KeyTarget::Brand);
}

#[test]
fn keyboard_step_wraps_left_from_brand_to_tray() {
    let targets = keyboard_targets(&[], 0, 0);
    assert_eq!(keyboard_step(&targets, &KeyTarget::Brand, -1), KeyTarget::Tray);
}

#[test]
fn keyboard_step_tracks_the_group_after_apps_are_reordered() {
    let targets = keyboard_targets(&[app("browser"), app("editor")], 2, 0);
    assert_eq!(keyboard_step(&targets, &KeyTarget::Item(GroupId::App("editor".into())), -1), KeyTarget::Item(GroupId::App("browser".into())));
}

#[test]
fn keyboard_step_tracks_the_group_after_an_earlier_app_is_removed() {
    let targets = keyboard_targets(&[app("browser"), app("terminal")], 2, 0);
    assert_eq!(keyboard_step(&targets, &KeyTarget::Item(GroupId::App("browser".into())), 1), KeyTarget::Item(GroupId::App("terminal".into())));
}

#[test]
fn keyboard_step_returns_to_brand_when_the_selected_app_disappears() {
    let targets = keyboard_targets(&[app("browser")], 1, 0);
    assert_eq!(keyboard_step(&targets, &KeyTarget::Item(GroupId::App("editor".into())), 1), KeyTarget::Brand);
}
