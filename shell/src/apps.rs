// Modelo puro del dock: qué ventanas cuentan como app, cómo se agrupan y cuándo el dock se oculta.
// Sin Win32: lo que se lee del sistema entra como datos planos para poder probarlo.

pub type Rect = (i32, i32, i32, i32);

pub struct WinAttrs {
    pub visible: bool,
    pub owned: bool,
    pub appwindow: bool,
    pub toolwindow: bool,
    pub cloaked: bool,
    pub managed: bool,
    pub own_process: bool,
}

// Filtro estándar de taskbar: WS_EX_APPWINDOW gana sobre owner y toolwindow. komorebi cloakea las
// ventanas de otros workspaces (o con window_hiding_behaviour=Hide, las oculta): una que él gestiona sigue contando aunque no se vea.
pub fn eligible(a: &WinAttrs) -> bool {
    (a.visible || a.managed) && !a.own_process && (!a.owned || a.appwindow) && (!a.toolwindow || a.appwindow) && (!a.cloaked || a.managed)
}

// Un AUMID agrupa mejor que el exe (Configuración, PWAs y Electron comparten `ApplicationFrameHost`/`msedge`).
pub fn group_key(aumid: Option<&str>, exe: &str) -> String {
    match aumid.filter(|a| !a.is_empty()) {
        Some(a) => format!("aumid:{}", a.to_lowercase()),
        None => format!("exe:{}", exe.to_lowercase()),
    }
}

pub fn tray_wait_cancel(waiting: bool, same_foreground: bool, tray_window: bool, menu_active: bool) -> bool {
    waiting && !same_foreground && !tray_window && !menu_active
}

pub fn tray_activation_ready(win: bool, ctrl: bool, alt: bool, shift: bool) -> bool {
    !(win || ctrl || alt || shift)
}

pub fn tray_focus_dismisses(seen_tray: bool, tray_window: bool, menu_active: bool) -> bool {
    seen_tray && !tray_window && !menu_active
}

