// Bandeja del sistema dentro del dock. Windows 11 no expone los bitmaps de los íconos de bandeja;
// `YASBTrayHook.dll` (MIT, firmada, la del YASB pinneado) se carga en explorer con un hook WH_GETMESSAGE, hace subclass de
// `Shell_TrayWnd` y reenvía cada `Shell_NotifyIcon` por un pipe de mensajes. La shell es el servidor del pipe; los clics
// van directo a la ventana dueña del ícono. Sin la DLL, o con YASB corriendo (dueño del mutex), la shell no hace nada y
// el botón de bandeja del dock sigue abriendo la bandeja nativa.
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::sync::Mutex;

const NIF_MESSAGE: u32 = 0x1;
const NIF_ICON: u32 = 0x2;
const NIF_TIP: u32 = 0x4;
const NIF_STATE: u32 = 0x8;
const NIS_HIDDEN: u32 = 0x1;
const NIM_ADD: u32 = 0;
const NIM_MODIFY: u32 = 1;
const NIM_DELETE: u32 = 2;
const NIM_SETVERSION: u32 = 4;
const PACKET_ICON: u32 = 2;
const HEADER: usize = 28;
const MAX_SIDE: u32 = 256;

const WM_LBUTTONDOWN: u32 = 0x201;
const WM_LBUTTONUP: u32 = 0x202;
const WM_RBUTTONDOWN: u32 = 0x204;
const WM_RBUTTONUP: u32 = 0x205;
const WM_MBUTTONDOWN: u32 = 0x207;
const WM_MBUTTONUP: u32 = 0x208;
const WM_CONTEXTMENU: u32 = 0x7B;
const NIN_SELECT: u32 = 0x400;

#[derive(Clone, PartialEq, Debug)]
pub struct Icon {
    pub hwnd: u32,
    pub uid: u32,
    pub callback: u32,
    pub version: u32,
    pub tip: String,
    pub hidden: bool,
    pub exe: String,
    pub size: (u32, u32),
    pub bgra: Vec<u8>,
}

