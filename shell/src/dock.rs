// Dock de la shell: una ventana propia con las apps abiertas. La geometría es pura (`layout`) y lo demás es Win32.
use crate::apps::{self, Foreground, WinAttrs};
use crate::komorebi;
use crate::layout::{px, R};
use crate::theme::{rgb, Theme};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::os::windows::process::CommandExt;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::RemoteDesktop::{WTSRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION};
use windows::Win32::System::Threading::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const CELL_DIP: i32 = 40;
const PAD_DIP: i32 = 8;
pub const BAR_DIP: i32 = 48;
const MARGIN_DIP: i32 = 6;
const DOT_DIP: i32 = 4;
const MAX_ICONS: usize = 64;
// Altura que komorebi reserva: alto del dock + su margen + los 2 px de la franja de la taskbar nativa.
#[allow(dead_code)]
pub const RESERVE_DIP: i32 = BAR_DIP + MARGIN_DIP + 2;

type MonRect = (i32, i32, i32, i32);

// Personalización del usuario (`user.json`), acotada: la altura del dock no cambia, así que la reserva de komorebi sigue valiendo.
pub struct Style {
    pub icon: i32,
    pub radius: i32,
    pub gap: i32,
    pub opacity: f32,
}

pub fn style(t: &Theme) -> Style {
    Style { icon: t.dock_icon.clamp(16, 32), radius: t.dock_radius.clamp(0, 24), gap: t.dock_gap.clamp(0, 12), opacity: t.dock_opacity.unwrap_or(t.opacity).clamp(0.4, 1.0) }
}

const HOVER_MS: f32 = 120.0;
const ENTRY_MS: f32 = 220.0;
const FADE_MS: f32 = 160.0;

pub fn approach(cur: f32, target: f32, dt_ms: f32, dur_ms: f32) -> f32 {
    let s = dt_ms / dur_ms;
    if target > cur {
        (cur + s).min(target)
    } else {
        (cur - s).max(target)
    }
}

pub fn ease(t: f32) -> f32 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u * u
}

#[derive(Debug, PartialEq)]
pub struct Layout {
    pub bar: R,
    pub cells: Vec<R>,
    pub overflow: usize,
}

#[derive(Debug, PartialEq)]
pub enum Hit {
    Item(usize),
    Overflow,
}

// El dock vive en el borde opuesto al de la isla: `bottom` es el borde del dock. `bar` va en coordenadas de pantalla, `cells` relativas a `bar`.
pub fn layout(n: usize, scale: f32, mon: MonRect, bottom: bool, gap_dip: i32) -> Layout {
    let (cell, gap, pad) = (px(CELL_DIP, scale), px(gap_dip, scale), px(PAD_DIP, scale));
    let max_w = (mon.2 - mon.0) * 4 / 5;
    let cap = (((max_w - 2 * pad + gap) / (cell + gap)).max(1)) as usize;
    let (shown, overflow) = if n <= cap { (n, 0) } else { (cap - 1, n - (cap - 1)) };
    let slots = shown + usize::from(overflow > 0);
    let w = 2 * pad + slots as i32 * cell + (slots as i32 - 1).max(0) * gap;
    let h = px(BAR_DIP, scale);
    let x = mon.0 + (mon.2 - mon.0 - w) / 2;
    let y = if bottom { mon.3 - px(MARGIN_DIP, scale) - h } else { mon.1 + px(MARGIN_DIP, scale) };
    let cells = (0..slots).map(|i| R { x: pad + i as i32 * (cell + gap), y: (h - cell) / 2, w: cell, h: cell }).collect();
    Layout { bar: R { x, y, w, h }, cells, overflow }
}

pub fn hit_test(l: &Layout, x: i32, y: i32) -> Option<Hit> {
    let (rx, ry) = (x - l.bar.x, y - l.bar.y);
    let i = l.cells.iter().position(|c| rx >= c.x && rx < c.x + c.w && ry >= c.y && ry < c.y + c.h)?;
    Some(if l.overflow > 0 && i == l.cells.len() - 1 { Hit::Overflow } else { Hit::Item(i) })
}

// Ventana a enfocar al hacer clic: con el grupo ya activo, la siguiente; si no, la última activa del grupo.
pub fn pick_target(wins: &[isize], foreground: isize, last_active: Option<isize>) -> Option<isize> {
    if let Some(i) = wins.iter().position(|w| *w == foreground) {
        return (wins.len() > 1).then(|| wins[(i + 1) % wins.len()]).or(Some(wins[i]));
    }
    last_active.filter(|l| wins.contains(l)).or_else(|| wins.first().copied())
}

// Argumentos de `komorebic`: solo índices numéricos, nunca texto de la ventana.
pub fn focus_args(monitor: usize, workspace: usize) -> [String; 3] {
    ["focus-monitor-workspace".into(), monitor.to_string(), workspace.to_string()]
}

// Anclados primero y en su orden; un anclado cuyo exe ya corre se fusiona con ese grupo en vez de duplicarse.
// Anclado propio del dock (`state/dock-pins.json`). `id` identifica el anclado; `open` es lo que se lanza con `args`.
#[derive(Clone, Serialize, Deserialize)]
struct Pin {
    id: String,
    open: String,
    #[serde(default)]
    args: String,
    #[serde(default)]
    exe: String,
    #[serde(default)]
    aumid: Option<String>,
    // Squirrel (Discord, Slack, Teams): se lanza Update.exe y el exe real va en `--processStart X.exe`.
    #[serde(default)]
    launched: Option<String>,
}

// Anclado para un grupo en ejecución. UWP se lanza por AUMID; Squirrel por su Update.exe, porque la carpeta `app-X` cambia con cada versión.
fn pin_for(it: &Item, has_update: impl Fn(&std::path::Path) -> bool) -> Option<Pin> {
    let exe = std::path::Path::new(&it.exe);
    let file = exe.file_name()?.to_string_lossy().into_owned();
    let base = |open: String, args: String, exe: String, launched: Option<String>| Pin { id: it.key.clone(), open, args, exe, aumid: it.aumid.clone(), launched };
    if file.eq_ignore_ascii_case("applicationframehost.exe") {
        let a = it.aumid.clone()?;
        return Some(Pin { id: it.key.clone(), open: format!(r"shell:AppsFolder\{a}"), args: String::new(), exe: String::new(), aumid: Some(a), launched: None });
    }
    if let Some(root) = exe.parent().filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("app-"))).and_then(|p| p.parent()) {
        let upd = root.join("Update.exe");
        if has_update(&upd) {
            let u = upd.to_string_lossy().into_owned();
            return Some(base(u.clone(), format!("--processStart {file}"), u, Some(file.to_lowercase())));
        }
    }
    Some(base(it.exe.clone(), String::new(), it.exe.clone(), None))
}

fn launch_of(pins: &[Pin], it: &Item) -> Option<(String, String)> {
    match &it.pin {
        Some(id) => pins.iter().find(|p| &p.id == id).map(|p| (p.open.clone(), p.args.clone())),
        None => pin_for(it, |p| p.exists()).map(|p| (p.open, p.args)),
    }
}

fn squirrel_target(exe: &str, args: &str) -> Option<String> {
    if !std::path::Path::new(exe).file_name()?.to_string_lossy().eq_ignore_ascii_case("update.exe") {
        return None;
    }
    let mut it = args.split_whitespace();
    it.find(|a| a.eq_ignore_ascii_case("--processStart") || a.eq_ignore_ascii_case("--processStartAndWait"))?;
    it.next().map(|n| n.trim_matches('"').to_lowercase())
}

fn arrange(pins: &[Pin], mut groups: Vec<Item>) -> Vec<Item> {
    let mut out = Vec::new();
    for Pin { id, exe, aumid, launched, .. } in pins {
        let same = |g: &Item| {
            (!exe.is_empty() && g.exe.eq_ignore_ascii_case(exe))
                || launched.as_deref().is_some_and(|n| std::path::Path::new(&g.exe).file_name().is_some_and(|f| f.to_string_lossy().eq_ignore_ascii_case(n)))
                || matches!((&g.aumid, aumid), (Some(a), Some(b)) if a.eq_ignore_ascii_case(b))
        };
        match groups.iter().position(same) {
            Some(i) => {
                let mut g = groups.remove(i);
                g.pin = Some(id.clone());
                out.push(g);
            }
            None => out.push(Item { key: format!("pin:{}", id.to_lowercase()), exe: exe.clone(), aumid: aumid.clone(), wins: Vec::new(), active: false, other_ws: false, pin: Some(id.clone()) }),
        }
    }
    out.extend(groups);
    out
}

const WM_ICON: u32 = WM_APP + 21;
const T_REFRESH: usize = 1;
const T_TICK: usize = 2;
const T_FOCUS: usize = 3;
const T_ANIM: usize = 4;
const ANIM_MS: u32 = 16;
const TRAY_SECS: u64 = 12;
const REFRESH_MS: u32 = 60;
const FOCUS_DELAY_MS: u32 = 220;

#[derive(Clone, PartialEq)]
struct Item {
    key: String,
    exe: String,
    aumid: Option<String>,
    wins: Vec<isize>,
    active: bool,
    other_ws: bool,
    pin: Option<String>,
}

struct Icon {
    w: i32,
    h: i32,
    px: Vec<u32>,
}

