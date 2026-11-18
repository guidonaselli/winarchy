use super::*;

#[path = "dock_pointer_integration_tests.rs"]
mod integration_tests;

#[test]
fn a_cancelled_drag_does_not_activate_on_release_and_a_new_click_still_works() {
    let mut down = Some(KeyTarget::Item(GroupId::App("editor".into())));
    let _ = down.take();
    let cancelled = finish_pointer_gesture(&mut down, Some(KeyTarget::Item(GroupId::App("editor".into()))), false);
    down = Some(KeyTarget::Item(GroupId::App("editor".into())));
    let fresh = finish_pointer_gesture(&mut down, Some(KeyTarget::Item(GroupId::App("editor".into()))), false);
    assert_eq!((cancelled, fresh), (None, Some(KeyTarget::Item(GroupId::App("editor".into())))));
}

#[test]
fn a_click_activates_its_matching_stable_target() {
    for target in [KeyTarget::Brand, KeyTarget::Tray, KeyTarget::Overflow, KeyTarget::Item(GroupId::Pin("editor.lnk".into()))] {
        let mut down = Some(target.clone());
        assert_eq!(finish_pointer_gesture(&mut down, Some(target.clone()), false), Some(target));
    }
}

#[test]
fn releasing_over_another_group_does_not_activate_it() {
    let mut down = Some(KeyTarget::Item(GroupId::App("editor".into())));
    assert_eq!(finish_pointer_gesture(&mut down, Some(KeyTarget::Item(GroupId::App("browser".into()))), false), None);
}

#[test]
fn a_drag_does_not_activate_even_when_released_over_its_original_target() {
    let mut down = Some(KeyTarget::Item(GroupId::App("editor".into())));
    assert_eq!(finish_pointer_gesture(&mut down, Some(KeyTarget::Item(GroupId::App("editor".into()))), true), None);
}

#[test]
fn a_release_without_a_down_does_not_activate() {
    assert_eq!(finish_pointer_gesture(&mut None, Some(KeyTarget::Brand), false), None);
}

#[test]
fn a_release_consumes_the_down_so_another_release_cannot_activate() {
    let mut down = Some(KeyTarget::Tray);
    let first = finish_pointer_gesture(&mut down, Some(KeyTarget::Tray), false);
    let second = finish_pointer_gesture(&mut down, Some(KeyTarget::Tray), false);
    assert_eq!((first, second, down), (Some(KeyTarget::Tray), None, None));
}

#[test]
fn a_rejected_release_also_consumes_the_down() {
    let mut down = Some(KeyTarget::Brand);
    let outside = finish_pointer_gesture(&mut down, None, false);
    let later = finish_pointer_gesture(&mut down, Some(KeyTarget::Brand), false);
    assert_eq!((outside, later, down), (None, None, None));
}
