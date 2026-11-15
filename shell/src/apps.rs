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
// ventanas de otros workspaces, así que una cloaked que él gestiona sigue contando.
pub fn eligible(a: &WinAttrs) -> bool {
    a.visible && !a.own_process && (!a.owned || a.appwindow) && (!a.toolwindow || a.appwindow) && (!a.cloaked || a.managed)
}

// Un AUMID agrupa mejor que el exe (Configuración, PWAs y Electron comparten `ApplicationFrameHost`/`msedge`).
pub fn group_key(aumid: Option<&str>, exe: &str) -> String {
    match aumid.filter(|a| !a.is_empty()) {
        Some(a) => format!("aumid:{}", a.to_lowercase()),
        None => format!("exe:{}", exe.to_lowercase()),
    }
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
pub fn should_hide(fg: Option<&Foreground>, monitor: Rect, quns_busy: bool) -> bool {
    fg.is_some_and(|f| covers_monitor(f, monitor) || (quns_busy && f.on_dock_monitor))
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
}