struct Dock {
    hwnd: HWND,
    tip: HWND,
    theme: Theme,
    items: Vec<Item>,
    order: Vec<String>,
    last_active: HashMap<String, isize>,
    icons: HashMap<(String, i32), Icon>,
    asked: HashSet<(String, i32)>,
    layout: Layout,
    hover: Option<usize>,
    tracking: bool,
    scale: f32,
    mon: MonRect,
    sig: u64,
    hidden: bool,
    pending_focus: Option<isize>,
    pins: Vec<Pin>,
    ident: HashMap<isize, (String, Option<String>, std::time::Instant)>,
    pins_path: std::path::PathBuf,
    pins_stamp: Option<std::time::SystemTime>,
    shell_msg: u32,
    created_msg: u32,
    motion: bool,
    hov: HashMap<String, f32>,
    born: HashMap<String, f32>,
    known: HashSet<String>,
    seeded: bool,
    pressed: Option<usize>,
    vis: f32,
    anim_at: std::time::Instant,
    tray_until: Option<std::time::Instant>,
    flashing: HashSet<isize>,
    drag: Option<Drag>,
}

struct Drag {
    key: String,
    x0: i32,
    moving: bool,
}


thread_local! { static DOCK: RefCell<Option<Dock>> = const { RefCell::new(None) }; }

fn with<R>(f: impl FnOnce(&mut Dock) -> R) -> R {
    DOCK.with(|d| f(d.borrow_mut().as_mut().expect("dock")))
}

// Íconos listos desde el worker, con píxeles premultiplicados.
static READY: Mutex<Vec<(String, i32, Icon)>> = Mutex::new(Vec::new());
type Ask = (String, i32, Vec<String>);
static ASKS: Mutex<Option<std::sync::mpsc::Sender<Ask>>> = Mutex::new(None);

pub fn init(theme: &Theme, state_dir: &std::path::Path) -> Option<HWND> {
    unsafe {
        let inst = GetModuleHandleW(PCWSTR::null()).ok()?;
        let class = WNDCLASSW { lpfnWndProc: Some(dock_proc), hInstance: inst.into(), hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(), lpszClassName: w!("WinarchyShellDock"), ..Default::default() };
        RegisterClassW(&class);
        let ex = WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_LAYERED;
        let hwnd = CreateWindowExW(ex, class.lpszClassName, w!(""), WS_POPUP, 0, 0, 1, 1, None, None, inst, None).ok()?;
        let r = crate::monitor_rects()[0];
        let mon = (r.left, r.top, r.right, r.bottom);
        let scale = crate::mon_scale(&r);
        let tip_class = WNDCLASSW { lpfnWndProc: Some(tip_proc), hInstance: inst.into(), lpszClassName: w!("WinarchyShellDockTip"), ..Default::default() };
        RegisterClassW(&tip_class);
        let tip = CreateWindowExW(ex | WS_EX_TRANSPARENT, tip_class.lpszClassName, w!(""), WS_POPUP, 0, 0, 1, 1, None, None, inst, None).ok()?;
        let pins_path = state_dir.join("dock-pins.json");
        let d = Dock {
            hwnd,
            tip,
            theme: theme.clone(),
            items: Vec::new(),
            order: Vec::new(),
            last_active: HashMap::new(),
            icons: HashMap::new(),
            asked: HashSet::new(),
            layout: layout(0, scale, mon, theme.position != "bottom", style(theme).gap),
            hover: None,
            tracking: false,
            scale,
            mon,
            sig: 0,
            hidden: false,
            pending_focus: None,
            pins: load_pins(&pins_path),
            ident: HashMap::new(),
            pins_stamp: pins_stamp(&pins_path),
            pins_path,
            shell_msg: RegisterWindowMessageW(w!("SHELLHOOK")),
            created_msg: RegisterWindowMessageW(w!("TaskbarCreated")),
            motion: motion_enabled(theme),
            hov: HashMap::new(),
            born: HashMap::new(),
            known: HashSet::new(),
            seeded: false,
            pressed: None,
            vis: 0.0,
            anim_at: std::time::Instant::now(),
            tray_until: None,
            flashing: HashSet::new(),
            drag: None,
        };
        DOCK.with(|s| *s.borrow_mut() = Some(d));
        spawn_icon_worker(hwnd.0 as isize);
        komorebi::NOTIFY.store(hwnd.0 as isize, std::sync::atomic::Ordering::Relaxed);
        register(hwnd);
        refresh();
        SetTimer(hwnd, T_TICK, 1000, None);
        Some(hwnd)
    }
}

pub fn set_theme(theme: &Theme) {
    if DOCK.with(|d| d.borrow().is_none()) {
        return;
    }
    with(|d| {
        d.theme = theme.clone();
        d.motion = motion_enabled(theme);
        d.sig = 0;
    });
    unsafe { refresh() }
}

// El ajuste de Windows "efectos de animación" apagado manda sobre el theme.
fn motion_enabled(t: &Theme) -> bool {
    let mut on = windows::Win32::Foundation::BOOL(1);
    unsafe {
        let _ = SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, Some(&mut on as *mut _ as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
    }
    t.dock_motion && on.as_bool()
}

static NATIVE_HIDDEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

unsafe fn for_native_taskbars(f: impl Fn(HWND)) {
    for class in [w!("Shell_TrayWnd"), w!("Shell_SecondaryTrayWnd")] {
        let mut prev = HWND::default();
        while let Ok(h) = FindWindowExW(None, prev, class, PCWSTR::null()) {
            f(h);
            prev = h;
        }
    }
}

// Oculta la taskbar nativa (principal y secundarias): la bandeja de íconos queda inaccesible salvo `show_tray`.
unsafe fn set_native_taskbar(hide: bool) {
    NATIVE_HIDDEN.store(hide, std::sync::atomic::Ordering::Relaxed);
    for_native_taskbars(|h| {
        if hide != !IsWindowVisible(h).as_bool() {
            let _ = ShowWindow(h, if hide { SW_HIDE } else { SW_SHOWNA });
        }
    });
}

// Solo si esta shell la ocultó: una oculta por otra herramienta no se toca.
pub fn restore_native_taskbar() {
    if NATIVE_HIDDEN.load(std::sync::atomic::Ordering::Relaxed) {
        unsafe { set_native_taskbar(false) }
    }
}

// Al arrancar: una shell anterior matada a la fuerza no pudo devolverla.
pub fn restore_native_taskbar_at_start() {
    unsafe { set_native_taskbar(false) }
}

pub fn show_tray() {
    with(|d| d.tray_until = Some(std::time::Instant::now() + std::time::Duration::from_secs(TRAY_SECS)));
    unsafe { set_native_taskbar(false) }
}

unsafe fn is_elevated(h: HWND) -> bool {
    use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    let mut pid = 0u32;
    GetWindowThreadProcessId(h, Some(&mut pid));
    // Sin proceso que abrir no hay nada que bloquear; un token que no se abre con el proceso abierto es el caso típico de uno elevado.
    let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
    let mut tok = HANDLE::default();
    let mut elevated = true;
    if OpenProcessToken(p, TOKEN_QUERY, &mut tok).is_ok() {
        let (mut e, mut n) = (TOKEN_ELEVATION::default(), 0u32);
        if GetTokenInformation(tok, TokenElevation, Some(&mut e as *mut _ as *mut _), std::mem::size_of::<TOKEN_ELEVATION>() as u32, &mut n).is_ok() {
            elevated = e.TokenIsElevated != 0;
        }
        let _ = CloseHandle(tok);
    }
    let _ = CloseHandle(p);
    elevated && !self_elevated()
}

fn self_elevated() -> bool {
    use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    unsafe {
        let mut tok = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut tok).is_err() {
            return false;
        }
        let (mut e, mut n) = (TOKEN_ELEVATION::default(), 0u32);
        let ok = GetTokenInformation(tok, TokenElevation, Some(&mut e as *mut _ as *mut _), std::mem::size_of::<TOKEN_ELEVATION>() as u32, &mut n).is_ok() && e.TokenIsElevated != 0;
        let _ = CloseHandle(tok);
        ok
    }
}

unsafe fn start_anim(h: HWND) {
    with(|d| d.anim_at = std::time::Instant::now());
    SetTimer(h, T_ANIM, ANIM_MS, None);
}

// Un cuadro: acerca cada valor a su meta y vuelve a pintar; el timer se apaga solo cuando todo llegó.
unsafe fn anim_tick(h: HWND) {
    let busy = with(|d| {
        let now = std::time::Instant::now();
        let dt = (now.duration_since(d.anim_at).as_secs_f32() * 1000.0).min(50.0);
        d.anim_at = now;
        let hover_key = d.hover.and_then(|i| d.items.get(i)).map(|i| i.key.clone());
        let mut busy = false;
        for it in &d.items {
            let target = if Some(&it.key) == hover_key.as_ref() { 1.0 } else { 0.0 };
            let c = d.hov.entry(it.key.clone()).or_insert(0.0);
            let n = approach(*c, target, dt, HOVER_MS);
            busy |= n != *c;
            *c = n;
        }
        d.hov.retain(|k, _| d.items.iter().any(|i| &i.key == k));
        for p in d.born.values_mut() {
            *p = approach(*p, 1.0, dt, ENTRY_MS);
        }
        d.born.retain(|_, p| *p < 1.0);
        busy |= !d.born.is_empty();
        let target = if d.hidden || d.items.is_empty() { 0.0 } else { 1.0 };
        let v = approach(d.vis, target, dt, FADE_MS);
        busy |= v != d.vis;
        d.vis = v;
        busy
    });
    render();
    if !busy {
        let _ = KillTimer(h, T_ANIM);
    }
}

