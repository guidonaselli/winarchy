use super::*;

#[test]
fn node_ids_are_assigned_in_dock_order_for_distinct_action_keys() {
    let mut registry = NodeRegistry::new();
    assert_eq!(registry.sync(&[
        "logo".into(), "tray".into(), "overflow".into(),
        "group:logo".into(), "group:tray".into(), "group:overflow".into(),
    ]), vec![1, 2, 3, 4, 5, 6]);
}

#[test]
fn node_resolution_returns_each_current_action_key() {
    let mut registry = NodeRegistry::new();
    registry.sync(&["logo".into(), "tray".into(), "overflow".into(), "group:editor".into()]);
    assert_eq!(
        [registry.resolve(1), registry.resolve(2), registry.resolve(3), registry.resolve(4)],
        [Some("logo"), Some("tray"), Some("overflow"), Some("group:editor")],
    );
}

#[test]
fn reordered_groups_keep_their_original_action_ids() {
    let mut registry = NodeRegistry::new();
    registry.sync(&["logo".into(), "group:editor".into(), "group:browser".into(), "tray".into()]);
    assert_eq!(
        registry.sync(&["logo".into(), "group:browser".into(), "group:editor".into(), "tray".into()]),
        vec![1, 3, 2, 4],
    );
}

#[test]
fn removing_a_group_invalidates_its_action_without_affecting_survivors() {
    let mut registry = NodeRegistry::new();
    registry.sync(&["logo".into(), "group:editor".into(), "tray".into()]);
    registry.sync(&["logo".into(), "tray".into()]);
    assert_eq!(
        [registry.resolve(1), registry.resolve(2), registry.resolve(3)],
        [Some("logo"), None, Some("tray")],
    );
}

#[test]
fn returning_groups_receive_new_ids_and_leave_old_actions_invalid() {
    let mut registry = NodeRegistry::new();
    registry.sync(&["group:editor".into()]);
    registry.sync(&[]);
    assert_eq!(registry.sync(&["group:browser".into(), "group:editor".into()]), vec![2, 3]);
    assert_eq!(registry.resolve(1), None);
    assert_eq!(registry.resolve(3), Some("group:editor"));
}

#[test]
fn empty_sync_retires_every_current_action() {
    let mut registry = NodeRegistry::new();
    registry.sync(&["logo".into(), "tray".into(), "overflow".into(), "group:editor".into()]);
    assert_eq!(registry.sync(&[]), Vec::<u64>::new());
    assert_eq!(
        [registry.resolve(1), registry.resolve(2), registry.resolve(3), registry.resolve(4)],
        [None, None, None, None],
    );
}

#[test]
fn unknown_action_ids_do_not_resolve() {
    let registry = NodeRegistry::new();
    assert_eq!([registry.resolve(0), registry.resolve(1), registry.resolve(u64::MAX)], [None, None, None]);
}
