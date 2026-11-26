use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;
use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::Storage::FileSystem::{FILE_FLAGS_AND_ATTRIBUTES, PIPE_ACCESS_INBOUND};
use windows::Win32::System::Pipes::*;
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn pipe_name(session_id: u32) -> String {
    format!(r"\\.\pipe\winarchy-shell-komorebi-{session_id}")
}

fn subscription_name(session_id: u32) -> String {
    format!("winarchy-shell-komorebi-{session_id}")
}

#[derive(Clone, Default, PartialEq, Debug)]
pub struct Workspaces {
    pub occupied: Vec<bool>,
    pub focused: usize,
    pub layout: String,
    pub mode: Mode,
    // Modo del workspace enfocado de cada monitor, por (left, top) del monitor.
    pub monitor_modes: Vec<((i32, i32), Mode)>,
}

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub enum Mode {
    #[default]
    Tiling,
    Scrolling,
    Floating,
}

impl Mode {
    pub fn of(ws: &Value) -> Mode {
        if ws["tile"] == false {
            Mode::Floating
        } else if ws["layout"]["Default"] == "Scrolling" {
            Mode::Scrolling
        } else {
            Mode::Tiling
        }
    }
}

// El ciclo y su persistencia viven en AHK; la shell solo le avisa con un mensaje a su ventana oculta.
pub const WM_CYCLE_MODE: u32 = 0x8000 + 21;

fn ahk_window() -> Option<HWND> {
    use windows::core::w;
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, GetWindowTextW};
    let mut prev = HWND::default();
    while let Ok(h) = unsafe { FindWindowExW(None, Some(&prev), w!("AutoHotkey"), PCWSTR::null()) } {
        let mut title = [0u16; 260];
        let n = unsafe { GetWindowTextW(h, &mut title) } as usize;
        if String::from_utf16_lossy(&title[..n]).contains("winarchy.ahk") {
            return Some(h);
        }
        prev = h;
    }
    None
}

// AHK vigila a la shell; la shell vigila a AHK, así ninguna de las dos queda muerta sin que la otra la levante.
// La tarea programada relanza con #SingleInstance Force, por eso es idempotente.
pub fn spawn_ahk_watch(root: std::path::PathBuf) {
    const POLL: std::time::Duration = std::time::Duration::from_secs(5);
    const WINDOW: std::time::Duration = std::time::Duration::from_secs(300);
    const MAX_RELAUNCHES: usize = 3;
    std::thread::spawn(move || {
        let mut misses = 0;
        let mut relaunches: Vec<std::time::Instant> = Vec::new();
        loop {
            std::thread::sleep(POLL);
            let window = ahk_window();
            if window.is_some_and(responds) {
                misses = 0;
                continue;
            }
            misses += 1;
            relaunches.retain(|t| t.elapsed() < WINDOW);
            if relaunches.len() >= MAX_RELAUNCHES {
                continue;
            }
            // RunWait y Sleep de AHK bombean mensajes: 3 chequeos mudo es un cuelgue, y #SingleInstance no reemplaza a un colgado.
            if let Some(h) = window {
                if misses >= 3 {
                    crate::boot_log(&root, &format!("ahk hung, terminate: {}", terminate_owner(h)));
                }
                continue;
            }
            if misses < 2 {
                continue;
            }
            misses = 0;
            relaunches.push(std::time::Instant::now());
            let started = std::process::Command::new("schtasks.exe").args(["/run", "/tn", r"\Winarchy\ahk"]).creation_flags(0x0800_0000).status();
            crate::boot_log(&root, &format!("ahk missing, relaunch via task: {started:?}"));
        }
    });
}

fn responds(h: HWND) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, SMTO_ABORTIFHUNG, SMTO_ERRORONEXIT, WM_NULL};
    unsafe { SendMessageTimeoutW(h, WM_NULL, WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG | SMTO_ERRORONEXIT, 2000, None).0 != 0 }
}