// `.lnk` de la taskbar nativa: solo siembran los anclados propios la primera vez.
fn native_pins_dir() -> Option<std::path::PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(std::path::Path::new(&appdata).join(r"Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar"))
}

fn pins_stamp(path: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

// Sin archivo se siembra con los de la taskbar nativa; desde ahí los anclados son del dock y la nativa no los pisa.
fn load_pins(path: &std::path::Path) -> Vec<Pin> {
    if let Ok(text) = std::fs::read_to_string(path) {
        return serde_json::from_str(&text).unwrap_or_default();
    }
    let seeded = load_native_pins();
    save_pins(path, &seeded);
    seeded
}

fn save_pins(path: &std::path::Path, pins: &[Pin]) {
    let Ok(text) = serde_json::to_string_pretty(pins) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

fn move_item<T>(v: &mut Vec<T>, from: usize, to: usize) {
    let x = v.remove(from);
    v.insert(to, x);
}

// Arrastrar una celda sobre otra del mismo tipo (ancladas entre sí, abiertas entre sí) las intercambia de lugar.
fn reorder(d: &mut Dock, from: &str, to: &str) -> bool {
    let (Some(a), Some(b)) = (d.items.iter().find(|i| i.key == from), d.items.iter().find(|i| i.key == to)) else { return false };
    match (a.pin.clone(), b.pin.clone()) {
        (Some(pa), Some(pb)) => {
            let (Some(ia), Some(ib)) = (d.pins.iter().position(|p| p.id == pa), d.pins.iter().position(|p| p.id == pb)) else { return false };
            move_item(&mut d.pins, ia, ib);
            save_pins(&d.pins_path, &d.pins);
            d.pins_stamp = pins_stamp(&d.pins_path);
            true
        }
        (None, None) => {
            let (Some(ia), Some(ib)) = (d.order.iter().position(|k| k == from), d.order.iter().position(|k| k == to)) else { return false };
            move_item(&mut d.order, ia, ib);
            true
        }
        _ => false,
    }
}

pub fn toggle_pin(key: &str) {
    let changed = with(|d| {
        let Some(it) = d.items.iter().find(|i| i.key == key).cloned() else { return false };
        match &it.pin {
            Some(id) => d.pins.retain(|p| &p.id != id),
            None => match pin_for(&it, |p| p.exists()) {
                Some(p) => d.pins.push(p),
                None => return false,
            },
        }
        save_pins(&d.pins_path, &d.pins);
        d.pins_stamp = pins_stamp(&d.pins_path);
        true
    });
    if changed {
        unsafe { refresh() }
    }
}

// `Taskband\Favorites` guarda los nombres de los `.lnk` anclados en UTF-16, en el orden de la taskbar.
// shortcut: se buscan cadenas `*.lnk` en el blob sin parsear su formato (puede colar un byte basura delante del nombre, por eso el match es por sufijo); si Windows lo cambia, el orden vuelve a alfabético.
fn lnk_order(blob: &[u8]) -> Vec<String> {
    let mut found: Vec<(usize, String)> = Vec::new();
    // las cadenas no están alineadas a 2 bytes: se recorren las dos paridades.
    for skew in 0..2 {
        let mut cur = String::new();
        let mut start = 0;
        let units = blob.get(skew..).unwrap_or(&[]).chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]));
        for (i, u) in units.chain(std::iter::once(0)).enumerate() {
            match char::from_u32(u as u32).filter(|c| !c.is_control()) {
                Some(c) => {
                    if cur.is_empty() {
                        start = skew + i * 2;
                    }
                    cur.push(c)
                }
                None => {
                    if cur.to_lowercase().ends_with(".lnk") {
                        found.push((start, cur.rsplit(['\\', '/']).next().unwrap_or("").to_lowercase()));
                    }
                    cur.clear();
                }
            }
        }
    }
    found.sort();
    found.into_iter().map(|(_, n)| n).collect()
}

fn taskband_blob() -> Vec<u8> {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_BINARY};
    let mut buf = vec![0u8; 16384];
    let mut len = buf.len() as u32;
    let rc = unsafe { RegGetValueW(HKEY_CURRENT_USER, w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Taskband"), w!("Favorites"), RRF_RT_REG_BINARY, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut len)) };
    if rc.is_err() {
        return Vec::new();
    }
    buf.truncate(len as usize);
    buf
}

fn sort_pins(files: &mut [std::path::PathBuf], order: &[String]) {
    let rank = |p: &std::path::PathBuf| p.file_name().and_then(|n| order.iter().position(|o| o.ends_with(&n.to_string_lossy().to_lowercase()))).unwrap_or(usize::MAX);
    files.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.cmp(b)));
}

fn load_native_pins() -> Vec<Pin> {
    use windows::core::Interface;
    use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ};
    let Some(dir) = native_pins_dir() else { return Vec::new() };
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk"))).collect();
    sort_pins(&mut files, &lnk_order(&taskband_blob()));
    let mut pins = Vec::new();
    for f in files {
        let wide: Vec<u16> = f.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
        let (exe, aumid, args) = unsafe {
            let Ok(link) = CoCreateInstance::<_, IShellLinkW>(&ShellLink, None, CLSCTX_INPROC_SERVER) else { continue };
            let Ok(file) = link.cast::<IPersistFile>() else { continue };
            if file.Load(PCWSTR(wide.as_ptr()), STGM_READ).is_err() {
                continue;
            }
            let mut buf = [0u16; 520];
            let _ = link.GetPath(&mut buf, std::ptr::null_mut(), 0);
            let n = buf.iter().position(|c| *c == 0).unwrap_or(0);
            let aumid = link.cast::<windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore>().ok().and_then(|st| variant_string(&st));
            let mut abuf = [0u16; 1024];
            let _ = link.GetArguments(&mut abuf);
            let an = abuf.iter().position(|c| *c == 0).unwrap_or(0);
            (String::from_utf16_lossy(&buf[..n]), aumid, String::from_utf16_lossy(&abuf[..an]))
        };
        // File Explorer y los de Store no traen exe: se identifican por AUMID.
        if !exe.is_empty() || aumid.is_some() {
            let launched = squirrel_target(&exe, &args);
            let lnk = f.to_string_lossy().into_owned();
            pins.push(Pin { id: lnk.to_lowercase(), open: lnk, args: String::new(), exe, aumid, launched });
        }
    }
    pins
}

unsafe fn register(hwnd: HWND) {
    let _ = RegisterShellHookWindow(hwnd);
    let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
}

unsafe fn queue_refresh(hwnd: HWND) {
    SetTimer(hwnd, T_REFRESH, REFRESH_MS, None);
}

unsafe fn is_cloaked(h: HWND) -> bool {
    let mut c = 0u32;
    DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut c as *mut _ as *mut _, 4).is_ok() && c != 0
}

unsafe fn exe_of(h: HWND) -> String {
    let mut pid = 0u32;
    GetWindowThreadProcessId(h, Some(&mut pid));
    let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return String::new() };
    let mut buf = [0u16; 520];
    let mut n = buf.len() as u32;
    let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
    let _ = CloseHandle(p);
    if ok { String::from_utf16_lossy(&buf[..n as usize]) } else { String::new() }
}

unsafe fn aumid_of(h: HWND) -> Option<String> {
    use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
    let store: IPropertyStore = SHGetPropertyStoreForWindow(h).ok()?;
    variant_string(&store)
}

fn variant_string(store: &windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore) -> Option<String> {
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
    unsafe {
        let v = store.GetValue(&PKEY_AppUserModel_ID).ok()?;
        let s = PropVariantToStringAlloc(&v).ok()?;
        let r = s.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(s.0 as *const _));
        r.filter(|a| !a.is_empty())
    }
}

unsafe fn is_eligible(h: HWND, own_pid: u32, shell: HWND) -> bool {
    if h == shell || GetWindowTextLengthW(h) == 0 {
        return false;
    }
    let ex = GetWindowLongW(h, GWL_EXSTYLE) as u32;
    let mut pid = 0u32;
    GetWindowThreadProcessId(h, Some(&mut pid));
    apps::eligible(&WinAttrs {
        visible: IsWindowVisible(h).as_bool(),
        owned: GetWindow(h, GW_OWNER).is_ok_and(|o| !o.0.is_null()),
        appwindow: ex & WS_EX_APPWINDOW.0 != 0,
        toolwindow: ex & WS_EX_TOOLWINDOW.0 != 0,
        cloaked: is_cloaked(h),
        managed: komorebi::window_slot(h.0 as isize).is_some(),
        own_process: pid == own_pid,
    })
}

unsafe extern "system" fn collect(h: HWND, data: LPARAM) -> BOOL {
    (*(data.0 as *mut Vec<HWND>)).push(h);
    BOOL(1)
}