pub fn tray_click_dismisses(class: &str) -> bool {
    !matches!(class, "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "TopLevelWindowForOverflowXamlIsland" | "#32768")
}

pub fn tray_surface(class: &str, exe: &str) -> bool {
    !tray_click_dismisses(class) || exe.rsplit(['\\', '/']).next().is_some_and(|name| name.eq_ignore_ascii_case("ShellExperienceHost.exe"))
}

#[cfg(test)]
#[path = "tray_surface_tests.rs"]
mod tray_surface_tests;

pub fn is_start_surface(exe: &str) -> bool {
    ["StartMenuExperienceHost.exe", "SearchHost.exe"].iter().any(|name| exe.eq_ignore_ascii_case(name))
}

pub fn show_start_surface(exe: &str, visible: bool, cloaked: bool, on_monitor: bool) -> bool {
    visible && !cloaked && on_monitor && is_start_surface(exe)
}

pub struct Foreground {
    pub rect: Rect,
    pub caption: bool,
    pub thickframe: bool,
    pub zoomed: bool,
    pub shell_like: bool,
    pub on_dock_monitor: bool,
}

// Ventana sin marco que cubre el monitor del dock (juego borderless, video, presentación). El escritorio,
// una maximizada con marco o un juego en otro monitor no cuentan.
pub fn covers_monitor(fg: &Foreground, monitor: Rect) -> bool {
    let (l, t, r, b) = fg.rect;
    fg.on_dock_monitor && l <= monitor.0 && t <= monitor.1 && r >= monitor.2 && b >= monitor.3 && !fg.caption && !fg.thickframe && !fg.zoomed && !fg.shell_like
}

// `quns_busy` viene de SHQueryUserNotificationState, que no distingue monitor: solo cuenta si el foreground está en el del dock.
// Una maximizada con marco (Steam, fuera de komorebi) cubre el monitor con el dock en `hide` y Windows la da por fullscreen: no cuenta.
pub fn should_hide(fg: Option<&Foreground>, monitor: Rect, quns_busy: bool) -> bool {
    fg.is_some_and(|f| covers_monitor(f, monitor) || (quns_busy && f.on_dock_monitor && !f.shell_like && !(f.zoomed && (f.caption || f.thickframe))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win() -> WinAttrs {
        WinAttrs { visible: true, owned: false, appwindow: false, toolwindow: false, cloaked: false, managed: false, own_process: false }
    }

    #[test]
    fn plain_visible_window_is_listed() {
        assert!(eligible(&win()));
    }

    #[test]
    fn hidden_owned_and_tool_windows_are_not() {
        assert!(!eligible(&WinAttrs { visible: false, ..win() }));
        assert!(eligible(&WinAttrs { visible: false, managed: true, ..win() }));
        assert!(!eligible(&WinAttrs { owned: true, ..win() }));
        assert!(!eligible(&WinAttrs { toolwindow: true, ..win() }));
    }

    #[test]
    fn appwindow_style_overrides_owner_and_toolwindow() {
        assert!(eligible(&WinAttrs { owned: true, appwindow: true, ..win() }));
        assert!(eligible(&WinAttrs { toolwindow: true, appwindow: true, ..win() }));
    }

    #[test]
    fn own_windows_never_count() {
        assert!(!eligible(&WinAttrs { own_process: true, appwindow: true, ..win() }));
    }

    #[test]
    fn cloaked_counts_only_when_komorebi_manages_it() {
        assert!(!eligible(&WinAttrs { cloaked: true, ..win() }));
        assert!(eligible(&WinAttrs { cloaked: true, managed: true, ..win() }));
    }

    #[test]
    fn aumid_wins_over_exe_and_is_case_insensitive() {
        assert_ne!(group_key(Some("Microsoft.Windows.Settings"), r"C:\Windows\System32\ApplicationFrameHost.exe"), group_key(Some("Microsoft.WindowsCalculator"), r"C:\Windows\System32\ApplicationFrameHost.exe"));
        assert_eq!(group_key(Some("A.B"), "x.exe"), group_key(Some("a.b"), "y.exe"));
    }

    #[test]
    fn missing_or_empty_aumid_falls_back_to_the_exe() {
        assert_eq!(group_key(None, r"C:\A\Tool.EXE"), group_key(Some(""), r"c:\a\tool.exe"));
    }

    const MON: Rect = (0, 0, 1920, 1080);

    #[test]
    fn tray_activation_is_ready_when_all_modifiers_are_released() {
        assert!(tray_activation_ready(false, false, false, false));
    }

    #[test]
    fn tray_activation_waits_for_each_modifier_to_be_released() {
        assert!(!tray_activation_ready(true, false, false, false));
        assert!(!tray_activation_ready(false, true, false, false));
        assert!(!tray_activation_ready(false, false, true, false));
        assert!(!tray_activation_ready(false, false, false, true));
    }

    #[test]
    fn tray_wait_cancels_when_foreground_changes_to_another_app() {
        assert!(tray_wait_cancel(true, false, false, false));
    }

    #[test]
    fn tray_wait_keeps_waiting_while_foreground_is_unchanged() {
        assert!(!tray_wait_cancel(true, true, false, false));
    }

    #[test]
    fn tray_wait_does_not_cancel_without_a_pending_wait() {
        assert!(!tray_wait_cancel(false, false, false, false));
    }

    #[test]
    fn tray_wait_keeps_waiting_when_foreground_changes_to_the_tray() {
        assert!(!tray_wait_cancel(true, false, true, false));
    }

    #[test]
    fn tray_wait_keeps_waiting_when_a_menu_is_active() {
        assert!(!tray_wait_cancel(true, false, false, true));
    }

    #[test]
    fn tray_activation_waits_for_combined_modifiers_to_be_released() {
        for (win, ctrl, alt, shift) in [
            (true, true, false, false),
            (true, false, true, false),
            (true, false, false, true),
            (false, true, true, false),
            (false, true, false, true),
            (false, false, true, true),
            (true, true, true, false),
            (true, true, false, true),
            (true, false, true, true),
            (false, true, true, true),
            (true, true, true, true),
        ] {
            assert!(!tray_activation_ready(win, ctrl, alt, shift));
        }
    }

    #[test]
    fn tray_clicks_on_taskbars_keep_the_session_open() {
        assert!(!tray_click_dismisses("Shell_TrayWnd"));
        assert!(!tray_click_dismisses("Shell_SecondaryTrayWnd"));
    }

    #[test]
    fn tray_clicks_on_overflow_and_context_menus_keep_the_session_open() {
        assert!(!tray_click_dismisses("TopLevelWindowForOverflowXamlIsland"));
        assert!(!tray_click_dismisses("#32768"));
    }

    #[test]
    fn tray_clicks_on_other_windows_dismiss_the_session() {
        assert!(tray_click_dismisses("Chrome_WidgetWin_1"));
        assert!(tray_click_dismisses("Progman"));
    }

    #[test]
    fn tray_clicks_without_a_window_class_dismiss_the_session() {
        assert!(tray_click_dismisses(""));
    }

    #[test]
    fn tray_focus_before_the_first_tray_focus_keeps_the_session_open() {
        assert!(!tray_focus_dismisses(false, false, false));
        assert!(!tray_focus_dismisses(false, true, false));
        assert!(!tray_focus_dismisses(false, false, true));
        assert!(!tray_focus_dismisses(false, true, true));
    }

    #[test]
    fn tray_focus_on_a_tray_window_keeps_the_session_open() {
        assert!(!tray_focus_dismisses(true, true, false));
        assert!(!tray_focus_dismisses(true, true, true));
    }

    #[test]
    fn tray_focus_with_an_active_menu_keeps_the_session_open() {
        assert!(!tray_focus_dismisses(true, false, true));
    }

    #[test]
    fn tray_focus_alt_tab_after_tray_focus_dismisses_the_session() {
        assert!(tray_focus_dismisses(true, false, false));
    }

    #[test]
    fn visible_start_and_search_surfaces_show_the_dock() {
        assert!(show_start_surface("StartMenuExperienceHost.exe", true, false, true));
        assert!(show_start_surface("SearchHost.exe", true, false, true));
    }

    #[test]
    fn start_and_search_executable_names_are_case_insensitive() {
        assert!(show_start_surface("STARTMENUEXPERIENCEHOST.EXE", true, false, true));
        assert!(show_start_surface("searchhost.exe", true, false, true));
    }

    #[test]
    fn other_processes_do_not_show_the_dock() {
        assert!(!show_start_surface("explorer.exe", true, false, true));
        assert!(!show_start_surface("OtherSearchHost.exe", true, false, true));
        assert!(!show_start_surface("", true, false, true));
    }

    #[test]
    fn start_and_search_on_another_monitor_do_not_show_the_dock() {
        assert!(!show_start_surface("StartMenuExperienceHost.exe", true, false, false));
        assert!(!show_start_surface("SearchHost.exe", true, false, false));
    }

    #[test]
    fn hidden_start_and_search_surfaces_do_not_show_the_dock() {
        assert!(!show_start_surface("StartMenuExperienceHost.exe", false, false, true));
        assert!(!show_start_surface("SearchHost.exe", false, false, true));
    }

    #[test]
    fn cloaked_start_and_search_surfaces_do_not_show_the_dock() {
        assert!(!show_start_surface("StartMenuExperienceHost.exe", true, true, true));
        assert!(!show_start_surface("SearchHost.exe", true, true, true));
    }

    fn fg() -> Foreground {
        Foreground { rect: MON, caption: false, thickframe: false, zoomed: false, shell_like: false, on_dock_monitor: true }
    }

    #[test]
    fn borderless_window_covering_the_monitor_hides_the_dock() {
        assert!(should_hide(Some(&fg()), MON, false));
    }

    #[test]
    fn desktop_maximized_and_framed_windows_do_not_hide_it() {
        assert!(!should_hide(Some(&Foreground { shell_like: true, ..fg() }), MON, false));
        assert!(!should_hide(Some(&Foreground { zoomed: true, ..fg() }), MON, false));
        assert!(!should_hide(Some(&Foreground { caption: true, thickframe: true, ..fg() }), MON, false));
    }

    #[test]
    fn desktop_reported_as_busy_does_not_hide_the_dock() {
        assert!(!should_hide(Some(&Foreground { shell_like: true, ..fg() }), MON, true));
    }

    #[test]
    fn a_game_on_another_monitor_does_not_hide_it() {
        let other = Foreground { on_dock_monitor: false, ..fg() };
        assert!(!should_hide(Some(&other), MON, false));
        assert!(!should_hide(Some(&other), MON, true));
    }

    #[test]
    fn fullscreen_notification_state_hides_it_only_on_the_dock_monitor() {
        let small = Foreground { rect: (100, 100, 800, 600), caption: true, ..fg() };
        assert!(should_hide(Some(&small), MON, true));
        assert!(!should_hide(Some(&small), MON, false));
        assert!(!should_hide(None, MON, true));
    }

    #[test]
    fn a_maximized_window_reported_as_fullscreen_does_not_hide_it() {
        assert!(!should_hide(Some(&Foreground { zoomed: true, caption: true, thickframe: true, ..fg() }), MON, true));
    }

    #[test]
    fn a_maximized_borderless_game_reported_as_fullscreen_hides_it() {
        assert!(should_hide(Some(&Foreground { zoomed: true, caption: false, thickframe: false, ..fg() }), MON, true));
    }
}