fn terminate_owner(h: HWND) -> bool {
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(h, Some(&mut pid));
        let Ok(process) = OpenProcess(PROCESS_TERMINATE, false, pid) else { return false };
        let done = TerminateProcess(process, 1).is_ok();
        let _ = CloseHandle(process);
        done
    }
}

// Un hilo de UI colgado no se recupera solo y AHK no mata la shell de sesión: la shell se aborta y
// la relanzan Windows o AHK. Cuenta chequeos seguidos, no reloj, para no disparar al volver de suspensión.
pub fn spawn_ui_watch(window: HWND, root: std::path::PathBuf) {
    let window = window.0 as isize;
    std::thread::spawn(move || {
        wait_until_hung(HWND(window as *mut _), std::time::Duration::from_secs(5), 6);
        crate::boot_log(&root, "exit: ui thread hung, abort");
        crate::dock::restore_native_taskbar();
        std::process::abort();
    });
}

fn wait_until_hung(window: HWND, poll: std::time::Duration, limit: u32) {
    let mut misses = 0;
    while misses < limit {
        std::thread::sleep(poll);
        misses = if responds(window) { 0 } else { misses + 1 };
    }
}

#[cfg(test)]
mod ui_watch_tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;
    use windows::core::w;
    use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, DispatchMessageW, PeekMessageW, HWND_MESSAGE, MSG, PM_REMOVE, WINDOW_EX_STYLE, WINDOW_STYLE};

    #[test]
    fn only_a_thread_that_stops_pumping_counts_as_hung() {
        let (window_tx, window_rx) = mpsc::channel();
        let (stop_pumping_tx, stop_pumping_rx) = mpsc::channel::<()>();
        let (quit_tx, quit_rx) = mpsc::channel::<()>();
        let ui = std::thread::spawn(move || unsafe {
            let window = CreateWindowExW(WINDOW_EX_STYLE(0), w!("STATIC"), w!(""), WINDOW_STYLE(0), 0, 0, 0, 0, HWND_MESSAGE, None, None, None).unwrap();
            window_tx.send(window.0 as isize).unwrap();
            let mut msg = MSG::default();
            while stop_pumping_rx.try_recv().is_err() {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    DispatchMessageW(&msg);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            quit_rx.recv().unwrap();
            let _ = DestroyWindow(window);
        });
        let window = window_rx.recv().unwrap();
        let (hung_tx, hung_rx) = mpsc::channel();
        std::thread::spawn(move || {
            wait_until_hung(HWND(window as *mut _), Duration::from_millis(20), 2);
            let _ = hung_tx.send(());
        });
        assert!(hung_rx.recv_timeout(Duration::from_millis(500)).is_err(), "a pumping thread was reported hung");
        stop_pumping_tx.send(()).unwrap();
        assert!(hung_rx.recv_timeout(Duration::from_secs(20)).is_ok(), "a stuck thread was not detected");
        quit_tx.send(()).unwrap();
        ui.join().unwrap();
    }
}

const STOP_MARKER_TTL: std::time::Duration = std::time::Duration::from_secs(120);

// komorebi ausente y nadie lo paró a propósito (marcador fresco) ni hay un juego en primer plano: es una muerte.
fn should_revive(misses: u32, marker_age: Option<std::time::Duration>, game_mode: bool, recent_relaunches: usize) -> bool {
    misses >= 2 && !game_mode && recent_relaunches < 3 && marker_age.is_none_or(|age| age >= STOP_MARKER_TTL)
}

fn komorebi_running() -> bool {
    use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return true };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut found = false;
        let mut ok = Process32FirstW(snap, &mut entry).is_ok();
        while ok {
            let n = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..n]).eq_ignore_ascii_case("komorebi.exe") {
                found = true;
                break;
            }
            ok = Process32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
        found
    }
}