// Re-lee las ventanas y redibuja solo si lo visible cambió.
unsafe fn refresh() {
    let mut all: Vec<HWND> = Vec::new();
    let _ = EnumWindows(Some(collect), LPARAM(&mut all as *mut _ as isize));
    let (own, shell) = (GetCurrentProcessId(), GetShellWindow());
    let fg = GetForegroundWindow().0 as isize;
    let mut groups: Vec<Item> = Vec::new();
    let eligible: Vec<HWND> = all.into_iter().filter(|h| is_eligible(*h, own, shell)).collect();
    // exe y AUMID no cambian en la vida de la ventana: se piden una vez y no en cada refresco.
    let mut ident = with(|d| std::mem::take(&mut d.ident));
    // Electron fija el AUMID tarde: una lectura sin AUMID caduca a los 5 s y se reintenta.
    let now = std::time::Instant::now();
    ident.retain(|k, v| eligible.iter().any(|h| h.0 as isize == *k) && (v.1.is_some() || now.duration_since(v.2).as_secs() < 5));
    for h in eligible {
        let (exe, aumid, _) = ident.entry(h.0 as isize).or_insert_with(|| (exe_of(h), aumid_of(h), now)).clone();
        let key = apps::group_key(aumid.as_deref(), &exe);
        let id = h.0 as isize;
        let cloaked = is_cloaked(h);
        match groups.iter_mut().find(|g| g.key == key) {
            Some(g) => {
                g.wins.push(id);
                g.active |= id == fg;
                g.other_ws &= cloaked;
            }
            None => groups.push(Item { key, exe, aumid, wins: vec![id], active: id == fg, other_ws: cloaked, pin: None }),
        }
    }
    with(|d| {
        d.ident = ident;
        d.flashing.retain(|w| *w != fg && IsWindow(HWND(*w as *mut _)).as_bool());
        d.order.retain(|k| groups.iter().any(|g| &g.key == k));
        for g in &groups {
            if !d.order.contains(&g.key) {
                d.order.push(g.key.clone());
            }
            if g.wins.contains(&fg) {
                d.last_active.insert(g.key.clone(), fg);
            }
        }
        let order = d.order.clone();
        groups.sort_by_key(|g| order.iter().position(|k| k == &g.key));
        d.items = arrange(&d.pins, groups);
        let keys: Vec<String> = d.items.iter().map(|i| i.key.clone()).collect();
        for k in keys {
            if d.known.insert(k.clone()) && d.seeded && d.motion {
                d.born.insert(k, 0.0);
            }
        }
        d.known.retain(|k| d.items.iter().any(|i| &i.key == k));
        d.seeded |= !d.items.is_empty();
    });
    render();
    if with(|d| !d.born.is_empty()) {
        start_anim(with(|d| d.hwnd));
    }
}

fn signature(d: &Dock) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for i in &d.items {
        (&i.key, i.wins.len(), i.active, i.other_ws, i.wins.iter().any(|w| d.flashing.contains(w))).hash(&mut h);
    }
    (d.hover, d.pressed, d.icons.len(), d.scale.to_bits(), d.mon, d.hidden, d.vis.to_bits()).hash(&mut h);
    (&d.theme.background, &d.theme.accent, &d.theme.position).hash(&mut h);
    (d.theme.dock_icon, d.theme.dock_radius, d.theme.dock_gap, d.theme.dock_opacity.map(f32::to_bits), d.motion).hash(&mut h);
    for i in &d.items {
        (d.hov.get(&i.key).map(|v| v.to_bits()), d.born.get(&i.key).map(|v| v.to_bits())).hash(&mut h);
    }
    h.finish()
}

pub(crate) fn blend(dst: &mut u32, src: u32) {
    let sa = src >> 24;
    if sa == 255 {
        *dst = src;
        return;
    }
    let inv = 255 - sa;
    let ch = |s: u32, d: u32| (s + d * inv / 255).min(255);
    let d = *dst;
    *dst = ch(sa, d >> 24) << 24 | ch((src >> 16) & 255, (d >> 16) & 255) << 16 | ch((src >> 8) & 255, (d >> 8) & 255) << 8 | ch(src & 255, d & 255);
}

pub(crate) fn premul(col: (u8, u8, u8), alpha: f32) -> u32 {
    let p = |c: u8| (c as f32 * alpha) as u32;
    ((alpha * 255.0) as u32) << 24 | p(col.0) << 16 | p(col.1) << 8 | p(col.2)
}

pub(crate) fn fill_round_a(buf: &mut [u32], stride: i32, rows: i32, r: R, rad: i32, col: (u8, u8, u8), alpha: f32) {
    let rad = (rad as f32).min(r.w as f32 / 2.0).min(r.h as f32 / 2.0);
    for y in r.y.max(0)..(r.y + r.h).min(rows) {
        for x in r.x.max(0)..(r.x + r.w).min(stride) {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let cx = fx.clamp(r.x as f32 + rad, (r.x + r.w) as f32 - rad);
            let cy = fy.clamp(r.y as f32 + rad, (r.y + r.h) as f32 - rad);
            let cov = (rad - ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt() + 0.5).clamp(0.0, 1.0);
            if cov > 0.0 {
                blend(&mut buf[(y * stride + x) as usize], premul(col, alpha * cov));
            }
        }
    }
}

// `sc` escala alrededor de (cx, cy) con muestreo bilineal (los píxeles ya están premultiplicados); a 1:1 copia texel por texel.
fn draw_icon(buf: &mut [u32], stride: i32, rows: i32, ic: &Icon, cx: f32, cy: f32, sc: f32, dim: f32) {
    let (w, h) = (ic.w as f32 * sc, ic.h as f32 * sc);
    let (mut left, mut top) = (cx - w / 2.0, cy - h / 2.0);
    if (sc - 1.0).abs() < 1e-3 {
        left = left.round();
        top = top.round();
    }
    let texel = |x: i32, y: i32| {
        let s = ic.px[(y.clamp(0, ic.h - 1) * ic.w + x.clamp(0, ic.w - 1)) as usize];
        [(s >> 24 & 255) as f32, (s >> 16 & 255) as f32, (s >> 8 & 255) as f32, (s & 255) as f32]
    };
    for dy in top.floor() as i32..(top + h).ceil() as i32 {
        for dx in left.floor() as i32..(left + w).ceil() as i32 {
            if dx < 0 || dy < 0 || dx >= stride || dy >= rows {
                continue;
            }
            let (sx, sy) = ((dx as f32 + 0.5 - left) / sc - 0.5, (dy as f32 + 0.5 - top) / sc - 0.5);
            if sx < -0.5 || sy < -0.5 || sx > ic.w as f32 - 0.5 || sy > ic.h as f32 - 0.5 {
                continue;
            }
            let (x0, y0) = (sx.floor() as i32, sy.floor() as i32);
            let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
            let (a, b, c, d) = (texel(x0, y0), texel(x0 + 1, y0), texel(x0, y0 + 1), texel(x0 + 1, y0 + 1));
            let m = |i: usize| (((a[i] * (1.0 - fx) + b[i] * fx) * (1.0 - fy) + (c[i] * (1.0 - fx) + d[i] * fx) * fy) * dim).round() as u32;
            blend(&mut buf[(dy * stride + dx) as usize], m(0) << 24 | m(1) << 16 | m(2) << 8 | m(3));
        }
    }
}

fn icon_sources(i: &Item, pins: &[Pin]) -> Vec<String> {
    let mut v = Vec::new();
    if let Some(p) = i.pin.as_ref().and_then(|id| pins.iter().find(|p| &p.id == id)) {
        if !p.open.starts_with("shell:") {
            v.push(p.open.clone());
        }
    }
    if let Some(a) = &i.aumid {
        v.push(format!(r"shell:AppsFolder\{a}"));
    }
    if !i.exe.is_empty() {
        v.push(i.exe.clone());
    }
    v
}

const ATTN: (u8, u8, u8) = (255, 176, 32);

// Texto centrado en una celda: GDI dibuja blanco sobre negro y la luminancia pasa a cobertura (como el tooltip).
unsafe fn draw_label(buf: &mut [u32], stride: i32, rows: i32, cell: R, text: &str, theme: &Theme, scale: f32, fg: (u8, u8, u8)) {
    let (w, h) = (cell.w, cell.h);
    let wide: Vec<u16> = text.encode_utf16().collect();
    let family: Vec<u16> = theme.font_family.encode_utf16().chain(std::iter::once(0)).collect();
    let screen = GetDC(None);
    let mem = CreateCompatibleDC(screen);
    let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: 0, ..Default::default() }, ..Default::default() };
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    if let Ok(dib) = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
        let old = SelectObject(mem, dib);
        let font = CreateFontW(-px(theme.font_size.max(11), scale), 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, ANTIALIASED_QUALITY.0 as u32, 0, PCWSTR(family.as_ptr()));
        let old_font = SelectObject(mem, font);
        let src = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
        src.fill(0);
        SetBkMode(mem, TRANSPARENT);
        SetTextColor(mem, COLORREF(0x00FF_FFFF));
        let mut rc = RECT { left: 0, top: 0, right: w, bottom: h };
        let mut tmp = wide.clone();
        DrawTextW(mem, &mut tmp, &mut rc, DT_CENTER | DT_VCENTER | DT_NOPREFIX | DT_SINGLELINE);
        for y in 0..h {
            for x in 0..w {
                let p = src[(y * w + x) as usize];
                let c = ((p >> 16) & 255).max((p >> 8) & 255).max(p & 255);
                let (dx, dy) = (cell.x + x, cell.y + y);
                if c > 0 && dx >= 0 && dy >= 0 && dx < stride && dy < rows {
                    blend(&mut buf[(dy * stride + dx) as usize], premul(fg, c as f32 / 255.0));
                }
            }
        }
        SelectObject(mem, old_font);
        let _ = DeleteObject(font);
        SelectObject(mem, old);
        let _ = DeleteObject(dib);
    }
    let _ = DeleteDC(mem);
    ReleaseDC(None, screen);
}