#[derive(Debug, PartialEq)]
pub struct Packet {
    pub message: u32,
    pub hwnd: u32,
    pub uid: u32,
    pub flags: u32,
    pub callback: u32,
    pub version: u32,
    pub tip: String,
    pub state: u32,
    pub mask: u32,
    pub icon: Option<(u32, u32, Vec<u8>)>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Gesture {
    Left,
    Right,
    Middle,
}

fn u32_at(b: &[u8], off: usize) -> u32 {
    b.get(off..off + 4).map_or(0, |s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

// Una trama del hook: cabecera `=IQIIII` (tipo, dwData, cbData, ancho, alto, bytes de ícono), el SHELLTRAYDATA crudo y los píxeles BGRA.
pub fn parse(data: &[u8]) -> Option<Packet> {
    if u32_at(data, 0) != PACKET_ICON || data.len() < HEADER {
        return None;
    }
    let (cb, w, h, icon_len) = (u32_at(data, 12) as usize, u32_at(data, 16), u32_at(data, 20), u32_at(data, 24) as usize);
    let payload = data.get(HEADER..HEADER.checked_add(cb)?)?;
    if payload.len() < 32 {
        return None;
    }
    let flags = u32_at(payload, 20);
    let tip = if flags & NIF_TIP != 0 && payload.len() >= 288 {
        let units: Vec<u16> = payload[32..288].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|u| *u != 0).collect();
        String::from_utf16_lossy(&units)
    } else {
        String::new()
    };
    let pixels = data.get(HEADER + cb..HEADER + cb + icon_len);
    let icon = pixels.filter(|p| w > 0 && h > 0 && w <= MAX_SIDE && h <= MAX_SIDE && p.len() == (w * h * 4) as usize).map(|p| (w, h, p.to_vec()));
    Some(Packet { message: u32_at(payload, 4), hwnd: u32_at(payload, 12), uid: u32_at(payload, 16), flags, callback: u32_at(payload, 24), version: u32_at(payload, 808), tip, state: u32_at(payload, 288), mask: u32_at(payload, 292), icon })
}

// Aplica una trama a la lista; devuelve si algo cambió. `uVersion` solo cuenta en NIM_SETVERSION: en NIM_MODIFY comparte lugar con el timeout.
pub fn apply(icons: &mut Vec<Icon>, p: Packet) -> bool {
    let at = icons.iter().position(|i| i.hwnd == p.hwnd && i.uid == p.uid);
    match p.message {
        NIM_ADD | NIM_MODIFY | NIM_SETVERSION => {
            let before = at.map(|i| icons[i].clone());
            let i = at.unwrap_or_else(|| {
                icons.push(Icon { hwnd: p.hwnd, uid: p.uid, callback: 0, version: 0, tip: String::new(), hidden: false, exe: String::new(), size: (0, 0), bgra: Vec::new() });
                icons.len() - 1
            });
            let icon = &mut icons[i];
            if p.message == NIM_SETVERSION {
                icon.version = p.version;
            } else {
                if p.flags & NIF_MESSAGE != 0 {
                    icon.callback = p.callback;
                }
                if p.flags & NIF_TIP != 0 {
                    icon.tip = p.tip;
                }
                if p.flags & NIF_STATE != 0 && p.mask & NIS_HIDDEN != 0 {
                    icon.hidden = p.state & NIS_HIDDEN != 0;
                }
                if let (true, Some((w, h, px))) = (p.flags & NIF_ICON != 0, p.icon) {
                    icon.size = (w, h);
                    icon.bgra = px;
                }
            }
            before.as_ref() != Some(&icons[i])
        }
        NIM_DELETE => at.map(|i| icons.remove(i)).is_some(),
        _ => false,
    }
}

// Mensajes que Explorer entrega al dueño del ícono por cada gesto.
pub fn messages_for(g: Gesture, version: u32) -> Vec<u32> {
    let mut m = match g {
        Gesture::Left => vec![WM_LBUTTONDOWN, WM_LBUTTONUP],
        Gesture::Right => vec![WM_RBUTTONDOWN, WM_RBUTTONUP],
        Gesture::Middle => vec![WM_MBUTTONDOWN, WM_MBUTTONUP],
    };
    if version >= 3 {
        match g {
            Gesture::Left => m.push(NIN_SELECT),
            Gesture::Right => m.push(WM_CONTEXTMENU),
            _ => {}
        }
    }
    m
}

// (wparam, lparam) del mensaje de callback: versión 4 lleva el cursor y el id, las anteriores solo el id.
pub fn callback_params(version: u32, uid: u32, message: u32, at: (i32, i32)) -> (usize, isize) {
    if version >= 4 {
        (((at.1 as u32 as usize & 0xFFFF) << 16) | (at.0 as u32 as usize & 0xFFFF), (((uid & 0xFFFF) << 16) | (message & 0xFFFF)) as isize)
    } else {
        (uid as usize, message as isize)
    }
}

static ICONS: Mutex<Vec<Icon>> = Mutex::new(Vec::new());
static REVISION: AtomicU64 = AtomicU64::new(0);
static CONNECTED: AtomicBool = AtomicBool::new(false);
static HOOK: AtomicIsize = AtomicIsize::new(0);

pub fn active() -> bool {
    CONNECTED.load(Ordering::Relaxed)
}

pub fn revision() -> u64 {
    REVISION.load(Ordering::Relaxed)
}

// Identidad estable de un ícono entre sesiones: el ejecutable dueño y su id.
pub fn key(i: &Icon) -> String {
    format!("{}:{}", i.exe, i.uid)
}

// Máximo de íconos fijos a la vista en la píldora; el resto queda en el flyout.
pub const PINNED_MAX: usize = 4;

static PINS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static PINS_PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

pub fn load_pins(state_dir: &std::path::Path) {
    let path = state_dir.join("tray-pins.json");
    *PINS.lock().unwrap() = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let _ = PINS_PATH.set(path);
}

fn save_pins(pins: &[String]) {
    let Some(path) = PINS_PATH.get() else { return };
    let Ok(text) = serde_json::to_string_pretty(pins) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

// Los íconos fijos que están presentes, en el orden en que se fijaron y hasta `PINNED_MAX`.
fn pinned_of<'a>(all: &'a [Icon], pins: &[String]) -> Vec<&'a Icon> {
    pins.iter().filter_map(|k| all.iter().find(|i| !i.hidden && !i.bgra.is_empty() && &key(i) == k)).take(PINNED_MAX).collect()
}

pub fn with_pinned<R>(f: impl FnOnce(&[&Icon]) -> R) -> R {
    let all = ICONS.lock().unwrap();
    let pins = PINS.lock().unwrap();
    f(&pinned_of(&all, &pins))
}

pub fn is_pinned(icon: &Icon) -> bool {
    PINS.lock().unwrap().contains(&key(icon))
}

// Fija o desfija; falso si ya hay `PINNED_MAX` a la vista y el ícono no estaba fijo.
pub fn toggle_pin(icon: &Icon) -> bool {
    let k = key(icon);
    let changed = {
        let all = ICONS.lock().unwrap();
        let mut pins = PINS.lock().unwrap();
        if let Some(at) = pins.iter().position(|p| *p == k) {
            pins.remove(at);
        } else if pinned_of(&all, &pins).len() < PINNED_MAX {
            pins.push(k);
        } else {
            return false;
        }
        save_pins(&pins);
        true
    };
    REVISION.fetch_add(1, Ordering::Relaxed);
    changed
}

// Íconos visibles, en el orden en que los anunciaron.
pub fn with_visible<R>(f: impl FnOnce(&[&Icon]) -> R) -> R {
    let all = ICONS.lock().unwrap();
    let visible: Vec<&Icon> = all.iter().filter(|i| !i.hidden && !i.bgra.is_empty()).collect();
    f(&visible)
}

fn set_icons(change: impl FnOnce(&mut Vec<Icon>) -> bool) -> bool {
    let changed = change(&mut ICONS.lock().unwrap());
    if changed {
        REVISION.fetch_add(1, Ordering::Relaxed);
    }
    changed
}

pub fn click(hwnd: u32, uid: u32, gesture: Gesture, at: (i32, i32)) {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, GetWindowThreadProcessId, IsWindow, SendNotifyMessageW};
    let Some((callback, version)) = ICONS.lock().unwrap().iter().find(|i| i.hwnd == hwnd && i.uid == uid).map(|i| (i.callback, i.version)) else { return };
    let target = HWND(hwnd as usize as *mut _);
    unsafe {
        if callback == 0 || !IsWindow(target).as_bool() {
            return;
        }
        // La app dueña suele activarse al recibir el clic: se le cede el derecho a tomar el primer plano.
        let mut pid = 0;
        GetWindowThreadProcessId(target, Some(&mut pid));
        let _ = AllowSetForegroundWindow(pid);
        for message in messages_for(gesture, version) {
            let (w, l) = callback_params(version, uid, message, at);
            let _ = SendNotifyMessageW(target, callback, WPARAM(w), LPARAM(l));
        }
    }
}