// Con `Hide`, las ventanas de otros workspaces quedan SW_HIDE y un komorebi nuevo no las conoce: se muestran antes de reiniciarlo.
fn reveal_known_windows() -> usize {
    use windows::Win32::UI::WindowsAndMessaging::{IsIconic, IsWindow, IsWindowVisible, ShowWindow, SW_SHOWNOACTIVATE};
    let hwnds: Vec<isize> = WINDOWS.lock().unwrap().as_ref().map(|m| m.keys().copied().collect()).unwrap_or_default();
    let mut shown = 0;
    for h in hwnds {
        let hwnd = HWND(h as *mut _);
        unsafe {
            if IsWindow(hwnd).as_bool() && !IsWindowVisible(hwnd).as_bool() && !IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                shown += 1;
            }
        }
    }
    shown
}

// Solo vigila si lo vio vivo en esta sesión: quien nunca quiso tiling no lo recibe de golpe.
pub fn spawn_komorebi_watch(root: std::path::PathBuf) {
    std::thread::spawn(move || {
        let (mut seen, mut misses) = (false, 0);
        let mut relaunches: Vec<std::time::Instant> = Vec::new();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(5));
            if komorebi_running() {
                (seen, misses) = (true, 0);
                continue;
            }
            if !seen {
                continue;
            }
            misses += 1;
            relaunches.retain(|t| t.elapsed() < std::time::Duration::from_secs(300));
            let marker_age = std::fs::metadata(root.join("state").join("komorebi-stop")).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok());
            if !should_revive(misses, marker_age, root.join("state").join("game-mode.flag").exists(), relaunches.len()) {
                continue;
            }
            misses = 0;
            relaunches.push(std::time::Instant::now());
            let shown = reveal_known_windows();
            let script = root.join("scripts").join("Start-Komorebi.ps1");
            let started = std::process::Command::new("powershell.exe")
                .args(["-NoProfile", "-WindowStyle", "Hidden", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(&script)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn();
            crate::boot_log(&root, &format!("komorebi missing: showed {shown} hidden windows, relaunch: {:?}", started.map(|c| c.id())));
        }
    });
}

pub fn cycle_mode() {
    if let Some(h) = ahk_window() {
        unsafe {
            let _ = PostMessageW(h, WM_CYCLE_MODE, WPARAM(0), LPARAM(0));
        }
    }
}

pub static CURRENT: Mutex<Option<Workspaces>> = Mutex::new(None);

// hwnd -> (monitor, workspace) de komorebi. Se conserva el último mapa si komorebi se reinicia.
pub type WindowMap = HashMap<isize, (usize, usize)>;
pub static WINDOWS: Mutex<Option<WindowMap>> = Mutex::new(None);
// hwnd de la ventana a la que se avisa (con `WM_WINDOW_MAP`) cuando cambia el mapa.
pub static NOTIFY: AtomicIsize = AtomicIsize::new(0);
pub const WM_WINDOW_MAP: u32 = 0x8000 + 20;

pub fn parse_windows(event: &Value) -> Option<WindowMap> {
    let monitors = event.get("state")?.get("monitors")?.get("elements")?.as_array()?;
    let mut map = WindowMap::new();
    for (mi, m) in monitors.iter().enumerate() {
        for (wi, w) in m["workspaces"]["elements"].as_array().into_iter().flatten().enumerate() {
            let mut add = |v: &Value| {
                if let Some(h) = v["hwnd"].as_i64() {
                    map.insert(h as isize, (mi, wi));
                }
            };
            for c in w["containers"]["elements"].as_array().into_iter().flatten() {
                c["windows"]["elements"].as_array().into_iter().flatten().for_each(&mut add);
            }
            w["floating_windows"]["elements"].as_array().into_iter().flatten().for_each(&mut add);
            w["monocle_container"]["windows"]["elements"].as_array().into_iter().flatten().for_each(&mut add);
        }
    }
    Some(map)
}

pub fn window_slot(hwnd: isize) -> Option<(usize, usize)> {
    WINDOWS.lock().unwrap().as_ref()?.get(&hwnd).copied()
}

