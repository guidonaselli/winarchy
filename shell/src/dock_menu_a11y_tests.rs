use super::*;
use crate::dock::GroupId;

#[test]
fn accessible_window_row_includes_the_process_identity() {
    assert_eq!(
        a11y_row_keys(Act::Focus(21), Some((21, 100)), None, false),
        vec!["window:21:100"],
    );
}

#[test]
fn accessible_close_button_is_published_for_an_eligible_window() {
    assert_eq!(
        a11y_row_keys(Act::Focus(21), Some((21, 100)), None, true),
        vec!["window:21:100", "close:21:100"],
    );
}

#[test]
fn reused_window_handles_have_different_accessible_identities() {
    assert_eq!(
        [
            a11y_row_keys(Act::Focus(21), Some((21, 100)), None, true),
            a11y_row_keys(Act::Focus(21), Some((21, 200)), None, true),
        ],
        [
            vec!["window:21:100", "close:21:100"],
            vec!["window:21:200", "close:21:200"],
        ],
    );
}

#[test]
fn accessible_window_rows_require_a_matching_known_process() {
    assert_eq!(
        [
            a11y_row_keys(Act::Focus(21), None, None, true),
            a11y_row_keys(Act::Focus(21), Some((21, 0)), None, true),
            a11y_row_keys(Act::Focus(21), Some((22, 100)), None, true),
        ],
        [Vec::<String>::new(), Vec::new(), Vec::new()],
    );
}

#[test]
fn accessible_group_identity_survives_a_changed_menu_index() {
    let group = GroupId::App("exe:editor".into());
    assert_eq!(
        [
            a11y_row_keys(Act::Group(0), None, Some(&group), false),
            a11y_row_keys(Act::Group(3), None, Some(&group), false),
        ],
        [vec!["group:app:exe:editor"], vec!["group:app:exe:editor"]],
    );
}

#[test]
fn accessible_app_and_pin_groups_cannot_share_an_identity() {
    let app = GroupId::App("editor".into());
    let pin = GroupId::Pin("editor".into());
    assert_eq!(
        [
            a11y_row_keys(Act::Group(0), None, Some(&app), true),
            a11y_row_keys(Act::Group(1), None, Some(&pin), true),
        ],
        [vec!["group:app:editor"], vec!["group:pin:editor"]],
    );
}

#[test]
fn accessible_group_without_a_stable_identity_is_omitted() {
    assert_eq!(a11y_row_keys(Act::Group(0), None, None, true), Vec::<String>::new());
}

#[test]
fn accessible_commands_have_distinct_keys_without_window_close_children() {
    let keys: Vec<Vec<String>> = [Act::Open, Act::Pin, Act::Unpin, Act::Close, Act::CloseAll]
        .into_iter()
        .map(|action| a11y_row_keys(action, Some((21, 100)), None, true))
        .collect();
    assert_eq!(keys, vec![
        vec!["action:open"], vec!["action:pin"], vec!["action:unpin"],
        vec!["action:close"], vec!["action:close_all"],
    ]);
}

#[test]
fn accessible_window_keys_follow_reordered_rows_without_changing_identity() {
    let keys: Vec<Vec<String>> = [(31, 200), (21, 100)].into_iter()
        .map(|(hwnd, pid)| a11y_row_keys(Act::Focus(hwnd), Some((hwnd, pid)), None, true))
        .collect();
    assert_eq!(keys, vec![
        vec!["window:31:200", "close:31:200"],
        vec!["window:21:100", "close:21:100"],
    ]);
}

#[test]
fn accessible_group_keys_cannot_collide_with_window_or_command_keys() {
    let group = GroupId::App("window:21:100".into());
    assert_eq!(
        a11y_row_keys(Act::Group(0), Some((21, 100)), Some(&group), true),
        vec!["group:app:window:21:100"],
    );
}
