// Bandeja del sistema dentro del dock. Windows 11 no expone los bitmaps de los íconos de bandeja;
// `YASBTrayHook.dll` (MIT, firmada, la del YASB pinneado) se carga en explorer con un hook WH_GETMESSAGE, hace subclass de
// `Shell_TrayWnd` y reenvía cada `Shell_NotifyIcon` por un pipe de mensajes. La shell es el servidor del pipe; los clics
// van directo a la ventana dueña del ícono. Sin la DLL, o con YASB corriendo (dueño del mutex), la shell no hace nada y
// el botón de bandeja del dock sigue abriendo la bandeja nativa.
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::collections::VecDeque;
use std::sync::Mutex;

const NIF_MESSAGE: u32 = 0x1;
const NIF_ICON: u32 = 0x2;
const NIF_TIP: u32 = 0x4;
const NIF_STATE: u32 = 0x8;
const NIF_INFO: u32 = 0x10;
const NIF_REALTIME: u32 = 0x40;
const NIS_HIDDEN: u32 = 0x1;
const NIM_ADD: u32 = 0;
const NIM_MODIFY: u32 = 1;
const NIM_DELETE: u32 = 2;
const NIM_SETVERSION: u32 = 4;
const NIF_GUID: u32 = 0x20;
const NIIF_USER: u32 = 0x4;
const PACKET_ICON: u32 = 2;
const HEADER: usize = 28;
const MAX_SIDE: u32 = 256;

const WM_LBUTTONDOWN: u32 = 0x201;
const WM_LBUTTONUP: u32 = 0x202;
const WM_LBUTTONDBLCLK: u32 = 0x203;
const WM_RBUTTONDOWN: u32 = 0x204;
const WM_RBUTTONUP: u32 = 0x205;
const WM_MBUTTONDOWN: u32 = 0x207;
const WM_MBUTTONUP: u32 = 0x208;
const WM_CONTEXTMENU: u32 = 0x7B;
const NIN_SELECT: u32 = 0x400;
const NIN_BALLOONHIDE: u32 = 0x403;

#[derive(Clone, PartialEq, Debug)]
pub struct Icon {
    pub hwnd: u32,
    pub uid: u32,
    pub guid: [u8; 16],
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
    pub guid: [u8; 16],
    pub flags: u32,
    pub callback: u32,
    pub version: u32,
    pub tip: String,
    pub info: String,
    pub info_title: String,
    pub info_flags: u32,
    pub timeout_ms: u32,
    pub state: u32,
    pub mask: u32,
    pub icon: Option<(u32, u32, Vec<u8>)>,
    pub balloon_icon: Option<(u32, u32, Vec<u8>)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BalloonRequest {
    pub hwnd: u32,
    pub uid: u32,
    pub guid: [u8; 16],
    pub callback: u32,
    pub version: u32,
    pub title: String,
    pub text: String,
    pub flags: u32,
    pub timeout_ms: u32,
    pub realtime: bool,
    pub sequence: u64,
    pub icon: Option<(u32, u32, Vec<u8>)>,
}

#[derive(Default)]
pub struct BalloonQueue {
    current: Option<BalloonRequest>,
    pending: VecDeque<BalloonRequest>,
}

impl BalloonQueue {
    pub fn submit(&mut self, request: BalloonRequest) -> bool {
        if request.text.is_empty() {
            return false;
        }
        if self.current.is_none() {
            self.current = Some(request);
            return true;
        }
        if request.realtime || self.pending.len() >= 32 {
            return false;
        }
        self.pending.push_back(request);
        true
    }

    #[cfg(test)]
    pub fn finish(&mut self) -> Option<BalloonRequest> {
        self.current.take();
        self.current = self.pending.pop_front();
        self.current.clone()
    }

    pub fn finish_if(&mut self, expected: &BalloonRequest) -> Option<(BalloonRequest, Option<BalloonRequest>)> {
        if self.current.as_ref()?.sequence != expected.sequence { return None; }
        let completed = self.current.take()?;
        self.current = self.pending.pop_front();
        Some((completed, self.current.clone()))
    }

    pub fn update_callback(&mut self, hwnd: u32, uid: u32, guid: [u8; 16], callback: Option<u32>, version: Option<u32>) {
        let matches = |r: &BalloonRequest| if guid != [0; 16] { r.guid == guid } else { r.guid == [0; 16] && r.hwnd == hwnd && r.uid == uid };
        for request in self.current.iter_mut().chain(self.pending.iter_mut()).filter(|r| matches(r)) {
            if let Some(callback) = callback { request.callback = callback; }
            if let Some(version) = version { request.version = version; }
        }
    }