pub fn parse(event: &Value) -> Option<Workspaces> {
    let monitors = event.get("state")?.get("monitors")?.get("elements")?.as_array()?;
    let primary = monitors.iter().find(|m| m["size"]["left"] == 0 && m["size"]["top"] == 0).or_else(|| monitors.first())?;
    let ws = &primary["workspaces"];
    let occupied = ws["elements"]
        .as_array()?
        .iter()
        .map(|w| {
            let has = |k: &str| w[k]["elements"].as_array().is_some_and(|a| !a.is_empty());
            has("containers") || has("floating_windows") || !w["monocle_container"].is_null()
        })
        .collect();
    let focused = ws["focused"].as_u64()? as usize;
    let layout = ws["elements"][focused]["layout"]["Default"].as_str().unwrap_or_default().to_string();
    let mode = Mode::of(&ws["elements"][focused]);
    let monitor_modes = monitors
        .iter()
        .filter_map(|m| {
            let wss = &m["workspaces"];
            let at = wss["focused"].as_u64()? as usize;
            Some(((m["size"]["left"].as_i64()? as i32, m["size"]["top"].as_i64()? as i32), Mode::of(&wss["elements"][at])))
        })
        .collect();
    Some(Workspaces { occupied, focused, layout, mode, monitor_modes })
}

// komorebi no sigue el foco a una ventana flotante de otro workspace (Alt+Tab, una notificación, la app que se activa
// sola): queda al frente pero oculta. Si la ventana que pasa al frente está oculta y komorebi la ubica, se va a su workspace.
pub fn follow_foreground() {
    use windows::Win32::UI::Accessibility::SetWinEventHook;
    use windows::Win32::UI::WindowsAndMessaging::{EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT};
    unsafe { SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, None, Some(on_foreground), 0, 0, WINEVENT_OUTOFCONTEXT) };
}

unsafe extern "system" fn on_foreground(_: windows::Win32::UI::Accessibility::HWINEVENTHOOK, _: u32, hwnd: HWND, _: i32, _: i32, _: u32, _: u32) {
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    let mut cloaked = 0u32;
    if DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut _, 4).is_err() || cloaked == 0 {
        return;
    }
    if window_slot(hwnd.0 as isize).is_some() {
        bring_to_front(hwnd.0 as isize);
    }
}

// Lee el estado de komorebi en el momento: el mapa en caché puede ir atrás de una mudanza de ventana y llevar al workspace equivocado.
fn fresh_slot(hwnd: isize) -> Option<(usize, usize)> {
    let out = std::process::Command::new("komorebic.exe").arg("state").creation_flags(CREATE_NO_WINDOW).output().ok()?;
    let state = serde_json::from_slice::<Value>(&out.stdout).ok()?;
    let map = parse_windows(&serde_json::json!({ "state": state }))?;
    let slot = map.get(&hwnd).copied();
    let changed = WINDOWS.lock().unwrap().replace(map.clone()).as_ref() != Some(&map);
    if changed {
        notify_windows();
    }
    slot
}