fn render() {
    let flashing = with(|d| d.flashing.clone());
    let (hwnd, theme, items, layout_, hover, scale, hidden, motion, pressed) = with(|d| {
        d.layout = layout(d.items.len(), d.scale, d.mon, d.theme.position != "bottom", style(&d.theme).gap);
        (d.hwnd, d.theme.clone(), d.items.clone(), Layout { bar: d.layout.bar, cells: d.layout.cells.clone(), overflow: d.layout.overflow }, d.hover, d.scale, d.hidden, d.motion, d.pressed)
    });
    let st = style(&theme);
    let size = px(st.icon, scale);
    let to_ask: Vec<Ask> = with(|d| {
        let missing: Vec<&Item> = items.iter().filter(|i| !d.icons.contains_key(&(i.key.clone(), size))).collect();
        missing.into_iter().filter(|i| d.asked.insert((i.key.clone(), size))).map(|i| (i.key.clone(), size, icon_sources(i, &d.pins))).collect()
    });
    if let Some(tx) = ASKS.lock().unwrap().as_ref() {
        for a in to_ask {
            let _ = tx.send(a);
        }
    }
    let sig = with(|d| signature(d));
    if with(|d| d.sig == sig) {
        return;
    }
    with(|d| d.sig = sig);
    let want_vis = if hidden || items.is_empty() { 0.0 } else { 1.0 };
    let vis = with(|d| {
        if !d.motion {
            d.vis = want_vis;
        }
        d.vis
    });
    unsafe {
        if vis != want_vis {
            start_anim(hwnd);
        }
        if want_vis == 0.0 {
            let _ = ShowWindow(with(|d| d.tip), SW_HIDE);
        }
        if vis == 0.0 && want_vis == 0.0 {
            let _ = ShowWindow(hwnd, SW_HIDE);
            return;
        }
        let (w, h) = (layout_.bar.w, layout_.bar.h);
        let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: 0, ..Default::default() }, ..Default::default() };
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(screen);
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let Ok(dib) = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else {
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
            return;
        };
        let old = SelectObject(mem, dib);
        let buf = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
        buf.fill(0);
        let (bg, fg, accent) = (rgb(&theme.background), rgb(&theme.foreground), rgb(&theme.accent));
        fill_round_a(buf, w, h, R { x: 0, y: 0, w, h }, px(st.radius, scale), bg, st.opacity);
        DOCK.with(|d| {
            let d = d.borrow();
            let icons = &d.as_ref().expect("dock").icons;
            for (idx, cell) in layout_.cells.iter().enumerate() {
                if layout_.overflow > 0 && idx == layout_.cells.len() - 1 {
                    fill_round_a(buf, w, h, *cell, px(10, scale), fg, 0.12);
                    draw_label(buf, w, h, *cell, &format!("+{}", layout_.overflow), &theme, scale, fg);
                    continue;
                }
                let Some(item) = items.get(idx) else { continue };
                let (hv, entry) = if motion { (ease(d.as_ref().expect("dock").hov.get(&item.key).copied().unwrap_or(0.0)), ease(d.as_ref().expect("dock").born.get(&item.key).copied().unwrap_or(1.0))) } else { ((hover == Some(idx)) as i32 as f32, 1.0) };
                let attn = !item.active && item.wins.iter().any(|w| flashing.contains(w));
                if item.active {
                    fill_round_a(buf, w, h, *cell, px(10, scale), accent, 0.28);
                } else if attn {
                    fill_round_a(buf, w, h, *cell, px(10, scale), ATTN, 0.38);
                } else if hv > 0.0 {
                    fill_round_a(buf, w, h, *cell, px(10, scale), fg, 0.10 * hv);
                }
                let dim = if item.other_ws { 0.5 } else { 1.0 } * entry;
                let lift = px(2, scale) as f32;
                let sc = if motion { (1.0 + 0.14 * hv) * (0.6 + 0.4 * entry) * if pressed == Some(idx) { 0.92 } else { 1.0 } } else { 1.0 };
                let (ccx, ccy) = (cell.x as f32 + cell.w as f32 / 2.0, cell.y as f32 + cell.h as f32 / 2.0 - lift - if motion { px(2, scale) as f32 * hv } else { 0.0 });
                match icons.get(&(item.key.clone(), size)) {
                    Some(ic) => draw_icon(buf, w, h, ic, ccx, ccy, sc, dim),
                    None => fill_round_a(buf, w, h, R { x: (ccx - size as f32 / 2.0) as i32, y: (ccy - size as f32 / 2.0) as i32, w: size, h: size }, px(6, scale), fg, 0.25),
                }
                if item.wins.is_empty() {
                    continue;
                }
                let dot = px(DOT_DIP, scale);
                let dw = if item.active { dot * 3 } else { dot };
                fill_round_a(buf, w, h, R { x: cell.x + (cell.w - dw) / 2, y: cell.y + cell.h - dot - px(3, scale), w: dw, h: dot }, dot / 2, if item.active { accent } else if attn { ATTN } else { fg }, if item.active || attn { 1.0 } else { 0.6 });
            }
        });
        let pos = POINT { x: layout_.bar.x, y: layout_.bar.y };
        let sz = SIZE { cx: w, cy: h };
        let src = POINT::default();
        let bf = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: (vis * 255.0).round() as u8, AlphaFormat: 1 };
        let _ = UpdateLayeredWindow(hwnd, None, Some(&pos), Some(&sz), mem, Some(&src), COLORREF(0), Some(&bf), ULW_ALPHA);
        SelectObject(mem, old);
        let _ = DeleteObject(dib);
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
}

fn drain_ready() {
    let ready: Vec<_> = std::mem::take(&mut *READY.lock().unwrap());
    if ready.is_empty() {
        return;
    }
    with(|d| {
        for (k, size, ic) in ready {
            d.icons.insert((k, size), ic);
        }
        if d.icons.len() > MAX_ICONS {
            let live: HashSet<String> = d.items.iter().map(|i| i.key.clone()).collect();
            d.icons.retain(|(k, _), _| live.contains(k));
            let kept: HashSet<(String, i32)> = d.icons.keys().cloned().collect();
            d.asked.retain(|a| kept.contains(a));
        }
        d.sig = 0;
    });
    render();
}

fn spawn_icon_worker(target: isize) {
    let (tx, rx) = std::sync::mpsc::channel::<Ask>();
    *ASKS.lock().unwrap() = Some(tx);
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        while let Ok((key, size, sources)) = rx.recv() {
            // shortcut: reintenta 3 veces con 400 ms en el mismo hilo (bloquea la cola un máximo de ~0,8 s); un ícono que sigue fallando queda en placeholder hasta recargar el dock.
            let found = (0..3).find_map(|n| {
                if n > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(400));
                }
                sources.iter().find_map(|s| fetch_icon(s, size))
            });
            if let Some(ic) = found {
                READY.lock().unwrap().push((key, size, ic));
                let _ = PostMessageW(HWND(target as *mut _), WM_ICON, WPARAM(0), LPARAM(0));
            }
        }
        CoUninitialize();
    });
}

unsafe fn fetch_icon(source: &str, size: i32) -> Option<Icon> {
    let wide: Vec<u16> = source.encode_utf16().chain(std::iter::once(0)).collect();
    let item: IShellItemImageFactory = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None).ok()?;
    let bmp = item.GetImage(SIZE { cx: size, cy: size }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK).ok()?;
    let mut bm = BITMAP::default();
    if GetObjectW(bmp, std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _)) == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 {
        let _ = DeleteObject(bmp);
        return None;
    }
    let (w, h) = (bm.bmWidth, bm.bmHeight);
    let mut buf = vec![0u32; (w * h) as usize];
    let mut bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: 0, ..Default::default() }, ..Default::default() };
    let dc = GetDC(None);
    let got = GetDIBits(dc, bmp, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut bmi, DIB_RGB_COLORS);
    ReleaseDC(None, dc);
    let _ = DeleteObject(bmp);
    if got == 0 {
        return None;
    }
    premultiply(&mut buf);
    Some(Icon { w, h, px: buf })
}

// GetImage entrega alfa recto; el dock compone con alfa premultiplicado, y sin esto los bordes semitransparentes salen blancos.
// Un ícono sin canal alfa (todo 0) se toma opaco.
fn premultiply(px: &mut [u32]) {
    if px.iter().all(|p| p >> 24 == 0) {
        px.iter_mut().for_each(|p| *p |= 0xFF00_0000);
        return;
    }
    for p in px.iter_mut() {
        let a = *p >> 24;
        let m = |sh: u32| (((*p >> sh) & 255) * a + 127) / 255;
        *p = a << 24 | m(16) << 16 | m(8) << 8 | m(0);
    }
}

unsafe fn dock_foreground(mon: MonRect) -> Option<Foreground> {
    let fg = GetForegroundWindow();
    if fg.0.is_null() {
        return None;
    }
    let mut rc = RECT::default();
    let _ = GetWindowRect(fg, &mut rc);
    let style = GetWindowLongW(fg, GWL_STYLE) as u32;
    let mut cls = [0u16; 64];
    let n = GetClassNameW(fg, &mut cls).max(0) as usize;
    let class = String::from_utf16_lossy(&cls[..n]);
    let m = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    let _ = GetMonitorInfoW(m, &mut mi);
    Some(Foreground {
        rect: (rc.left, rc.top, rc.right, rc.bottom),
        caption: style & WS_CAPTION.0 == WS_CAPTION.0,
        thickframe: style & WS_THICKFRAME.0 != 0,
        zoomed: IsZoomed(fg).as_bool(),
        shell_like: fg == GetShellWindow() || matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd"),
        on_dock_monitor: (mi.rcMonitor.left, mi.rcMonitor.top) == (mon.0, mon.1),
    })
}

