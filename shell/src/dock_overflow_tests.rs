use super::*;

#[test]
fn overflow_selection_follows_the_same_window_when_its_aumid_arrives() {
    let items = [Item { key: "aumid:editor".into(), exe: "Editor.exe".into(), aumid: Some("Editor".into()), wins: vec![21, 22], active: false, other_ws: false, pin: None }];
    assert_eq!(resolve_overflow_group(&items, &GroupId::App("exe:editor".into()), Some((21, 100)), |_| 100), Some(0));
}

#[test]
fn overflow_selection_rejects_a_window_handle_reused_by_another_process() {
    let items = [Item { key: "aumid:other".into(), exe: "Other.exe".into(), aumid: Some("Other".into()), wins: vec![21], active: false, other_ws: false, pin: None }];
    assert_eq!(resolve_overflow_group(&items, &GroupId::App("exe:editor".into()), Some((21, 100)), |_| 200), None);
}

#[test]
fn overflow_selection_rejects_an_unknown_captured_process() {
    let items = [Item { key: "aumid:editor".into(), exe: "Editor.exe".into(), aumid: Some("Editor".into()), wins: vec![21], active: false, other_ws: false, pin: None }];
    assert_eq!(resolve_overflow_group(&items, &GroupId::App("exe:editor".into()), Some((21, 0)), |_| 0), None);
}

#[test]
fn overflow_selection_does_not_replace_a_disappeared_window_with_another_group() {
    let items = [Item { key: "aumid:other".into(), exe: "Other.exe".into(), aumid: Some("Other".into()), wins: vec![31], active: false, other_ws: false, pin: None }];
    assert_eq!(resolve_overflow_group(&items, &GroupId::App("exe:editor".into()), Some((21, 100)), |_| 100), None);
}

#[test]
fn overflow_selection_resolves_a_pin_without_a_window() {
    let items = [Item { key: "pin:calculator".into(), exe: String::new(), aumid: None, wins: vec![], active: false, other_ws: false, pin: Some("Calculator.lnk".into()) }];
    assert_eq!(resolve_overflow_group(&items, &GroupId::Pin("Calculator.lnk".into()), None, |_| 0), Some(0));
}

#[test]
fn overflow_selection_prefers_the_exact_group_over_a_window_witness() {
    let items = [
        Item { key: "aumid:other".into(), exe: "Other.exe".into(), aumid: Some("Other".into()), wins: vec![21], active: false, other_ws: false, pin: None },
        Item { key: "exe:editor".into(), exe: "Editor.exe".into(), aumid: None, wins: vec![31], active: false, other_ws: false, pin: None },
    ];
    assert_eq!(resolve_overflow_group(&items, &GroupId::App("exe:editor".into()), Some((21, 100)), |_| 100), Some(1));
}
