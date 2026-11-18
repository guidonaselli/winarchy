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

const PIPE: &str = r"\\.\pipe\winarchy-shell-komorebi";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Default, PartialEq, Debug)]
pub struct Workspaces {
    pub occupied: Vec<bool>,
    pub focused: usize,
    pub layout: String,
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
    Some(Workspaces { occupied, focused, layout })
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
pub fn spawn(on_change: impl Fn() + Send + 'static) {
    std::thread::spawn(move || loop {
        run_once(&on_change);
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

fn run_once(on_change: &impl Fn()) {
    let name: Vec<u16> = PIPE.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let h = CreateNamedPipeW(PCWSTR(name.as_ptr()), FILE_FLAGS_AND_ATTRIBUTES(PIPE_ACCESS_INBOUND.0), PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS, 1, 0, 1 << 20, 0, None);
        if h.is_invalid() {
            return;
        }
        let sub = std::process::Command::new("komorebic.exe").args(["subscribe-pipe", "winarchy-shell-komorebi"]).creation_flags(CREATE_NO_WINDOW).status();
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
                    *cur = Some(ws);
                    drop(cur);
                    on_change();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn ignores_events_without_state() {
        assert!(parse(&serde_json::json!({"event":{}})).is_none());
    }
}