// La taskbar nativa en auto-hide, al aparecer, queda sobre el dock: mientras se vea (o su overflow) no se reafirma el orden.
unsafe fn native_taskbar_visible(mon: MonRect) -> bool {
    if let Ok(t) = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) {
        let mut rc = RECT::default();
        if GetWindowRect(t, &mut rc).is_ok() && IsWindowVisible(t).as_bool() && rc.top < mon.3 - 4 && rc.bottom > mon.3 - 4 {
            return true;
        }
    }
    FindWindowW(w!("TopLevelWindowForOverflowXamlIsland"), PCWSTR::null()).is_ok_and(|o| IsWindowVisible(o).as_bool())
}

unsafe fn tick() {
    let (hwnd, mon, hidden) = with(|d| (d.hwnd, d.mon, d.hidden));
    let busy = matches!(SHQueryUserNotificationState(), Ok(s) if s == QUNS_BUSY || s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_PRESENTATION_MODE);
    let hide = apps::should_hide(dock_foreground(mon).as_ref(), mon, busy);
    let r0 = crate::monitor_rects()[0];
    let m0 = (r0.left, r0.top, r0.right, r0.bottom);
    let scale = crate::mon_scale(&r0);
    let changed = with(|d| {
        let ch = d.mon != m0 || (d.scale - scale).abs() > 0.01;
        if ch {
            d.mon = m0;
            d.scale = scale;
            d.icons.clear();
            d.asked.clear();
        }
        d.hidden = hide;
        d.motion = motion_enabled(&d.theme);
        ch
    });
    let (want_hide, tray_open) = with(|d| (d.theme.hide_native_taskbar, d.tray_until.is_some_and(|t| t > std::time::Instant::now())));
    if want_hide && !tray_open {
        set_native_taskbar(true);
    } else if !want_hide {
        restore_native_taskbar();
    }
    if hide && !hidden {
        crate::dock_menu::dismiss();
    }
    let pins_path = with(|d| d.pins_path.clone());
    let stamp = pins_stamp(&pins_path);
    let pins_changed = with(|d| d.pins_stamp != stamp);
    if pins_changed {
        let pins = load_pins(&pins_path);
        with(|d| {
            d.pins = pins;
            d.pins_stamp = stamp;
        });
        refresh();
    }
    if hide != hidden || changed {
        render();
    }
    if !hide && !native_taskbar_visible(m0) {
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

// Windows niega SetForegroundWindow a quien no es el foreground: se comparte la cola de entrada con él un instante.
unsafe fn raise(hwnd: HWND) {
    if SetForegroundWindow(hwnd).as_bool() {
        return;
    }
    let fg_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
    let me = GetCurrentThreadId();
    let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, true).as_bool();
    let _ = BringWindowToTop(hwnd);
    let _ = SetForegroundWindow(hwnd);
    if attached {
        let _ = AttachThreadInput(me, fg_thread, false);
    }
}

pub(crate) unsafe fn focus_window(hwnd: HWND) {
    let dock = with(|d| d.hwnd);
    if !IsWindow(hwnd).as_bool() {
        queue_refresh(dock);
        return;
    }
    let id = hwnd.0 as isize;
    if IsIconic(hwnd).as_bool() {
        let _ = ShowWindow(hwnd, SW_RESTORE);
    }
    match komorebi::window_slot(id) {
        Some((m, ws)) if is_cloaked(hwnd) => {
            let _ = std::process::Command::new("komorebic.exe").args(focus_args(m, ws)).creation_flags(0x0800_0000).spawn();
            with(|d| d.pending_focus = Some(id));
            SetTimer(dock, T_FOCUS, FOCUS_DELAY_MS, None);
        }
        _ => raise(hwnd),
    }
}

unsafe fn click(idx: usize) {
    let target = with(|d| d.items.get(idx).map(|i| (i.wins.clone(), d.last_active.get(&i.key).copied(), launch_of(&d.pins, i))));
    let Some((wins, last, launch)) = target else { return };
    if wins.is_empty() {
        launch.iter().for_each(|(o, a)| crate::dock_menu::launch(o, a));
        return;
    }
    let fg = GetForegroundWindow().0 as isize;
    if wins.len() > 1 {
        if let Some(ctx) = menu_ctx(idx, true) {
            hide_tip();
            crate::dock_menu::show(ctx);
        }
        return;
    }
    // Como la taskbar de Windows: clic en la app que ya está al frente la minimiza.
    let h = HWND(wins[0] as *mut _);
    if wins[0] == fg && !IsIconic(h).as_bool() {
        let _ = ShowWindowAsync(h, SW_MINIMIZE);
    } else if let Some(t) = pick_target(&wins, fg, last) {
        focus_window(HWND(t as *mut _));
    }
}

// Un selector con la primera ventana de cada grupo desde `from`; los anclados sin ventana no tienen a quién enfocar.
fn overflow_wins(items: &[Item], from: usize) -> Vec<isize> {
    items.iter().skip(from).filter_map(|i| i.wins.first().copied()).collect()
}

// "+N": un selector con la primera ventana de cada grupo que no entró en el dock.
unsafe fn overflow_ctx(first_hidden: usize) -> Option<crate::dock_menu::Ctx> {
    let mut ctx = with(|d| {
        let cell = *d.layout.cells.last()?;
        let wins = overflow_wins(&d.items, first_hidden);
        Some(crate::dock_menu::Ctx {
            picker: true,
            titles: Vec::new(),
            active: None,
            theme: d.theme.clone(),
            scale: d.scale,
            cx: d.layout.bar.x + cell.x + cell.w / 2,
            bar: d.layout.bar,
            bottom: d.theme.position != "bottom",
            mon: d.mon,
            wins,
            last: None,
            key: String::new(),
            pinned: false,
            pinnable: false,
            launch: None,
            closable: false,
            tray: false,
        })
    })?;
    ctx.active = Some(GetForegroundWindow().0 as isize);
    ctx.titles = window_titles(&ctx.wins);
    Some(ctx)
}

unsafe fn window_titles(wins: &[isize]) -> Vec<String> {
    wins.iter()
        .map(|w| {
            let mut buf = [0u16; 256];
            let n = GetWindowTextW(HWND(*w as *mut _), &mut buf).max(0) as usize;
            clip_title(&String::from_utf16_lossy(&buf[..n]), 48)
        })
        .collect()
}

// Datos del menú de una celda. Los títulos se leen fuera del préstamo del estado.
unsafe fn menu_ctx(idx: usize, picker: bool) -> Option<crate::dock_menu::Ctx> {
    let mut ctx = with(|d| {
        let it = d.items.get(idx)?;
        let cell = *d.layout.cells.get(idx)?;
        Some(crate::dock_menu::Ctx {
            picker,
            titles: Vec::new(),
            active: None,
            theme: d.theme.clone(),
            scale: d.scale,
            cx: d.layout.bar.x + cell.x + cell.w / 2,
            bar: d.layout.bar,
            bottom: d.theme.position != "bottom",
            mon: d.mon,
            wins: it.wins.clone(),
            last: d.last_active.get(&it.key).copied(),
            key: it.key.clone(),
            pinned: it.pin.is_some(),
            pinnable: pin_for(it, |p| p.exists()).is_some(),
            launch: launch_of(&d.pins, it),
            closable: true,
            tray: d.theme.hide_native_taskbar,
        })
    })?;
    ctx.closable = ctx.wins.iter().any(|w| !is_elevated(HWND(*w as *mut _)));
    ctx.active = Some(GetForegroundWindow().0 as isize);
    ctx.titles = window_titles(&ctx.wins);
    Some(ctx)
}

fn xy(lp: LPARAM) -> (i32, i32) {
    ((lp.0 & 0xffff) as i16 as i32, ((lp.0 >> 16) & 0xffff) as i16 as i32)
}

unsafe extern "system" fn tip_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if m == WM_MOUSEACTIVATE {
        return LRESULT(MA_NOACTIVATE as isize);
    }
    DefWindowProcW(h, m, wp, lp)
}

