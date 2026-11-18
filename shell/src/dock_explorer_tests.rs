use super::*;

const SYSTEM_EXPLORER: &str = r"C:\Windows\explorer.exe";

fn explorer_pin() -> Pin {
    Pin {
        id: "file explorer.lnk".into(),
        open: "file explorer.lnk".into(),
        args: String::new(),
        exe: String::new(),
        aumid: Some("Microsoft.Windows.Explorer".into()),
        launched: None,
    }
}

fn running_window(exe: &str, aumid: Option<&str>, hwnd: isize) -> Item {
    let aumid = explorer_aumid(aumid, exe, SYSTEM_EXPLORER);
    Item {
        key: apps::group_key(aumid.as_deref(), exe),
        exe: exe.into(),
        aumid,
        wins: vec![hwnd],
        active: false,
        other_ws: false,
        pin: None,
    }
}

#[test]
fn explorer_pin_with_no_executable_merges_a_window_without_an_aumid() {
    let out = arrange(&[explorer_pin()], vec![running_window(SYSTEM_EXPLORER, None, 21)]);
    assert_eq!(
        out.iter().map(|item| (item.pin.as_deref(), item.wins.as_slice())).collect::<Vec<_>>(),
        vec![(Some("file explorer.lnk"), &[21][..])],
    );
}

#[test]
fn system_explorer_with_an_empty_aumid_uses_its_canonical_identity() {
    assert_eq!(
        explorer_aumid(Some(""), SYSTEM_EXPLORER, SYSTEM_EXPLORER),
        Some("Microsoft.Windows.Explorer".into()),
    );
}

#[test]
fn system_explorer_path_matching_is_case_insensitive() {
    assert_eq!(
        explorer_aumid(None, r"C:\WINDOWS\Explorer.EXE", SYSTEM_EXPLORER),
        Some("Microsoft.Windows.Explorer".into()),
    );
}

#[test]
fn an_explicit_explorer_surface_keeps_its_identity() {
    assert_eq!(
        explorer_aumid(Some("Microsoft.Windows.Explorer.Folder"), SYSTEM_EXPLORER, SYSTEM_EXPLORER),
        Some("Microsoft.Windows.Explorer.Folder".into()),
    );
}

#[test]
fn explorer_pin_does_not_capture_a_different_explicit_surface() {
    let out = arrange(&[explorer_pin()], vec![
        running_window(SYSTEM_EXPLORER, Some("Microsoft.Windows.Explorer.Folder"), 31),
    ]);
    assert_eq!(
        out.iter().map(|item| (item.pin.as_deref(), item.wins.as_slice())).collect::<Vec<_>>(),
        vec![(Some("file explorer.lnk"), &[][..]), (None, &[31][..])],
    );
}

#[test]
fn explorer_windows_with_missing_or_canonical_aumids_share_a_group_key() {
    let windows = [
        running_window(SYSTEM_EXPLORER, None, 21),
        running_window(SYSTEM_EXPLORER, Some(""), 22),
        running_window(SYSTEM_EXPLORER, Some("Microsoft.Windows.Explorer"), 23),
    ];
    assert_eq!(
        windows.iter().map(|item| item.key.as_str()).collect::<Vec<_>>(),
        vec![
            "aumid:microsoft.windows.explorer",
            "aumid:microsoft.windows.explorer",
            "aumid:microsoft.windows.explorer",
        ],
    );
}

#[test]
fn another_explorer_executable_is_not_given_the_system_identity() {
    assert_eq!(explorer_aumid(None, r"C:\Tools\explorer.exe", SYSTEM_EXPLORER), None);
}

#[test]
fn the_expected_system_explorer_path_is_supplied_by_the_caller() {
    assert_eq!(
        explorer_aumid(None, r"D:\Windows\explorer.exe", r"D:\Windows\explorer.exe"),
        Some("Microsoft.Windows.Explorer".into()),
    );
}

#[test]
fn explorer_pin_keeps_other_app_windows_separate() {
    let out = arrange(&[explorer_pin()], vec![
        running_window(SYSTEM_EXPLORER, None, 21),
        running_window(r"C:\Tools\editor.exe", Some("Editor"), 41),
    ]);
    assert_eq!(
        out.iter().map(|item| (item.pin.as_deref(), item.wins.as_slice())).collect::<Vec<_>>(),
        vec![(Some("file explorer.lnk"), &[21][..]), (None, &[41][..])],
    );
}

#[test]
fn other_apps_with_the_same_executable_keep_distinct_aumid_groups() {
    let windows = [
        running_window(r"C:\Tools\editor.exe", Some("Editor.Work"), 41),
        running_window(r"C:\Tools\editor.exe", Some("Editor.Personal"), 42),
    ];
    assert_eq!(
        windows.iter().map(|item| item.key.as_str()).collect::<Vec<_>>(),
        vec!["aumid:editor.work", "aumid:editor.personal"],
    );
}