    pub fn cancel(&mut self, hwnd: u32, uid: u32, guid: [u8; 16]) -> (Option<BalloonRequest>, Option<BalloonRequest>) {
        let matches = |r: &BalloonRequest| if guid != [0; 16] { r.guid == guid } else { r.guid == [0; 16] && r.hwnd == hwnd && r.uid == uid };
        let removed = if self.current.as_ref().is_some_and(matches) { self.current.take() } else { None };
        self.pending.retain(|r| !matches(r));
        let next = if removed.is_some() {
            self.current = self.pending.pop_front();
            self.current.clone()
        } else {
            None
        };
        (removed, next)
    }

    pub fn current(&self) -> Option<&BalloonRequest> {
        self.current.as_ref()
    }

    #[cfg(test)]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
}

fn utf16_field(data: &[u8], start: usize, len: usize) -> String {
    let units = data.get(start..start.saturating_add(len))
        .unwrap_or_default()
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|u| *u != 0)
        .collect::<Vec<_>>();
    String::from_utf16_lossy(&units)
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Gesture {
    Left,
    LeftDouble,
    Right,
    Middle,
}

fn u32_at(b: &[u8], off: usize) -> u32 {
    b.get(off..off + 4).map_or(0, |s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

// Una trama del hook: cabecera `=IQIIII` (tipo, dwData, cbData, ancho, alto, bytes de ícono), el SHELLTRAYDATA crudo y los píxeles BGRA.
pub fn parse_shell_notify(data: &[u8], icon: Option<(u32, u32, Vec<u8>)>, balloon_icon: Option<(u32, u32, Vec<u8>)>) -> Option<Packet> {
    if data.len() < 964 || data.len() > 4096 || u32_at(data, 0) != 0x34753423 {
        return None;
    }
    let (message, flags) = (u32_at(data, 4), u32_at(data, 20));
    if !matches!(message, NIM_ADD | NIM_MODIFY | NIM_DELETE | NIM_SETVERSION) {
        return None;
    }
    let tip = if flags & NIF_TIP != 0 { utf16_field(data, 32, 256) } else { String::new() };
    Some(Packet {
        message, hwnd: u32_at(data, 12), uid: u32_at(data, 16),
        guid: if flags & NIF_GUID != 0 { data[944..960].try_into().unwrap() } else { [0; 16] },
        flags, callback: u32_at(data, 24), version: u32_at(data, 808), tip,
        info: if flags & NIF_INFO != 0 { utf16_field(data, 296, 512) } else { String::new() },
        info_title: if flags & NIF_INFO != 0 { utf16_field(data, 812, 128) } else { String::new() },
        info_flags: if flags & NIF_INFO != 0 { u32_at(data, 940) } else { 0 },
        timeout_ms: if flags & NIF_INFO != 0 { u32_at(data, 808) } else { 0 },
        state: u32_at(data, 288), mask: u32_at(data, 292), icon, balloon_icon,
    })
}

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
    let tip = if flags & NIF_TIP != 0 { utf16_field(payload, 32, 256) } else { String::new() };
    let info = if flags & NIF_INFO != 0 { utf16_field(payload, 296, 512) } else { String::new() };
    let info_title = if flags & NIF_INFO != 0 { utf16_field(payload, 812, 128) } else { String::new() };
    let info_flags = if flags & NIF_INFO != 0 { u32_at(payload, 940) } else { 0 };
    let timeout_ms = if flags & NIF_INFO != 0 { u32_at(payload, 808) } else { 0 };
    let balloon_icon = if flags & NIF_INFO != 0 && info_flags & 0xF == NIIF_USER && payload.len() >= 964 { icon_pixels(u32_at(payload, 960)) } else { None };
    let pixels = data.get(HEADER + cb..HEADER + cb + icon_len);
    let icon = pixels.filter(|p| w > 0 && h > 0 && w <= MAX_SIDE && h <= MAX_SIDE && p.len() == (w * h * 4) as usize).map(|p| (w, h, p.to_vec()));
    Some(Packet { message: u32_at(payload, 4), hwnd: u32_at(payload, 12), uid: u32_at(payload, 16), guid: if flags & NIF_GUID != 0 && payload.len() >= 960 { payload[944..960].try_into().unwrap() } else { [0; 16] }, flags, callback: u32_at(payload, 24), version: u32_at(payload, 808), tip, info, info_title, info_flags, timeout_ms, state: u32_at(payload, 288), mask: u32_at(payload, 292), icon, balloon_icon })
}

// Aplica una trama a la lista; devuelve si algo cambió. `uVersion` solo cuenta en NIM_SETVERSION: en NIM_MODIFY comparte lugar con el timeout.
pub fn apply(icons: &mut Vec<Icon>, p: Packet) -> bool {
    let at = icons.iter().position(|i| if p.guid != [0; 16] { i.guid == p.guid } else { i.guid == [0; 16] && i.hwnd == p.hwnd && i.uid == p.uid });
    if p.message != NIM_ADD && at.is_none() {
        return false;
    }
    match p.message {
        NIM_ADD | NIM_MODIFY | NIM_SETVERSION => {
            let before = at.map(|i| icons[i].clone());
            let i = at.unwrap_or_else(|| {
                icons.push(Icon { hwnd: p.hwnd, uid: p.uid, guid: p.guid, callback: 0, version: 0, tip: String::new(), hidden: false, exe: String::new(), size: (0, 0), bgra: Vec::new() });
                icons.len() - 1
            });
            let icon = &mut icons[i];
            icon.hwnd = p.hwnd;
            icon.uid = p.uid;
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
        // varias apps (clientes de torrents, VPN) solo abren con el doble clic
        Gesture::LeftDouble => vec![WM_LBUTTONDBLCLK, WM_LBUTTONUP],
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
static BALLOONS: Mutex<BalloonQueue> = Mutex::new(BalloonQueue { current: None, pending: VecDeque::new() });
static REVISION: AtomicU64 = AtomicU64::new(0);
static NEXT_BALLOON_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static CONNECTED: AtomicBool = AtomicBool::new(false);
static NATIVE_ACTIVE: AtomicBool = AtomicBool::new(false);
static HOOK: AtomicIsize = AtomicIsize::new(0);

pub fn active() -> bool {
    CONNECTED.load(Ordering::Relaxed) || NATIVE_ACTIVE.load(Ordering::Relaxed)
}

pub fn set_native_active(active: bool) {
    NATIVE_ACTIVE.store(active, Ordering::Relaxed);
}

pub fn revision() -> u64 {
    REVISION.load(Ordering::Relaxed)
}

pub fn current_balloon() -> Option<BalloonRequest> {
    BALLOONS.lock().unwrap().current().cloned()
}

pub fn finish_balloon_if(expected: &BalloonRequest) -> Option<(BalloonRequest, Option<BalloonRequest>)> {
    BALLOONS.lock().unwrap().finish_if(expected)
}

pub fn cancel_balloon(hwnd: u32, uid: u32, guid: [u8; 16]) -> (Option<BalloonRequest>, Option<BalloonRequest>) {
    let result = BALLOONS.lock().unwrap().cancel(hwnd, uid, guid);
    result
}

// Identidad estable de un ícono entre sesiones: el ejecutable dueño y su id.
pub fn key(i: &Icon) -> String {
    if i.guid != [0; 16] {
        format!("guid:{}", i.guid.iter().map(|b| format!("{b:02x}")).collect::<String>())
    } else {
        format!("{}:{}", i.exe, i.uid)
    }
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

fn retain_live_icons(icons: &mut Vec<Icon>, mut is_window: impl FnMut(u32) -> bool) -> bool {
    let count = icons.len();
    icons.retain(|icon| is_window(icon.hwnd));
    icons.len() != count
}

fn cleanup_stale() -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::IsWindow;
    set_icons(|icons| retain_live_icons(icons, |hwnd| unsafe { IsWindow(HWND(hwnd as usize as *mut _)).as_bool() }))
}

pub fn spawn_cleanup(on_change: impl Fn() + Send + 'static) -> bool {
    std::thread::Builder::new().name("tray-cleanup".into()).spawn(move || loop {
        if cleanup_stale() { on_change(); }
        std::thread::sleep(std::time::Duration::from_millis(RETRY_MS));
    }).is_ok()
}

pub fn ingest_shell_notify(data: &[u8]) -> bool {
    let icon = if u32_at(data, 20) & NIF_ICON != 0 { icon_pixels(u32_at(data, 28)) } else { None };
    let flags = u32_at(data, 20);
    let balloon_icon = if flags & NIF_INFO != 0 && u32_at(data, 940) & 0xF == NIIF_USER { icon_pixels(u32_at(data, 960)) } else { None };
    let Some(packet) = parse_shell_notify(data, icon, balloon_icon) else { return false };
    ingest_packet(packet)
}

fn carries_balloon(message: u32, flags: u32) -> bool {
    matches!(message, NIM_ADD | NIM_MODIFY) && flags & NIF_INFO != 0
}

fn ingest_packet(packet: Packet) -> bool {
    let (message, hwnd, uid, guid, flags) = (packet.message, packet.hwnd, packet.uid, packet.guid, packet.flags);
    let notification = carries_balloon(message, flags).then(|| (packet.info_title.clone(), packet.info.clone(), packet.info_flags, packet.timeout_ms, packet.balloon_icon.clone()));
    let callback_update = (flags & NIF_MESSAGE != 0).then_some(()).or((message == NIM_SETVERSION).then_some(()));
    let (icon_changed, balloon, callback, version) = {
        let mut icons = ICONS.lock().unwrap();
        let changed = apply(&mut icons, packet);
        fill_exes(&mut icons);
        let balloon = if message == NIM_DELETE {
            None
        } else if let Some((title, text, info_flags, timeout_ms, icon)) = notification {
            icons.iter().find(|i| if guid != [0; 16] { i.guid == guid } else { i.guid == [0; 16] && i.hwnd == hwnd && i.uid == uid }).map(|i| BalloonRequest {
                hwnd, uid, guid, callback: i.callback, version: i.version,
                title, text, flags: info_flags, timeout_ms, realtime: flags & NIF_REALTIME != 0,
                sequence: NEXT_BALLOON_SEQUENCE.fetch_add(1, Ordering::Relaxed), icon,
            })
        } else {
            None
        };
        let metadata = if callback_update.is_some() {
            icons.iter().find(|i| if guid != [0; 16] { i.guid == guid } else { i.guid == [0; 16] && i.hwnd == hwnd && i.uid == uid })
                .map(|icon| ((flags & NIF_MESSAGE != 0).then_some(icon.callback), (message == NIM_SETVERSION).then_some(icon.version)))
        } else { None };
        if changed { REVISION.fetch_add(1, Ordering::Relaxed); }
        (changed, balloon, metadata.map(|m| m.0).flatten(), metadata.map(|m| m.1).flatten())
    };
    if callback_update.is_some() {
        BALLOONS.lock().unwrap().update_callback(hwnd, uid, guid, callback, version);
    }
    let has_balloon = balloon.is_some();
    let (removed, next) = if message == NIM_DELETE || balloon.as_ref().is_some_and(|b| b.text.is_empty()) {
        cancel_balloon(hwnd, uid, guid)
    } else if let Some(balloon) = balloon {
        enqueue_balloon(balloon);
        (None, None)
    } else {
        (None, None)
    };
    let balloon_changed = removed.is_some() || next.is_some();
    if let Some(removed) = &removed { notify_balloon_owner(removed, NIN_BALLOONHIDE); }
    if removed.is_some() { crate::tray_notifications::dismiss(); }
    if next.is_some() { crate::tray_notifications::start(); }
    icon_changed || balloon_changed || has_balloon
}

fn enqueue_balloon(balloon: BalloonRequest) {
    let availability = crate::tray_notifications::availability(&balloon);
    if availability == crate::tray_notifications::Availability::Hide {
        notify_balloon_owner(&balloon, NIN_BALLOONHIDE);
    }
    let became_current = if matches!(availability, crate::tray_notifications::Availability::Hide | crate::tray_notifications::Availability::Discard) {
        false
    } else {
        let mut queue = BALLOONS.lock().unwrap();
        let was_idle = queue.current().is_none();
        queue.submit(balloon) && was_idle
    };
    if became_current { crate::tray_notifications::start(); }
}

// Toast de Windows sin ícono de bandeja dueño: no hay ventana a la que devolver eventos (callback 0).
// `open` es lo que abre el clic (ruta o shell:AppsFolder\AUMID); la tabla se limpia al terminar el toast.
pub fn submit_toast(title: String, text: String, timeout_ms: u32, open: Option<String>) {
    let sequence = NEXT_BALLOON_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    if let Some(open) = open {
        toast_targets().lock().unwrap().insert(sequence, open);
    }
    enqueue_balloon(BalloonRequest {
        hwnd: 0, uid: 0, guid: [0; 16], callback: 0, version: 4,
        title, text, flags: 0, timeout_ms, realtime: false,
        sequence, icon: None,
    });
}

fn toast_targets() -> &'static std::sync::Mutex<std::collections::HashMap<u64, String>> {
    static T: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<u64, String>>> = std::sync::OnceLock::new();
    T.get_or_init(Default::default)
}

pub fn notify_balloon_owner(balloon: &BalloonRequest, event: u32) {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, GetWindowThreadProcessId, IsWindow, SendNotifyMessageW};
    if balloon.callback == 0 {
        if balloon.hwnd == 0 && balloon.uid == 0 {
            let target = {
                let mut t = toast_targets().lock().unwrap();
                if event == 0x402 { None } else { t.remove(&balloon.sequence).filter(|_| event == 0x405) }
            };
            if let Some(open) = target { crate::dock_menu::launch(&open, ""); }
        }
        return;
    }
    let hwnd = HWND(balloon.hwnd as usize as *mut _);
    unsafe {
        if !IsWindow(hwnd).as_bool() { return; }
        if event == 0x405 {
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let _ = AllowSetForegroundWindow(pid);
        }
        let (w, l) = if balloon.version >= 4 { (0, (((balloon.uid & 0xFFFF) << 16) | (event & 0xFFFF)) as isize) } else { (balloon.uid as usize, event as isize) };
        let _ = SendNotifyMessageW(hwnd, balloon.callback, WPARAM(w), LPARAM(l));
    }
}

fn icon_pixels(handle: u32) -> Option<(u32, u32, Vec<u8>)> {
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::UI::WindowsAndMessaging::{CopyIcon, DestroyIcon, DrawIconEx, GetIconInfo, DI_NORMAL, HICON, ICONINFO};
    if handle == 0 { return None; }
    unsafe {
        let icon = CopyIcon(HICON(handle as usize as *mut _)).ok()?;
        let mut info = ICONINFO::default();
        if GetIconInfo(icon, &mut info).is_err() { let _ = DestroyIcon(icon); return None; }
        let bitmap = if !info.hbmColor.is_invalid() { info.hbmColor } else { info.hbmMask };
        let mut dimensions = BITMAP::default();
        let read = GetObjectW(bitmap, std::mem::size_of::<BITMAP>() as i32, Some(&mut dimensions as *mut _ as *mut _));
        let width = dimensions.bmWidth;
        let height = if info.hbmColor.is_invalid() { dimensions.bmHeight / 2 } else { dimensions.bmHeight };
        if !info.hbmColor.is_invalid() { let _ = DeleteObject(info.hbmColor); }
        if !info.hbmMask.is_invalid() { let _ = DeleteObject(info.hbmMask); }
        if read == 0 || width <= 0 || height <= 0 || width > MAX_SIDE as i32 || height > MAX_SIDE as i32 {
            let _ = DestroyIcon(icon);
            return None;
        }
        let (w, h) = (width as u32, height as u32);
        let len = w.checked_mul(h)?.checked_mul(4)? as usize;
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() { let _ = DestroyIcon(icon); return None; }
        let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: width, biHeight: -height, biPlanes: 1, biBitCount: 32, ..Default::default() }, ..Default::default() };
        let mut bits = std::ptr::null_mut();
        let Ok(dib) = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else {
            let _ = DeleteDC(dc); let _ = DestroyIcon(icon); return None;
        };
        let previous = SelectObject(dc, dib);
        let drawn = DrawIconEx(dc, 0, 0, icon, width, height, 0, None, DI_NORMAL).is_ok();
        let pixels = if drawn && !bits.is_null() { std::slice::from_raw_parts(bits as *const u8, len).to_vec() } else { Vec::new() };
        let _ = SelectObject(dc, previous);
        let _ = DeleteObject(dib);
        let _ = DeleteDC(dc);
        let _ = DestroyIcon(icon);
        (!pixels.is_empty()).then_some((w, h, pixels))
    }
}

pub fn click_identity(hwnd: u32, uid: u32, guid: [u8; 16], gesture: Gesture, at: (i32, i32)) {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, GetWindowThreadProcessId, IsWindow, SendNotifyMessageW};
    let Some((callback, version)) = ICONS.lock().unwrap().iter().find(|i| if guid != [0; 16] { i.guid == guid } else { i.guid == [0; 16] && i.hwnd == hwnd && i.uid == uid }).map(|i| (i.callback, i.version)) else { return };
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
                // Un cliente que conectó y cerró antes de este punto deja el pipe en cierre hasta que se desconecte.
                let _ = DisconnectNamedPipe(handle);
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
            if cleanup_stale() {
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