pub fn clip_title(t: &str, max: usize) -> String {
    if t.chars().count() <= max {
        return t.to_string();
    }
    let mut out: String = t.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

// Texto del tooltip: el título de la ventana a enfocar, o el nombre del anclado si no hay ventanas.
unsafe fn tip_text(idx: usize) -> Option<String> {
    if let Some(n) = with(|d| (d.layout.overflow > 0 && idx + 1 == d.layout.cells.len()).then_some(d.layout.overflow)) {
        return Some(format!("+{n}"));
    }
    let (wins, last, pin) = with(|d| d.items.get(idx).map(|i| (i.wins.clone(), d.last_active.get(&i.key).copied(), launch_of(&d.pins, i).map(|(o, _)| o))))?;
    let target = pick_target(&wins, 0, last);
    if let Some(t) = target {
        let h = HWND(t as *mut _);
        let mut buf = [0u16; 256];
        let n = GetWindowTextW(h, &mut buf).max(0) as usize;
        let title = String::from_utf16_lossy(&buf[..n]);
        if !title.is_empty() {
            return Some(clip_title(&title, 48));
        }
    }
    pin.and_then(|p| std::path::Path::new(&p).file_stem().map(|s| s.to_string_lossy().into_owned()))
}

unsafe fn hide_tip() {
    let tip = with(|d| d.tip);
    let _ = ShowWindow(tip, SW_HIDE);
}

// Tooltip propio (ventana layered chica): el texto se dibuja con GDI sobre negro y su luminancia pasa a alfa.
unsafe fn show_tip(idx: usize) {
    if crate::dock_menu::is_open() {
        return;
    }
    let Some(text) = tip_text(idx) else { return hide_tip() };
    let (tip, theme, scale, bar, cell, bottom) = with(|d| (d.tip, d.theme.clone(), d.scale, d.layout.bar, d.layout.cells.get(idx).copied(), d.theme.position != "bottom"));
    let Some(cell) = cell else { return hide_tip() };
    let wide: Vec<u16> = text.encode_utf16().collect();
    let family: Vec<u16> = theme.font_family.encode_utf16().chain(std::iter::once(0)).collect();
    let screen = GetDC(None);
    let mem = CreateCompatibleDC(screen);
    let font = CreateFontW(-px((theme.font_size - 1).max(10), scale), 0, 0, 0, 500, 0, 0, 0, 1, 0, 0, ANTIALIASED_QUALITY.0 as u32, 0, PCWSTR(family.as_ptr()));
    let old_font = SelectObject(mem, font);
    let mut ext = SIZE::default();
    let _ = GetTextExtentPoint32W(mem, &wide, &mut ext);
    let pad = px(8, scale);
    let (w, h) = (ext.cx + 2 * pad, ext.cy + pad);
    let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: 0, ..Default::default() }, ..Default::default() };
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let Ok(dib) = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else {
        SelectObject(mem, old_font);
        let _ = DeleteObject(font);
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        return;
    };
    let old = SelectObject(mem, dib);
    let buf = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
    buf.fill(0);
    SetBkMode(mem, TRANSPARENT);
    SetTextColor(mem, COLORREF(0x00FF_FFFF));
    let mut rc = RECT { left: pad, top: pad / 2, right: w - pad, bottom: h };
    let mut tmp = wide.clone();
    DrawTextW(mem, &mut tmp, &mut rc, DT_LEFT | DT_NOPREFIX | DT_SINGLELINE);
    let (bg, fg) = (rgb(&theme.background), rgb(&theme.foreground));
    let coverage: Vec<u32> = buf.iter().map(|p| ((*p >> 16) & 255).max((*p >> 8) & 255).max(*p & 255)).collect();
    buf.fill(0);
    fill_round_a(buf, w, h, R { x: 0, y: 0, w, h }, px(8, scale), bg, 0.96);
    for (i, c) in coverage.iter().enumerate() {
        if *c > 0 {
            blend(&mut buf[i], premul(fg, *c as f32 / 255.0));
        }
    }
    let mon = with(|d| d.mon);
    let (x, y) = crate::dock_menu::popup_pos(bar.x + cell.x + cell.w / 2, bar, w, h, bottom, px(8, scale), mon);
    let (pos, sz, src) = (POINT { x, y }, SIZE { cx: w, cy: h }, POINT::default());
    let bf = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: 1 };
    let _ = UpdateLayeredWindow(tip, None, Some(&pos), Some(&sz), mem, Some(&src), COLORREF(0), Some(&bf), ULW_ALPHA);
    SelectObject(mem, old);
    SelectObject(mem, old_font);
    let _ = DeleteObject(dib);
    let _ = DeleteObject(font);
    let _ = DeleteDC(mem);
    ReleaseDC(None, screen);
    let _ = ShowWindow(tip, SW_SHOWNOACTIVATE);
}