const MUTEX_NAME: windows::core::PCWSTR = windows::core::w!("Global\\YASBTrayHookAlive");
const PIPE_NAME: windows::core::PCWSTR = windows::core::w!("\\\\.\\pipe\\yasb_systray_monitor");
const PIPE_BUFFER: u32 = 1 << 20;
const RETRY_MS: u64 = 3000;

fn hook_dll() -> Option<std::path::PathBuf> {
    ["ProgramW6432", "ProgramFiles"].iter().filter_map(|v| std::env::var_os(v)).map(|p| std::path::Path::new(&p).join("YASB").join("lib").join("YASBTrayHook.dll")).find(|p| p.is_file())
}

// Arranca el host si la DLL está y nadie más (YASB) tiene el mutex. `on_change` corre en el hilo del pipe.
pub fn spawn(on_change: impl Fn() + Send + Sync + 'static) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{CreateMutexW, OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};
    let Some(dll) = hook_dll() else { return false };
    unsafe {
        if let Ok(other) = OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, MUTEX_NAME) {
            let _ = CloseHandle(other);
            return false;
        }
        // El mutex vive hasta que el proceso muere: la DLL lo vigila y se descarga de explorer cuando se abandona.
        if CreateMutexW(None, true, MUTEX_NAME).is_err() {
            return false;
        }
    }
    let Some(pipe) = create_pipe() else { return false };
    let on_change = std::sync::Arc::new(on_change);
    let notify = on_change.clone();
    std::thread::spawn(move || read_loop(pipe, move || notify()));
    std::thread::spawn(move || inject_loop(dll, move || on_change()));
    true
}

fn create_pipe() -> Option<isize> {
    use windows::Win32::Storage::FileSystem::PIPE_ACCESS_INBOUND;
    use windows::Win32::System::Pipes::{CreateNamedPipeW, PIPE_READMODE_MESSAGE, PIPE_TYPE_MESSAGE, PIPE_WAIT};
    unsafe {
        let h = CreateNamedPipeW(PIPE_NAME, PIPE_ACCESS_INBOUND, PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT, 1, 0, PIPE_BUFFER, 0, None);
        (!h.is_invalid()).then_some(h.0 as isize)
    }
}

