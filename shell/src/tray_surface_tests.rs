use super::tray_surface;

#[test]
fn quick_settings_executable_keeps_the_tray_session_open() {
    assert!(tray_surface("Windows.UI.Core.CoreWindow", "ShellExperienceHost.exe"));
}

#[test]
fn quick_settings_executable_is_recognized_in_a_full_path() {
    assert!(tray_surface("Windows.UI.Core.CoreWindow", r"C:\Windows\SystemApps\ShellExperienceHost_cw5n1h2txyewy\ShellExperienceHost.exe"));
    assert!(tray_surface("Windows.UI.Core.CoreWindow", "C:/Windows/SystemApps/ShellExperienceHost.exe"));
}

#[test]
fn quick_settings_executable_name_is_case_insensitive() {
    assert!(tray_surface("Windows.UI.Core.CoreWindow", r"C:\Windows\SystemApps\SHELLEXPERIENCEHOST.EXE"));
}

#[test]
fn another_app_with_the_quick_settings_window_class_is_not_a_tray_surface() {
    assert!(!tray_surface("Windows.UI.Core.CoreWindow", r"C:\Apps\Other.exe"));
}

#[test]
fn known_tray_classes_do_not_require_an_executable() {
    for class in ["Shell_TrayWnd", "Shell_SecondaryTrayWnd", "TopLevelWindowForOverflowXamlIsland", "#32768"] {
        assert!(tray_surface(class, ""), "{class}");
    }
}

#[test]
fn generic_windows_without_an_executable_are_not_tray_surfaces() {
    assert!(!tray_surface("Windows.UI.Core.CoreWindow", ""));
    assert!(!tray_surface("", ""));
}

#[test]
fn executable_name_must_match_the_entire_basename() {
    assert!(!tray_surface("Windows.UI.Core.CoreWindow", "OtherShellExperienceHost.exe"));
    assert!(!tray_surface("Windows.UI.Core.CoreWindow", "ShellExperienceHost.exe.bak"));
}