unsafe extern "system" fn dock_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let (shell_msg, created_msg) = DOCK.with(|d| d.try_borrow().ok().and_then(|d| d.as_ref().map(|d| (d.shell_msg, d.created_msg))).unwrap_or((0, 0)));
    if m != 0 && m == shell_msg {
        // HSHELL_FLASH: la ventana pide atención; se limpia al pasar al frente o al cerrarse.
        if wp.0 as u32 == 0x8006 {
            with(|d| d.flashing.insert(lp.0));
        }
        queue_refresh(h);
        return LRESULT(0);
    }
    if m != 0 && m == created_msg {
        register(h);
        queue_refresh(h);
        return LRESULT(0);
    }
    match m {
        WM_TIMER if wp.0 == T_REFRESH => {
            let _ = KillTimer(h, T_REFRESH);
            refresh();
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_ANIM => {
            anim_tick(h);
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_TICK => {
            tick();
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_FOCUS => {
            let _ = KillTimer(h, T_FOCUS);
            if let Some(id) = with(|d| d.pending_focus.take()) {
                let t = HWND(id as *mut _);
                if IsWindow(t).as_bool() {
                    raise(t);
                }
            }
            LRESULT(0)
        }
        WM_ICON => {
            drain_ready();
            LRESULT(0)
        }
        x if x == komorebi::WM_WINDOW_MAP => {
            queue_refresh(h);
            LRESULT(0)
        }
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            with(|d| d.sig = 0);
            tick();
            LRESULT(0)
        }
        // WM_WTSSESSION_CHANGE o suspensión: se re-siembra y se reafirma el orden.
        0x02B1 | WM_POWERBROADCAST => {
            with(|d| d.sig = 0);
            queue_refresh(h);
            let _ = SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            LRESULT(1)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_MOUSEMOVE => {
            let (x, y) = xy(lp);
            let dragged = with(|d| {
                let hit = match hit_test(&d.layout, x + d.layout.bar.x, y + d.layout.bar.y) {
                    Some(Hit::Item(i)) => d.items.get(i).map(|it| it.key.clone()),
                    _ => None,
                };
                let thresh = px(6, d.scale);
                let Some(dr) = d.drag.as_mut() else { return false };
                dr.moving |= (x - dr.x0).abs() > thresh;
                let from = dr.key.clone();
                match hit {
                    Some(to) if dr.moving && to != from => reorder(d, &from, &to),
                    _ => false,
                }
            });
            if dragged {
                refresh();
            }
            let (changed, need_track) = with(|d| {
                let idx = match hit_test(&d.layout, x + d.layout.bar.x, y + d.layout.bar.y) {
                    Some(Hit::Item(i)) => Some(i),
                    _ => None,
                };
                let changed = idx != d.hover;
                d.hover = idx;
                let need = !d.tracking;
                d.tracking = true;
                (changed, need)
            });
            if need_track {
                let _ = TrackMouseEvent(&mut TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: h, dwHoverTime: 0 });
            }
            if changed {
                render();
                if with(|d| d.motion) {
                    start_anim(h);
                }
                match with(|d| d.hover) {
                    Some(i) => show_tip(i),
                    None => hide_tip(),
                }
            }
            LRESULT(0)
        }
        0x02A3 => {
            with(|d| {
                d.hover = None;
                d.pressed = None;
                d.tracking = false;
            });
            render();
            if with(|d| d.motion) {
                start_anim(h);
            }
            hide_tip();
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let (x, y) = xy(lp);
            if let Some(Hit::Item(i)) = with(|d| hit_test(&d.layout, x + d.layout.bar.x, y + d.layout.bar.y)) {
                if let Some(ctx) = menu_ctx(i, false) {
                    hide_tip();
                    crate::dock_menu::show(ctx);
                }
            }
            LRESULT(0)
        }
        WM_MBUTTONUP => {
            let (x, y) = xy(lp);
            let target = with(|d| match hit_test(&d.layout, x + d.layout.bar.x, y + d.layout.bar.y) {
                Some(Hit::Item(i)) => d.items.get(i).and_then(|it| launch_of(&d.pins, it)),
                _ => None,
            });
            target.iter().for_each(|(o, a)| crate::dock_menu::launch(o, a));
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = xy(lp);
            let on_item = with(|d| match hit_test(&d.layout, x + d.layout.bar.x, y + d.layout.bar.y) {
                Some(Hit::Item(i)) => Some(i),
                _ => None,
            });
            if let Some(i) = on_item {
                with(|d| d.drag = d.items.get(i).map(|it| Drag { key: it.key.clone(), x0: x, moving: false }));
                SetCapture(h);
            }
            if on_item.is_some() && with(|d| d.motion) {
                with(|d| d.pressed = on_item);
                render();
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = xy(lp);
            let _ = ReleaseCapture();
            let moved = with(|d| d.drag.take()).is_some_and(|dr| dr.moving);
            if with(|d| d.pressed.take()).is_some() {
                with(|d| d.sig = 0);
                render();
            }
            if moved {
                return LRESULT(0);
            }
            match with(|d| hit_test(&d.layout, x + d.layout.bar.x, y + d.layout.bar.y).map(|h| (h, d.layout.cells.len()))) {
                Some((Hit::Item(i), _)) => click(i),
                Some((Hit::Overflow, cells)) => {
                    if let Some(ctx) = overflow_ctx(cells - 1) {
                        hide_tip();
                        crate::dock_menu::show(ctx);
                    }
                }
                None => {}
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MON: MonRect = (0, 0, 1920, 1080);

    #[test]
    fn dock_is_centered_on_the_requested_edge() {
        let b = layout(3, 1.0, MON, true, 4);
        assert_eq!(b.bar.y + b.bar.h, 1080 - MARGIN_DIP);
        assert_eq!(b.bar.x * 2 + b.bar.w, 1920);
        assert_eq!(layout(3, 1.0, MON, false, 4).bar.y, MARGIN_DIP);
        assert_eq!(b.cells.len(), 3);
    }

    #[test]
    fn style_is_clamped_and_motion_eases_to_its_target() {
        let t = Theme { dock_icon: 99, dock_radius: -3, dock_gap: 50, dock_opacity: Some(0.1), ..Theme::default() };
        let s = style(&t);
        assert_eq!((s.icon, s.radius, s.gap, s.opacity), (32, 0, 12, 0.4));
        assert_eq!(approach(0.0, 1.0, 60.0, 120.0), 0.5);
        assert_eq!(approach(0.9, 1.0, 60.0, 120.0), 1.0);
        assert_eq!(approach(0.3, 0.0, 120.0, 120.0), 0.0);
        assert!(ease(0.5) > 0.5 && ease(1.0) == 1.0 && ease(0.0) == 0.0);
    }

    #[test]
    fn a_scaled_icon_stays_centred_and_one_to_one_is_exact() {
        let ic = Icon { w: 2, h: 2, px: vec![0xFF_FF_FF_FF; 4] };
        let mut one = vec![0u32; 16];
        draw_icon(&mut one, 4, 4, &ic, 2.0, 2.0, 1.0, 1.0);
        assert_eq!(one.iter().filter(|p| **p == 0xFF_FF_FF_FF).count(), 4);
        let mut big = vec![0u32; 16];
        draw_icon(&mut big, 4, 4, &ic, 2.0, 2.0, 2.0, 1.0);
        assert_eq!(big.iter().filter(|p| **p == 0xFF_FF_FF_FF).count(), 16);
        let mut half = vec![0u32; 16];
        draw_icon(&mut half, 4, 4, &ic, 2.0, 2.0, 1.0, 0.5);
        assert!(half.iter().all(|p| *p == 0 || (*p >> 24) == 0x80));
    }

    #[test]
    fn moving_an_item_shifts_the_ones_in_between() {
        let mut v = vec![1, 2, 3, 4];
        move_item(&mut v, 0, 2);
        assert_eq!(v, [2, 3, 1, 4]);
        move_item(&mut v, 3, 0);
        assert_eq!(v, [4, 2, 3, 1]);
    }

    #[test]
    fn the_overflow_picker_lists_one_window_per_hidden_group_and_skips_launchers() {
        let it = |wins: Vec<isize>| Item { key: String::new(), exe: String::new(), aumid: None, wins, active: false, other_ws: false, pin: None };
        let items = vec![it(vec![1]), it(vec![2, 3]), it(vec![]), it(vec![4])];
        assert_eq!(overflow_wins(&items, 1), [2, 4]);
        assert!(overflow_wins(&items, 4).is_empty());
    }

    #[test]
    fn many_windows_collapse_into_a_plus_n_cell() {
        let l = layout(200, 1.0, MON, true, 4);
        assert!(l.overflow > 1 && l.bar.w <= 1920 * 4 / 5);
        assert_eq!(l.cells.len() + l.overflow - 1, 200);
        assert_eq!(hit_test(&l, l.bar.x + l.cells.last().unwrap().x + 1, l.bar.y + l.cells[0].y + 1), Some(Hit::Overflow));
    }

    #[test]
    fn hit_test_finds_the_cell_and_ignores_the_gaps() {
        let l = layout(3, 1.0, MON, true, 4);
        let c = l.cells[1];
        assert_eq!(hit_test(&l, l.bar.x + c.x + 5, l.bar.y + c.y + 5), Some(Hit::Item(1)));
        assert_eq!(hit_test(&l, l.bar.x + c.x - 1, l.bar.y + c.y + 5), None);
        assert_eq!(hit_test(&l, 0, 0), None);
    }

    #[test]
    fn zero_items_do_not_panic() {
        assert!(layout(0, 1.5, MON, true, 4).cells.is_empty());
    }

    #[test]
    fn click_cycles_a_group_that_is_already_active() {
        assert_eq!(pick_target(&[1, 2, 3], 2, None), Some(3));
        assert_eq!(pick_target(&[1, 2, 3], 3, None), Some(1));
        assert_eq!(pick_target(&[7], 7, None), Some(7));
    }

    #[test]
    fn click_prefers_the_last_active_window_and_ignores_stale_ones() {
        assert_eq!(pick_target(&[1, 2], 99, Some(2)), Some(2));
        assert_eq!(pick_target(&[1, 2], 99, Some(5)), Some(1));
        assert_eq!(pick_target(&[], 99, None), None);
    }

    fn item(key: &str, exe: &str) -> Item {
        Item { key: key.into(), exe: exe.into(), aumid: None, wins: vec![1], active: false, other_ws: false, pin: None }
    }

    #[test]
    fn a_running_pin_merges_with_its_group_and_leads_the_list() {
        let pins = vec![Pin { id: "a.lnk".into(), open: "a.lnk".into(), args: String::new(), exe: r"C:\X\Tool.exe".into(), aumid: None, launched: None }];
        let out = arrange(&pins, vec![item("exe:other", r"C:\o.exe"), item("exe:tool", r"c:\x\tool.EXE")]);
        assert_eq!((out[0].key.as_str(), out[0].pin.as_deref(), out.len()), ("exe:tool", Some("a.lnk"), 2));
    }

    #[test]
    fn a_pin_without_exe_merges_by_aumid() {
        let pins = vec![Pin { id: "e.lnk".into(), open: "e.lnk".into(), args: String::new(), exe: String::new(), aumid: Some("Microsoft.Windows.Explorer".into()), launched: None }];
        let mut g = item("aumid:microsoft.windows.explorer", r"C:\Windows\explorer.exe");
        g.aumid = Some("microsoft.windows.explorer".into());
        let out = arrange(&pins, vec![item("exe:o", "o.exe"), g]);
        assert_eq!((out.len(), out[0].pin.as_deref(), out[0].wins.len()), (2, Some("e.lnk"), 1));
    }

    #[test]
    fn a_squirrel_pin_merges_with_the_real_exe() {
        assert_eq!(squirrel_target(r"C:\U\Discord\Update.exe", "--processStart Discord.exe").as_deref(), Some("discord.exe"));
        assert_eq!(squirrel_target(r"C:\U\Tool.exe", "--processStart Discord.exe"), None);
        let pins = vec![Pin { id: "d.lnk".into(), open: "d.lnk".into(), args: String::new(), exe: r"C:\U\Discord\Update.exe".into(), aumid: None, launched: Some("discord.exe".into()) }];
        let out = arrange(&pins, vec![item("exe:d", r"C:\U\Discord\app-1\Discord.exe")]);
        assert_eq!((out.len(), out[0].wins.len()), (1, 1));
    }

    #[test]
    fn pins_launch_uwp_by_aumid_and_squirrel_through_update_exe() {
        let mut uwp = item("aumid:calc", r"C:\Windows\System32\ApplicationFrameHost.exe");
        uwp.aumid = Some("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App".into());
        let p = pin_for(&uwp, |_| false).unwrap();
        assert_eq!((p.open.as_str(), p.exe.as_str()), (r"shell:AppsFolder\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App", ""));
        let sq = item("exe:discord", r"C:\U\Discord\app-1.0.9\Discord.exe");
        let p = pin_for(&sq, |u| u.ends_with("Update.exe")).unwrap();
        assert_eq!((p.args.as_str(), p.launched.as_deref()), ("--processStart Discord.exe", Some("discord.exe")));
        assert!(p.open.ends_with("Update.exe"));
        let plain = pin_for(&item("exe:t", r"C:\T\Tool.exe"), |_| false).unwrap();
        assert_eq!((plain.open.as_str(), plain.args.as_str()), (r"C:\T\Tool.exe", ""));
        assert!(pin_for(&item("exe:none", ""), |_| false).is_none());
    }

    #[test]
    fn a_pin_without_windows_stays_as_a_launcher() {
        let pins = vec![Pin { id: "b.lnk".into(), open: "b.lnk".into(), args: String::new(), exe: r"C:\gone.exe".into(), aumid: None, launched: None }];
        let out = arrange(&pins, vec![item("exe:o", "o.exe")]);
        assert!(out[0].wins.is_empty() && out[0].pin.as_deref() == Some("b.lnk") && out[1].key == "exe:o");
    }

    #[test]
    fn pins_follow_the_taskband_order_and_unknowns_go_last_alphabetically() {
        let utf16 = |t: &str| t.encode_utf16().flat_map(|u| u.to_le_bytes()).collect::<Vec<u8>>();
        let mut blob = vec![1u8, 0, 0xFF];
        for n in ["Helium.lnk", "File Explorer.lnk"] {
            blob.extend(utf16(n));
            blob.extend([0, 0, 7, 0]);
        }
        let order = lnk_order(&blob);
        assert_eq!(order.len(), 2);
        assert!(order[0].ends_with("helium.lnk") && order[1].ends_with("file explorer.lnk"));
        let mut files: Vec<std::path::PathBuf> = ["Zed.lnk", "File Explorer.lnk", "Abc.lnk", "Helium.lnk"].iter().map(Into::into).collect();
        sort_pins(&mut files, &order);
        let names: Vec<_> = files.iter().map(|f| f.to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["Helium.lnk", "File Explorer.lnk", "Abc.lnk", "Zed.lnk"]);
    }

    #[test]
    fn straight_alpha_icons_are_premultiplied_and_alphaless_ones_become_opaque() {
        let mut half = [0x80FF_8000u32];
        premultiply(&mut half);
        assert_eq!(half[0], 0x8080_4000);
        let mut flat = [0x0011_2233u32, 0];
        premultiply(&mut flat);
        assert_eq!(flat, [0xFF11_2233, 0xFF00_0000]);
    }

    #[test]
    fn long_titles_are_clipped_with_an_ellipsis() {
        assert_eq!(clip_title("abc", 5), "abc");
        assert_eq!(clip_title("abcdefgh", 5), "abcd…");
    }

    #[test]
    fn komorebic_arguments_are_numeric_only() {
        assert_eq!(focus_args(1, 3), ["focus-monitor-workspace", "1", "3"]);
    }
}