// Lleva la vista al workspace de la ventana y la sube. Al cambiar de workspace komorebi enfoca su última ventana de ahí, no la pedida: se la vuelve a traer.
pub fn bring_to_front(target: isize) {
    std::thread::spawn(move || {
        if let Some((m, ws)) = fresh_slot(target) {
            let cmd = std::process::Command::new("komorebic.exe").args(crate::dock::focus_args(m, ws)).creation_flags(CREATE_NO_WINDOW).spawn();
            if let Ok(mut child) = cmd {
                let _ = child.wait();
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
        unsafe { crate::dock::raise(HWND(target as *mut _)) };
    });
}

pub fn focus_workspace(index: usize) {
    let _ = std::process::Command::new("komorebic.exe").args(["focus-workspace", &index.to_string()]).creation_flags(CREATE_NO_WINDOW).spawn();
}

// Hilo propio: un suscriptor que no lee degrada a komorebi (probe 1.4), así que nunca se lee desde la UI.
pub fn spawn(session_id: u32, on_change: impl Fn() + Send + 'static) {
    std::thread::spawn(move || loop {
        run_once(session_id, &on_change);
        std::thread::sleep(std::time::Duration::from_secs(2));
    });
}

// El mapa de ventanas solo llega con el próximo evento: se siembra con el estado actual para no perder las ocultas por komorebi.
fn seed_windows() {
    let out = std::process::Command::new("komorebic.exe").arg("state").creation_flags(CREATE_NO_WINDOW).output();
    let state = out.ok().and_then(|o| serde_json::from_slice::<Value>(&o.stdout).ok());
    if let Some(map) = state.and_then(|st| parse_windows(&serde_json::json!({ "state": st }))) {
        *WINDOWS.lock().unwrap() = Some(map);
        notify_windows();
    }
}

fn notify_windows() {
    let n = NOTIFY.load(Ordering::Relaxed);
    if n != 0 {
        unsafe {
            let _ = PostMessageW(HWND(n as *mut _), WM_WINDOW_MAP, WPARAM(0), LPARAM(0));
        }
    }
}

fn run_once(session_id: u32, on_change: &impl Fn()) {
    let name: Vec<u16> = pipe_name(session_id).encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let h = CreateNamedPipeW(PCWSTR(name.as_ptr()), FILE_FLAGS_AND_ATTRIBUTES(PIPE_ACCESS_INBOUND.0), PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS, 1, 0, 1 << 20, 0, None);
        if h.is_invalid() {
            return;
        }
        let subscriber = subscription_name(session_id);
        let sub = std::process::Command::new("komorebic.exe").args(["subscribe-pipe", &subscriber]).creation_flags(CREATE_NO_WINDOW).status();
        if sub.is_ok_and(|s| s.success()) && (ConnectNamedPipe(h, None).is_ok() || GetLastError() == ERROR_PIPE_CONNECTED) {
            std::thread::spawn(seed_windows);
            read_events(h, on_change);
        }
        let _ = DisconnectNamedPipe(h);
        let _ = CloseHandle(h);
    }
}

unsafe fn read_events(h: HANDLE, on_change: &impl Fn()) {
    use std::os::windows::io::FromRawHandle;
    let mut file = std::mem::ManuallyDrop::new(std::fs::File::from_raw_handle(h.0));
    let mut pending = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = match file.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        pending.extend_from_slice(&buf[..n]);
        while let Some(i) = pending.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = pending.drain(..=i).collect();
            let ev = serde_json::from_slice::<Value>(&line).ok();
            if let Some(map) = ev.as_ref().and_then(parse_windows) {
                let mut cur = WINDOWS.lock().unwrap();
                if cur.as_ref() != Some(&map) {
                    *cur = Some(map);
                    drop(cur);
                    notify_windows();
                }
            }
            if let Some(ws) = ev.as_ref().and_then(parse) {
                let mut cur = CURRENT.lock().unwrap();
                if cur.as_ref() != Some(&ws) {
                    let modes_changed = cur.as_ref().map(|w| &w.monitor_modes) != Some(&ws.monitor_modes);
                    *cur = Some(ws);
                    drop(cur);
                    on_change();
                    if modes_changed {
                        notify_windows();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_window_whose_thread_does_not_pump_is_reported_as_hung() {
        use windows::core::w;
        use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE};
        let (tx, rx) = std::sync::mpsc::channel();
        let (done, wait) = std::sync::mpsc::channel::<()>();
        let owner = std::thread::spawn(move || unsafe {
            let h = CreateWindowExW(WINDOW_EX_STYLE(0), w!("Static"), w!(""), WINDOW_STYLE(0), 0, 0, 0, 0, HWND_MESSAGE, None, None, None).unwrap();
            assert!(super::responds(h), "same thread dispatches directly");
            tx.send(h.0 as isize).unwrap();
            let _ = wait.recv();
            let _ = DestroyWindow(h);
        });
        let h = super::HWND(rx.recv().unwrap() as *mut _);
        assert!(!super::responds(h));
        done.send(()).unwrap();
        owner.join().unwrap();
    }

    #[test]
    fn revive_waits_for_two_misses_and_respects_stops_games_and_the_cap() {
        use std::time::Duration;
        let old = Some(Duration::from_secs(600));
        assert!(!should_revive(1, None, false, 0));
        assert!(should_revive(2, None, false, 0));
        assert!(should_revive(2, old, false, 0));
        assert!(!should_revive(2, Some(Duration::from_secs(5)), false, 0));
        assert!(!should_revive(2, None, true, 0));
        assert!(!should_revive(2, None, false, 3));
    }

    use super::*;

    #[test]
    fn pipe_names_are_scoped_to_the_windows_session() {
        assert_eq!(pipe_name(1), r"\\.\pipe\winarchy-shell-komorebi-1");
        assert_eq!(pipe_name(42), r"\\.\pipe\winarchy-shell-komorebi-42");
        assert_ne!(pipe_name(1), pipe_name(42));
        assert_eq!(subscription_name(1), "winarchy-shell-komorebi-1");
        assert_eq!(subscription_name(42), "winarchy-shell-komorebi-42");
        assert_ne!(subscription_name(1), subscription_name(42));
    }

    #[test]
    fn picks_primary_monitor_occupancy_and_focus() {
        let ev = serde_json::json!({"state":{"monitors":{"elements":[
            {"size":{"left":2560,"top":0},"workspaces":{"focused":0,"elements":[{"containers":{"elements":[1]}}]}},
            {"size":{"left":0,"top":0},"workspaces":{"focused":2,"elements":[
                {"containers":{"elements":[]},"floating_windows":{"elements":[]},"monocle_container":null},
                {"containers":{"elements":[1]},"floating_windows":{"elements":[]},"monocle_container":null},
                {"containers":{"elements":[]},"floating_windows":{"elements":[]},"monocle_container":null}]}}]}}});
        let ws = parse(&ev).unwrap();
        assert_eq!(ws.focused, 2);
        assert_eq!(ws.occupied, vec![false, true, false]);
        assert_eq!(ws.layout, "");
    }

    #[test]
    fn maps_every_window_to_its_monitor_and_workspace() {
        let ev = serde_json::json!({"state":{"monitors":{"elements":[
            {"workspaces":{"elements":[
                {"containers":{"elements":[{"windows":{"elements":[{"hwnd":10},{"hwnd":11}]}}]},"floating_windows":{"elements":[{"hwnd":12}]},"monocle_container":null},
                {"containers":{"elements":[]},"floating_windows":{"elements":[]},"monocle_container":{"windows":{"elements":[{"hwnd":13}]}}}]}},
            {"workspaces":{"elements":[{"containers":{"elements":[{"windows":{"elements":[{"hwnd":20}]}}]}}]}}]}}});
        let m = parse_windows(&ev).unwrap();
        assert_eq!((m[&10], m[&11], m[&12], m[&13], m[&20]), ((0, 0), (0, 0), (0, 0), (0, 1), (1, 0)));
        assert_eq!(m.len(), 5);
    }

    #[test]
    fn events_without_state_leave_the_window_map_alone() {
        assert!(parse_windows(&serde_json::json!({"event":{}})).is_none());
    }

    #[test]
    fn reads_the_mode_of_each_monitors_focused_workspace() {
        let ev = serde_json::json!({"state":{"monitors":{"elements":[
            {"size":{"left":0,"top":0},"workspaces":{"focused":1,"elements":[{"tile":true},{"tile":false}]}},
            {"size":{"left":2560,"top":0},"workspaces":{"focused":0,"elements":[{"tile":true,"layout":{"Default":"Scrolling"}}]}}]}}});
        let ws = parse(&ev).unwrap();
        assert_eq!(ws.mode, Mode::Floating);
        assert_eq!(ws.monitor_modes, vec![((0, 0), Mode::Floating), ((2560, 0), Mode::Scrolling)]);
    }

    #[test]
    #[ignore = "needs the winarchy.ahk host running on the desktop"]
    fn finds_the_ahk_host_window() {
        assert!(ahk_window().is_some());
    }

    #[test]
    fn ignores_events_without_state() {
        assert!(parse(&serde_json::json!({"event":{}})).is_none());
    }
}