fn read_loop(pipe: isize, on_change: impl Fn()) {
    use windows::Win32::Foundation::{GetLastError, ERROR_PIPE_CONNECTED, HANDLE};
    use windows::Win32::Storage::FileSystem::ReadFile;
    use windows::Win32::System::Pipes::{ConnectNamedPipe, DisconnectNamedPipe};
    let handle = HANDLE(pipe as *mut _);
    let mut buf = vec![0u8; PIPE_BUFFER as usize];
    loop {
        unsafe {
            if ConnectNamedPipe(handle, None).is_err() && GetLastError() != ERROR_PIPE_CONNECTED {
                std::thread::sleep(std::time::Duration::from_millis(RETRY_MS));
                continue;
            }
        }
        CONNECTED.store(true, Ordering::Relaxed);
        replay_icons();
        loop {
            let mut n = 0u32;
            if unsafe { ReadFile(handle, Some(&mut buf), Some(&mut n), None) }.is_err() {
                break;
            }
            if let Some(packet) = parse(&buf[..n as usize]) {
                if set_icons(|icons| {
                    let changed = apply(icons, packet);
                    fill_exes(icons);
                    changed
                }) {
                    on_change();
                }
            }
        }
        CONNECTED.store(false, Ordering::Relaxed);
        unsafe {
            let _ = DisconnectNamedPipe(handle);
        }
        if set_icons(|icons| !std::mem::take(icons).is_empty()) {
            on_change();
        }
    }
}

// El ejecutable dueño se resuelve una vez por ícono; sin permiso para leerlo (proceso elevado) cae al nombre de la clase de ventana.
fn fill_exes(icons: &mut [Icon]) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;
    for i in icons.iter_mut().filter(|i| i.exe.is_empty()) {
        let h = HWND(i.hwnd as usize as *mut _);
        let path = unsafe { crate::dock::exe_of(h) };
        i.exe = match path.rsplit(['\\', '/']).next().filter(|n| !n.is_empty()) {
            Some(name) => name.to_lowercase(),
            None => {
                let mut buf = [0u16; 256];
                let n = unsafe { GetClassNameW(h, &mut buf) } as usize;
                String::from_utf16_lossy(&buf[..n]).to_lowercase()
            }
        };
    }
}

// Los íconos que ya existían no se anunciaron: `TaskbarCreated` hace que cada app vuelva a agregar el suyo.
fn replay_icons() {
    use windows::core::w;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{RegisterWindowMessageW, SendNotifyMessageW, HWND_BROADCAST};
    unsafe {
        let m = RegisterWindowMessageW(w!("TaskbarCreated"));
        if m != 0 {
            let _ = SendNotifyMessageW(HWND_BROADCAST, m, WPARAM(0), LPARAM(0));
        }
    }
}

// Mientras el pipe no esté conectado (explorer reiniciado, DLL todavía descargándose) vuelve a enganchar el hook;
// con el pipe vivo solo retira los íconos cuya ventana desapareció sin avisar.
fn inject_loop(dll: std::path::PathBuf, on_change: impl Fn()) {
    loop {
        if CONNECTED.load(Ordering::Relaxed) {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::WindowsAndMessaging::IsWindow;
            if set_icons(|icons| {
                let n = icons.len();
                icons.retain(|i| unsafe { IsWindow(HWND(i.hwnd as usize as *mut _)).as_bool() });
                icons.len() != n
            }) {
                on_change();
            }
        } else {
            unsafe { hook(&dll) };
        }
        std::thread::sleep(std::time::Duration::from_millis(RETRY_MS));
    }
}

unsafe fn hook(dll: &std::path::Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{s, PCWSTR};
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::Foundation::FreeLibrary;
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowThreadProcessId, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, WH_GETMESSAGE};
    let old = HOOK.swap(0, Ordering::Relaxed);
    if old != 0 {
        let _ = UnhookWindowsHookEx(HHOOK(old as *mut _));
    }
    let Ok(tray) = FindWindowW(windows::core::w!("Shell_TrayWnd"), PCWSTR::null()) else { return };
    let tid = GetWindowThreadProcessId(tray, None);
    let wide: Vec<u16> = dll.as_os_str().encode_wide().chain(Some(0)).collect();
    let Ok(module) = LoadLibraryW(PCWSTR(wide.as_ptr())) else { return };
    if let (Some(proc), true) = (GetProcAddress(module, s!("GetMsgProc")), tid != 0) {
        if let Ok(h) = SetWindowsHookExW(WH_GETMESSAGE, Some(std::mem::transmute(proc)), module, tid) {
            HOOK.store(h.0 as isize, Ordering::Relaxed);
            let _ = PostThreadMessageW(tid, 0, WPARAM(0), LPARAM(0));
        }
    }
    let _ = FreeLibrary(module);
}

#[cfg(test)]
#[path = "tray_tests.rs"]
mod tests;
