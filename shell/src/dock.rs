// Dock de la shell: una ventana propia con las apps abiertas. La geometría es pura (`layout`) y lo demás es Win32.
use crate::apps::{self, Foreground, WinAttrs};
use crate::komorebi;
use crate::layout::{px, R};
use crate::theme::{rgb, Theme};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
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
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, GetCapture, ReleaseCapture, SendInput, SetCapture, TrackMouseEvent, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, TME_LEAVE, TRACKMOUSEEVENT, VK_APPS, VK_B, VK_CONTROL, VK_END, VK_ESCAPE, VK_F10, VK_HOME, VK_LEFT, VK_LWIN, VK_MENU, VK_RETURN, VK_RIGHT, VK_RWIN, VK_SHIFT, VK_SPACE, VK_TAB};
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const CELL_DIP: i32 = 40;
const PAD_DIP: i32 = 8;
pub const BAR_DIP: i32 = 48;
pub(crate) const MARGIN_DIP: i32 = 6;
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

pub(crate) const HOVER_MS: f32 = 120.0;
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
    pub brand: R,
    pub tray: R,
    pub cells: Vec<R>,
    pub overflow: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Brand,
    Tray,
    Item(usize),
    Overflow,
}

const HUB_INDEX: usize = usize::MAX;
const TRAY_INDEX: usize = usize::MAX - 1;

// El dock vive en el borde opuesto al de la isla: `bottom` es el borde del dock. `bar` va en coordenadas de pantalla, `cells` relativas a `bar`.
pub fn layout(n: usize, scale: f32, mon: MonRect, bottom: bool, gap_dip: i32) -> Layout {
    layout_for(n, scale, mon, bottom, gap_dip, false)
}

// `reserve` deja a cada lado el ancho del cluster; solo el dock que lo dibuja lo necesita.
pub fn layout_for(n: usize, scale: f32, mon: MonRect, bottom: bool, gap_dip: i32, reserve: bool) -> Layout {
    let (cell, gap, pad) = (px(CELL_DIP, scale), px(gap_dip, scale), px(PAD_DIP, scale));
    // El dock va centrado: deja a cada lado el ancho del cluster para que la píldora entre en cualquier monitor.
    let side = if reserve { px(crate::cluster::RESERVE_DIP, scale) } else { 0 };
    let max_w = ((mon.2 - mon.0) * 4 / 5).min(mon.2 - mon.0 - 2 * side);
    let cap = (((max_w - 2 * pad + gap) / (cell + gap)).max(1)) as usize;
    // El botón de bandeja vive en el cluster de la derecha cuando este dock lo dibuja.
    let tail = usize::from(!reserve);
    let app_cap = cap.saturating_sub(1 + tail);
    let (shown, overflow) = if n <= app_cap { (n, 0) } else { (app_cap.saturating_sub(1), n - app_cap.saturating_sub(1)) };
    let slots = shown + usize::from(overflow > 0) + 1 + tail;
    let w = 2 * pad + slots as i32 * cell + (slots as i32 - 1) * gap;
    let h = px(BAR_DIP, scale);
    let x = mon.0 + (mon.2 - mon.0 - w) / 2;
    let y = if bottom { mon.3 - px(MARGIN_DIP, scale) - h } else { mon.1 + px(MARGIN_DIP, scale) };
    let brand = R { x: pad, y: (h - cell) / 2, w: cell, h: cell };
    let cells = (0..slots - 1 - tail).map(|i| R { x: pad + (i as i32 + 1) * (cell + gap), y: (h - cell) / 2, w: cell, h: cell }).collect();
    let tray = if reserve { R::default() } else { R { x: pad + (slots as i32 - 1) * (cell + gap), y: (h - cell) / 2, w: cell, h: cell } };
    Layout { bar: R { x, y, w, h }, brand, tray, cells, overflow }
}

pub fn hit_test(l: &Layout, x: i32, y: i32) -> Option<Hit> {
    let (rx, ry) = (x - l.bar.x, y - l.bar.y);
    if rx >= l.brand.x && rx < l.brand.x + l.brand.w && ry >= l.brand.y && ry < l.brand.y + l.brand.h {
        return Some(Hit::Brand);
    }
    if let Some(i) = l.cells.iter().position(|c| rx >= c.x && rx < c.x + c.w && ry >= c.y && ry < c.y + c.h) {
        return Some(if l.overflow > 0 && i == l.cells.len() - 1 { Hit::Overflow } else { Hit::Item(i) });
    }
    (rx >= l.tray.x && rx < l.tray.x + l.tray.w && ry >= l.tray.y && ry < l.tray.y + l.tray.h).then_some(Hit::Tray)
}

fn display_layout(layout: &Layout, xs: &[i32], bar: R) -> Layout {
    let tray = R { x: bar.w - (layout.bar.w - layout.tray.x), ..layout.tray };
    Layout { bar, brand: layout.brand, tray, cells: layout.cells.iter().zip(xs).map(|(c, x)| R { x: *x, ..*c }).collect(), overflow: layout.overflow }
}

// Ventana a enfocar al hacer clic: con el grupo ya activo, la siguiente; si no, la última activa del grupo.
// La rueda sobre un grupo con varias ventanas recorre cuál se previsualiza: hacia abajo la siguiente, hacia arriba la anterior.
pub fn step_preview(wins: &[isize], current: Option<isize>, delta: i32) -> Option<isize> {
    if wins.len() < 2 || delta == 0 {
        return None;
    }
    let at = current.and_then(|c| wins.iter().position(|w| *w == c)).unwrap_or(0) as i32;
    let n = wins.len() as i32;
    Some(wins[(at + if delta < 0 { 1 } else { -1 }).rem_euclid(n) as usize])
}

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
const T_ANIM: usize = 4;
const T_EDGE: usize = 5;
const T_TIP: usize = 6;
const WM_TRAY_DISMISS: u32 = WM_APP + 22;
const WM_TRAY_FOREGROUND: u32 = WM_APP + 23;
const T_TRAY_FOCUS: usize = 7;
const T_TRAY_ACTIVATE: usize = 8;
const T_CLICK: usize = 9;
const FOCUS_DELAY_MS: u32 = 220;
const TIP_FADE_MS: u128 = 130;
const GLIDE_TAU_MS: f32 = 55.0;
const EDGE_MS: u32 = 60;
const LEAVE_MS: u128 = 450;
const ANIM_MS: u32 = 16;
const REFRESH_MS: u32 = 60;

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

#[derive(Clone)]
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
    preview_pick: Option<(String, isize)>,
    key_target: Option<KeyTarget>,
    key_witness: Option<(isize, u32)>,
    accessible_titles: HashMap<String, String>,
    tracking: bool,
    scale: f32,
    mon: MonRect,
    sig: u64,
    hidden: bool,
    pending_click: Option<(Replay, u8)>,
    pins: Vec<Pin>,
    ident: HashMap<isize, (String, Option<String>, std::time::Instant)>,
    // hwnd -> cuándo se abrió (FILETIME): celdas y ventanas de un grupo van por orden de apertura, no por z-order.
    opened: HashMap<isize, u64>,
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
    flashing: HashSet<isize>,
    drag: Option<Drag>,
    pointer_down: Option<KeyTarget>,
    peek: bool,
    start_visible: bool,
    leave: Option<std::time::Instant>,
    tip_born: Option<std::time::Instant>,
    glide: HashMap<String, f32>,
    bar_glide: Option<(f32, f32)>,
    anim_on: bool,
}

impl Dock {
    fn want_visible(&self) -> bool {
        self.start_visible || self.key_target.is_some() || !(self.hidden || (self.hides() && !self.peek))
    }

    fn hides(&self) -> bool {
        dock_hides(&self.theme, self.mon)
    }
}

// `auto` esconde el dock salvo en un workspace floating; sin datos de komorebi lo deja a la vista.
fn dock_hides(theme: &Theme, mon: MonRect) -> bool {
    if theme.dock_hide {
        return true;
    }
    theme.dock_auto
        && komorebi::CURRENT.lock().unwrap().as_ref().and_then(|w| w.monitor_modes.iter().find(|(at, _)| *at == (mon.0, mon.1)).map(|(_, m)| *m)).is_some_and(|m| m != komorebi::Mode::Floating)
}

struct Drag {
    key: String,
    x0: i32,
    moving: bool,
}


// Un dock por monitor (el i-ésimo de `monitor_rects`); `CUR` marca sobre cuál operan `with` y las funciones sin parámetro.
thread_local! {
    static DOCKS: RefCell<Vec<Dock>> = const { RefCell::new(Vec::new()) };
    static CUR: Cell<usize> = const { Cell::new(0) };
    static START_FOREGROUND: RefCell<(isize, u32, String)> = const { RefCell::new((0, 0, String::new())) };
    static TRAY_SESSION: Cell<Option<(u32, usize)>> = const { Cell::new(None) };
    static KEY_SESSION: Cell<Option<(isize, isize, u32)>> = const { Cell::new(None) };
    static TRAY_SEQUENCE: Cell<usize> = const { Cell::new(0) };
    static TRAY_FOCUS_SEEN: Cell<bool> = const { Cell::new(false) };
    static TRAY_FOCUS_STARTED: Cell<Option<std::time::Instant>> = const { Cell::new(None) };
    static TRAY_BASE_FOCUS: Cell<isize> = const { Cell::new(0) };
    static TRAY_PENDING: Cell<bool> = const { Cell::new(false) };
    static TRAY_FALLBACK: Cell<bool> = const { Cell::new(false) };
    static TRAY_OBSERVER: Cell<(isize, usize)> = const { Cell::new((0, 0)) };
}

fn with<R>(f: impl FnOnce(&mut Dock) -> R) -> R {
    DOCKS.with(|v| {
        let mut v = v.borrow_mut();
        let i = CUR.get().min(v.len().saturating_sub(1));
        f(v.get_mut(i).expect("dock"))
    })
}

fn dock_count() -> usize {
    DOCKS.with(|v| v.borrow().len())
}

fn for_each_dock(mut f: impl FnMut()) {
    let keep = CUR.get();
    for i in 0..dock_count() {
        CUR.set(i);
        f();
    }
    CUR.set(keep);
}

// Íconos listos desde el worker, con píxeles premultiplicados.
static READY: Mutex<Vec<(String, i32, Icon)>> = Mutex::new(Vec::new());
type Ask = (String, i32, Vec<String>);
static ASKS: Mutex<Option<std::sync::mpsc::Sender<Ask>>> = Mutex::new(None);

#[link(name = "wtsapi32")]
unsafe extern "system" {
    fn WTSUnRegisterSessionNotification(hwnd: HWND) -> BOOL;
}

pub fn init(theme: &Theme, state_dir: &std::path::Path) -> Option<HWND> {
    unsafe {
        let inst = GetModuleHandleW(PCWSTR::null()).ok()?;
        let class = WNDCLASSW { lpfnWndProc: Some(dock_proc), hInstance: inst.into(), hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(), lpszClassName: w!("WinarchyShellDock"), ..Default::default() };
        RegisterClassW(&class);
        let tip_class = WNDCLASSW { lpfnWndProc: Some(tip_proc), hInstance: inst.into(), lpszClassName: w!("WinarchyShellDockTip"), ..Default::default() };
        RegisterClassW(&tip_class);
        let pins_path = state_dir.join("dock-pins.json");
        let docks: Vec<Dock> = crate::monitor_rects().iter().filter_map(|r| create_dock(r, theme, &pins_path)).collect();
        let first = docks.first()?.hwnd;
        DOCKS.with(|v| *v.borrow_mut() = docks);
        spawn_icon_worker(first.0 as isize);
        komorebi::NOTIFY.store(first.0 as isize, std::sync::atomic::Ordering::Relaxed);
        crate::cluster::init(theme, state_dir);
        for_each_dock(|| {
            let h = with(|d| d.hwnd);
            register(h);
            refresh();
            SetTimer(h, T_TICK, 1000, None);
            sync_edge_timer(h);
        });
        Some(first)
    }
}

unsafe fn create_dock(r: &RECT, theme: &Theme, pins_path: &std::path::Path) -> Option<Dock> {
    let inst = GetModuleHandleW(PCWSTR::null()).ok()?;
    let ex = WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_LAYERED;
    let hwnd = CreateWindowExW(ex, w!("WinarchyShellDock"), w!(""), WS_POPUP, 0, 0, 1, 1, None, None, inst, None).ok()?;
    let tip = CreateWindowExW(ex | WS_EX_TRANSPARENT, w!("WinarchyShellDockTip"), w!(""), WS_POPUP, 0, 0, 1, 1, None, None, inst, None).ok()?;
    crate::accessibility::create(hwnd, "Winarchy", accesskit::Role::Toolbar);
    crate::dock_drop::register(hwnd);
    let mon = (r.left, r.top, r.right, r.bottom);
    let scale = crate::mon_scale(r);
    Some(Dock {
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
        preview_pick: None,
        key_target: None,
        key_witness: None,
        accessible_titles: HashMap::new(),
        tracking: false,
        scale,
        mon,
        sig: 0,
        hidden: false,
        pending_click: None,
        pins: load_pins(pins_path),
        ident: HashMap::new(),
        opened: HashMap::new(),
        pins_stamp: pins_stamp(pins_path),
        pins_path: pins_path.to_path_buf(),
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
        flashing: HashSet::new(),
        drag: None,
        pointer_down: None,
        peek: false,
        start_visible: false,
        leave: None,
        tip_born: None,
        glide: HashMap::new(),
        bar_glide: None,
        anim_on: false,
    })
}

struct InteractionCleanup {
    end_keyboard: bool,
    dismiss_menu: bool,
    restore_previous: bool,
}

fn owns_keyboard_foreground(owner: isize, menu: Option<(isize, isize)>, foreground: isize) -> bool {
    foreground == owner || menu.is_some_and(|(hwnd, dock)| dock == owner && hwnd == foreground)
}

fn reconcile_interaction(previous: &[(isize, MonRect, f32)], next: &[(MonRect, f32)], keyboard_owner: Option<isize>, menu: Option<(isize, isize)>, foreground: isize) -> InteractionCleanup {
    let affected = |owner| previous.iter().position(|(hwnd, _, _)| *hwnd == owner).is_none_or(|index| {
        next.get(index).is_none_or(|(rect, scale)| previous[index].1 != *rect || (previous[index].2 - scale).abs() > 0.01)
    });
    let end_keyboard = keyboard_owner.is_some_and(affected);
    let dismiss_menu = menu.is_some_and(|(_, owner)| affected(owner));
    let restore_previous = end_keyboard && keyboard_owner.is_some_and(|owner| owns_keyboard_foreground(owner, menu, foreground));
    InteractionCleanup { end_keyboard, dismiss_menu, restore_previous }
}

// Altas y bajas de monitores: un dock por pantalla; se conserva el HWND principal.
unsafe fn sync_docks() -> bool {
    if !crate::dock_menu::can_reconcile() { return false }
    let rects = crate::monitor_rects();
    CUR.set(0);
    let (theme, pins_path) = with(|d| (d.theme.clone(), d.pins_path.clone()));
    let previous = DOCKS.with(|d| d.borrow().iter().map(|d| (d.hwnd.0 as isize, d.mon, d.scale)).collect::<Vec<_>>());
    let next = rects.iter().map(|r| ((r.left, r.top, r.right, r.bottom), crate::mon_scale(r))).collect::<Vec<_>>();
    let menu = crate::dock_menu::identity();
    let cleanup = reconcile_interaction(&previous, &next, KEY_SESSION.get().map(|(owner, _, _)| owner), menu, GetForegroundWindow().0 as isize);
    if cleanup.end_keyboard { end_keyboard(cleanup.restore_previous); }
    if cleanup.dismiss_menu { crate::dock_menu::dismiss(); }
    for (index, (owner, old_rect, old_scale)) in previous.iter().enumerate() {
        if next.get(index).is_none_or(|(rect, scale)| rect != old_rect || (scale - old_scale).abs() > 0.01) {
            on_dock(*owner, || {
                let hwnd = with(|d| {
                    d.drag = None;
                    d.pointer_down = None;
                    d.hover = None;
                    d.pressed = None;
                    d.peek = false;
                    d.leave = None;
                    d.sig = 0;
                    d.hwnd
                });
                if GetCapture() == hwnd { let _ = ReleaseCapture(); }
                hide_tip();
            });
        }
    }
    let extra: Vec<Dock> = DOCKS.with(|v| {
        let mut v = v.borrow_mut();
        if v.len() > rects.len().max(1) {
            v.split_off(rects.len().max(1))
        } else {
            Vec::new()
        }
    });
    for d in &extra {
        crate::accessibility::destroy(d.hwnd);
        crate::dock_drop::revoke(d.hwnd);
        let _ = DestroyWindow(d.tip);
        let _ = DestroyWindow(d.hwnd);
    }
    while dock_count() < rects.len() {
        let Some(d) = create_dock(&rects[dock_count()], &theme, &pins_path) else { break };
        let h = d.hwnd;
        DOCKS.with(|v| v.borrow_mut().push(d));
        CUR.set(dock_count() - 1);
        register(h);
        refresh();
        SetTimer(h, T_TICK, 1000, None);
        sync_edge_timer(h);
    }
    CUR.set(0);
    true
}

// El timer del borde solo corre en modo `hide`; fuera de él el dock siempre se ve.
unsafe fn sync_edge_timer(h: HWND) {
    if with(|d| d.theme.dock_hide || d.theme.dock_auto) {
        SetTimer(h, T_EDGE, EDGE_MS, None);
    } else {
        let _ = KillTimer(h, T_EDGE);
        with(|d| {
            d.peek = false;
            d.start_visible = false;
            d.leave = None;
        });
    }
}

// Devuelve (en el borde que lo revela, sobre la barra ya visible). El borde son 2 px, ensanchados una altura de barra a cada lado.
fn reveal_zones(m: MonRect, b: R, bottom: bool, p: (i32, i32)) -> (bool, bool) {
    let on_mon = p.0 >= m.0 && p.0 < m.2 && p.1 >= m.1 && p.1 < m.3;
    let edge = if bottom { p.1 >= m.3 - 2 } else { p.1 < m.1 + 2 };
    let at_edge = on_mon && edge && p.0 >= b.x - b.h && p.0 < b.x + b.w + b.h;
    let on_bar = on_mon && p.0 >= b.x && p.0 < b.x + b.w && if bottom { p.1 >= b.y - 2 } else { p.1 < b.y + b.h + 2 };
    (at_edge, on_bar)
}

// Mouse en el borde del dock (o sobre él, ya visible): lo muestra; fuera por un rato (y sin menú abierto): lo esconde.
unsafe fn edge_tick(h: HWND) {
    let mut p = POINT::default();
    if GetCursorPos(&mut p).is_err() {
        return;
    }
    let fg = GetForegroundWindow();
    let mut pid = 0;
    GetWindowThreadProcessId(fg, Some(&mut pid));
    let exe = START_FOREGROUND.with(|cached| {
        let mut cached = cached.borrow_mut();
        if (cached.0, cached.1) != (fg.0 as isize, pid) {
            *cached = (fg.0 as isize, pid, exe_of(fg));
        }
        cached.2.clone()
    });
    let name = exe.rsplit(['\\', '/']).next().unwrap_or("");
    let monitor = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
    let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    let on_monitor = GetMonitorInfoW(monitor, &mut info).as_bool();
    let changed = with(|d| {
        let before_start = d.start_visible;
        d.start_visible = apps::show_start_surface(name, IsWindowVisible(fg).as_bool(), is_cloaked(fg), on_monitor && (info.rcMonitor.left, info.rcMonitor.top) == (d.mon.0, d.mon.1));
        if !d.hides() {
            return false;
        }
        let (at_edge, on_bar) = reveal_zones(d.mon, d.layout.bar, d.theme.position != "bottom", (p.x, p.y));
        let at_edge = at_edge || crate::cluster::at_edge(d.mon, p);
        let on_bar = on_bar || crate::cluster::contains(p);
        let before = d.peek;
        if at_edge || (d.peek && (on_bar || crate::dock_menu::is_open())) {
            d.peek = true;
            d.leave = None;
        } else if d.peek {
            let t = *d.leave.get_or_insert_with(std::time::Instant::now);
            if t.elapsed().as_millis() > LEAVE_MS {
                d.peek = false;
                d.leave = None;
            }
        }
        before != d.peek || before_start != d.start_visible
    });
    if changed {
        render();
        if with(|d| d.motion) {
            start_anim(h);
        }
    }
}

pub fn set_theme(theme: &Theme) {
    crate::cluster::set_theme(theme);
    for_each_dock(|| {
        with(|d| {
            d.theme = theme.clone();
            d.motion = motion_enabled(theme);
            d.sig = 0;
        });
        unsafe {
            sync_edge_timer(with(|d| d.hwnd));
            refresh()
        }
    });
}

pub fn shutdown() {
    dismiss_tray();
    crate::tray_flyout::close();
    crate::dock_menu::dismiss();
    crate::dock_thumb::hide();
    if KEY_SESSION.get().is_some() { end_keyboard(false); }
    crate::cluster::destroy();
    restore_native_taskbar();
    let docks = DOCKS.with(|d| std::mem::take(&mut *d.borrow_mut()));
    for d in docks {
        unsafe {
            crate::accessibility::destroy(d.hwnd);
            crate::dock_drop::revoke(d.hwnd);
            let _ = DeregisterShellHookWindow(d.hwnd);
            let _ = WTSUnRegisterSessionNotification(d.hwnd);
            let _ = DestroyWindow(d.tip);
            let _ = DestroyWindow(d.hwnd);
        }
    }
    komorebi::NOTIFY.store(0, std::sync::atomic::Ordering::Relaxed);
    drop(ASKS.lock().unwrap().take());
    READY.lock().unwrap().clear();
    CUR.set(0);
}

// El ajuste de Windows "efectos de animación" apagado manda sobre el theme.
pub(crate) fn motion_enabled(t: &Theme) -> bool {
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
            if !crate::tray_host::owns_window(h) {
                f(h);
            }
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

// Con el host de bandeja activo abre el flyout propio sobre el botón; sin él, la bandeja nativa.
fn open_tray() {
    if with(|d| d.layout.tray.w == 0) {
        return crate::cluster::open_tray();
    }
    let (button, mon, bottom, scale, theme) = with(|d| (R { x: d.layout.bar.x + d.layout.tray.x, y: d.layout.bar.y + d.layout.tray.y, w: d.layout.tray.w, h: d.layout.tray.h }, d.mon, d.theme.position != "bottom", d.scale, d.theme.clone()));
    if !crate::tray_flyout::toggle(button, mon, bottom, scale, &theme) {
        show_tray();
    }
}

pub fn show_tray() {
    if dock_count() == 0 { return }
    unsafe {
        if TRAY_SESSION.get().is_none() {
            let h = DOCKS.with(|d| d.borrow()[0].hwnd.0 as isize);
            let generation = TRAY_SEQUENCE.get().wrapping_add(1);
            TRAY_SEQUENCE.set(generation);
            let (tx, rx) = std::sync::mpsc::sync_channel(0);
            std::thread::spawn(move || tray_observer(h, generation, tx));
            let Ok(Some(thread)) = rx.recv_timeout(std::time::Duration::from_secs(1)) else { return };
            TRAY_SESSION.set(Some((thread, generation)));
            TRAY_FOCUS_SEEN.set(false);
        }
        TRAY_BASE_FOCUS.set(GetForegroundWindow().0 as isize);
        TRAY_PENDING.set(true);
        TRAY_FALLBACK.set(false);
        set_native_taskbar(false);
        for_native_taskbars(|h| { let _ = SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW); });
        SetTimer(DOCKS.with(|d| d.borrow()[0].hwnd), T_TRAY_ACTIVATE, 50, None);
    }
}

unsafe fn tray_surface(h: HWND) -> bool {
    let mut cls = [0u16; 128];
    let n = GetClassNameW(h, &mut cls).max(0) as usize;
    let class = String::from_utf16_lossy(&cls[..n]);
    let exe = if apps::tray_click_dismisses(&class) { exe_of(h) } else { String::new() };
    apps::tray_surface(&class, &exe)
}

unsafe fn tray_foreground() -> (bool, bool, bool, bool) {
    let fg = GetForegroundWindow();
    let mut cls = [0u16; 128];
    let n = GetClassNameW(fg, &mut cls).max(0) as usize;
    let class = String::from_utf16_lossy(&cls[..n]);
    let taskbar = matches!(class.as_str(), "Shell_TrayWnd" | "Shell_SecondaryTrayWnd");
    let tray_window = tray_surface(fg);
    let mut gui = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
    let menu = GetGUIThreadInfo(0, &mut gui).is_ok() && (gui.flags.0 & GUI_INMENUMODE.0 != 0);
    (fg.0 as isize == TRAY_BASE_FOCUS.get(), tray_window, menu, taskbar)
}

unsafe fn activate_tray(h: HWND) {
    if TRAY_SESSION.get().is_none() || !TRAY_PENDING.get() { return }
    let (same, tray, menu, _) = tray_foreground();
    if apps::tray_wait_cancel(true, same, tray, menu) { dismiss_tray(); return }
    let down = |k| GetAsyncKeyState(k) < 0;
    if !apps::tray_activation_ready(down(VK_LWIN.0 as i32) || down(VK_RWIN.0 as i32), down(VK_CONTROL.0 as i32), down(VK_MENU.0 as i32), down(VK_SHIFT.0 as i32)) { return }
    let _ = KillTimer(h, T_TRAY_ACTIVATE);
    TRAY_PENDING.set(false);
    let key = |vk, flags| INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, dwFlags: flags, ..Default::default() } } };
    let keys = [key(VK_LWIN, Default::default()), key(VK_B, Default::default()), key(VK_B, KEYEVENTF_KEYUP), key(VK_LWIN, KEYEVENTF_KEYUP)];
    let sent = SendInput(&keys, std::mem::size_of::<INPUT>() as i32);
    if sent != keys.len() as u32 {
        if sent == 2 { let _ = SendInput(&keys[2..], std::mem::size_of::<INPUT>() as i32); }
        else if sent == 1 || sent == 3 { let _ = SendInput(&keys[3..], std::mem::size_of::<INPUT>() as i32); }
        TRAY_FALLBACK.set(true);
        return;
    }
    TRAY_FOCUS_STARTED.set(Some(std::time::Instant::now()));
    SetTimer(h, T_TRAY_FOCUS, 200, None);
}

fn tray_observer(target: isize, generation: usize, ready: std::sync::mpsc::SyncSender<Option<u32>>) {
    unsafe {
        let mut msg = MSG::default();
        let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
        TRAY_OBSERVER.set((target, generation));
        let Ok(hook) = SetWindowsHookExW(WH_MOUSE_LL, Some(tray_mouse_hook), None, 0) else { let _ = ready.send(None); return };
        let focus = SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, None, Some(tray_focus_hook), 0, 0, WINEVENT_OUTOFCONTEXT);
        if focus.0.is_null() { let _ = UnhookWindowsHookEx(hook); let _ = ready.send(None); return }
        if ready.send(Some(GetCurrentThreadId())).is_ok() {
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let _ = UnhookWinEvent(focus);
        let _ = UnhookWindowsHookEx(hook);
    }
}

unsafe extern "system" fn tray_focus_hook(_: HWINEVENTHOOK, _: u32, hwnd: HWND, _: i32, _: i32, _: u32, _: u32) {
    let (target, generation) = TRAY_OBSERVER.get();
    let _ = PostMessageW(HWND(target as *mut _), WM_TRAY_FOREGROUND, WPARAM(generation), LPARAM(hwnd.0 as isize));
}

unsafe extern "system" fn tray_mouse_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 && matches!(wp.0 as u32, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN) {
        let p = (*(lp.0 as *const MSLLHOOKSTRUCT)).pt;
        let root = GetAncestor(WindowFromPoint(p), GA_ROOT);
        let (target, generation) = TRAY_OBSERVER.get();
        let _ = PostMessageW(HWND(target as *mut _), WM_TRAY_DISMISS, WPARAM(generation), LPARAM(root.0 as isize));
    }
    CallNextHookEx(None, code, wp, lp)
}

pub fn dismiss_tray() {
    let Some((thread, _)) = TRAY_SESSION.take() else { return };
    TRAY_FOCUS_SEEN.set(false);
    TRAY_FOCUS_STARTED.set(None);
    TRAY_PENDING.set(false);
    TRAY_FALLBACK.set(false);
    TRAY_BASE_FOCUS.set(0);
    unsafe {
        if dock_count() > 0 {
            let h = DOCKS.with(|d| d.borrow()[0].hwnd);
            let _ = KillTimer(h, T_TRAY_FOCUS);
            let _ = KillTimer(h, T_TRAY_ACTIVATE);
        }
        let _ = PostThreadMessageW(thread, WM_QUIT, WPARAM(0), LPARAM(0));
        if dock_count() > 0 && with(|d| d.theme.hide_native_taskbar) { set_native_taskbar(true); }
    }
}

fn request_control_center() {
    let path = with(|d| d.pins_path.with_file_name("show-menu.flag"));
    let _ = std::fs::OpenOptions::new().write(true).create_new(true).open(path);
}

fn request_start() {
    let path = with(|d| d.pins_path.with_file_name("show-start.flag"));
    let _ = std::fs::OpenOptions::new().write(true).create_new(true).open(path);
}

pub(crate) unsafe fn is_elevated(h: HWND) -> bool {
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
    with(|d| {
        d.anim_at = std::time::Instant::now();
        d.anim_on = true;
    });
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
        let target = if d.want_visible() { 1.0 } else { 0.0 };
        let v = approach(d.vis, target, dt, FADE_MS);
        busy |= v != d.vis;
        d.vis = v;
        busy |= glide_step(d, dt);
        busy
    });
    render();
    if !busy {
        let _ = KillTimer(h, T_ANIM);
        with(|d| d.anim_on = false);
    }
}

// Posición destino (x de pantalla) de cada celda y de la barra; las celdas se deslizan hacia ahí en vez de saltar.
fn glide_targets(d: &Dock) -> (Vec<(String, f32)>, f32, f32) {
    let n = d.layout.cells.len();
    let cells = d
        .layout
        .cells
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let key = if d.layout.overflow > 0 && i + 1 == n { "\u{0}ovf".to_string() } else { d.items.get(i).map(|x| x.key.clone()).unwrap_or_default() };
            (key, (d.layout.bar.x + c.x) as f32)
        })
        .collect();
    (cells, d.layout.bar.x as f32, d.layout.bar.w as f32)
}

// Lo que se dibuja: x de cada celda relativa a la barra y el rectángulo de la barra, más si todavía falta recorrido.
fn glide_view(d: &mut Dock) -> (Vec<i32>, R, bool) {
    let (tg, bx, bw) = glide_targets(d);
    if !d.motion {
        d.glide = tg.iter().cloned().collect();
        d.bar_glide = Some((bx, bw));
    } else {
        d.glide.retain(|k, _| tg.iter().any(|(t, _)| t == k));
        for (k, x) in &tg {
            d.glide.entry(k.clone()).or_insert(*x);
        }
        d.bar_glide.get_or_insert((bx, bw));
    }
    let (cx, cw) = d.bar_glide.unwrap_or((bx, bw));
    let busy = (cx - bx).abs() >= 0.5 || (cw - bw).abs() >= 0.5 || tg.iter().any(|(k, x)| (d.glide[k] - x).abs() >= 0.5);
    let xs = tg.iter().map(|(k, _)| (d.glide[k] - cx).round() as i32).collect();
    (xs, R { x: cx.round() as i32, y: d.layout.bar.y, w: cw.round() as i32, h: d.layout.bar.h }, busy)
}

fn display_hit(d: &mut Dock, x: i32, y: i32) -> Option<Hit> {
    let (xs, bar, _) = glide_view(d);
    hit_test(&display_layout(&d.layout, &xs, bar), x + bar.x, y + bar.y)
}

fn glide_step(d: &mut Dock, dt: f32) -> bool {
    if !d.motion {
        return false;
    }
    let k = 1.0 - (-dt / GLIDE_TAU_MS).exp();
    let (tg, bx, bw) = glide_targets(d);
    let mut busy = false;
    let mut go = |cur: &mut f32, to: f32| {
        *cur += (to - *cur) * k;
        if (to - *cur).abs() < 0.5 {
            *cur = to;
        } else {
            busy = true;
        }
    };
    for (key, x) in &tg {
        if let Some(c) = d.glide.get_mut(key) {
            go(c, *x);
        }
    }
    if let Some((cx, cw)) = d.bar_glide.as_mut() {
        go(cx, bx);
        go(cw, bw);
    }
    busy
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

fn filetime_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| (d.as_nanos() / 100) as u64 + 116_444_736_000_000_000)
}

unsafe fn process_started(h: HWND) -> Option<u64> {
    let mut pid = 0u32;
    GetWindowThreadProcessId(h, Some(&mut pid));
    let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut t = [FILETIME::default(); 4];
    let ok = GetProcessTimes(p, &mut t[0], &mut t[1], &mut t[2], &mut t[3]).is_ok();
    let _ = CloseHandle(p);
    ok.then(|| (t[0].dwHighDateTime as u64) << 32 | t[0].dwLowDateTime as u64)
}

pub(crate) unsafe fn exe_of(h: HWND) -> String {
    let mut pid = 0u32;
    GetWindowThreadProcessId(h, Some(&mut pid));
    let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return String::new() };
    let mut buf = [0u16; 520];
    let mut n = buf.len() as u32;
    let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
    let _ = CloseHandle(p);
    if ok { String::from_utf16_lossy(&buf[..n as usize]) } else { String::new() }
}

fn wait_for_explorer_shell(timeout: std::time::Duration, mut is_ready: impl FnMut() -> bool) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if is_ready() {
            return true;
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return false;
        }
        std::thread::sleep(remaining.min(std::time::Duration::from_millis(100)));
    }
}

fn system_explorer_shell_responsive() -> bool {
    unsafe {
        let mut previous = HWND::default();
        while let Ok(hwnd) = FindWindowExW(None, previous, w!("Shell_TrayWnd"), PCWSTR::null()) {
            previous = hwnd;
            if !exe_of(hwnd).eq_ignore_ascii_case(system_explorer()) {
                continue;
            }
            let mut result = 0usize;
            if SendMessageTimeoutW(hwnd, WM_NULL, WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG | SMTO_ERRORONEXIT, 500, Some(&mut result)).0 != 0
                && exe_of(hwnd).eq_ignore_ascii_case(system_explorer())
            {
                return true;
            }
        }
        false
    }
}

pub fn start_system_explorer() -> bool {
    if system_explorer_shell_responsive() {
        return true;
    }
    let path = system_explorer();
    if path.is_empty() || std::process::Command::new(path).spawn().is_err() {
        return false;
    }
    let mut ready_samples = 0;
    wait_for_explorer_shell(std::time::Duration::from_secs(20), || {
        if system_explorer_shell_responsive() {
            ready_samples += 1;
        } else {
            ready_samples = 0;
        }
        ready_samples >= 2
    })
}

fn system_explorer() -> &'static str {
    static PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    PATH.get_or_init(|| unsafe {
        use windows::Win32::System::SystemInformation::GetWindowsDirectoryW;
        let mut buf = vec![0u16; 32768];
        let len = GetWindowsDirectoryW(Some(&mut buf)) as usize;
        if len == 0 || len >= buf.len() { return String::new() }
        std::path::Path::new(&String::from_utf16_lossy(&buf[..len])).join("explorer.exe").to_string_lossy().into_owned()
    })
}

fn explorer_aumid(aumid: Option<&str>, exe: &str, system_explorer: &str) -> Option<String> {
    aumid.filter(|id| !id.is_empty()).map(str::to_owned).or_else(|| {
        (!system_explorer.is_empty() && exe.eq_ignore_ascii_case(system_explorer)).then(|| "Microsoft.Windows.Explorer".to_string())
    })
}

unsafe fn aumid_of(h: HWND) -> Option<String> {
    use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
    let store: windows::core::Result<IPropertyStore> = SHGetPropertyStoreForWindow(h);
    let from_window = store.ok().and_then(|s| variant_string(&s));
    from_window.or_else(|| packaged_aumid(h))
}

// Las apps empaquetadas (WhatsApp, Calculadora) no ponen AUMID en la ventana pero sí en el proceso; sin él el ícono sale del .exe protegido y queda genérico.
unsafe fn packaged_aumid(h: HWND) -> Option<String> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetApplicationUserModelId(process: HANDLE, len: *mut u32, buf: *mut u16) -> i32;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(h, Some(&mut pid));
    let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buf = [0u16; 130];
    let mut n = buf.len() as u32;
    let rc = GetApplicationUserModelId(p, &mut n, buf.as_mut_ptr());
    let _ = CloseHandle(p);
    (rc == 0 && n > 1).then(|| String::from_utf16_lossy(&buf[..n as usize - 1]))
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
    let (mut opened, seeded) = with(|d| (std::mem::take(&mut d.opened), d.seeded));
    opened.retain(|h, _| IsWindow(HWND(*h as *mut _)).as_bool());
    let appeared = filetime_now();
    // Electron fija el AUMID tarde: una lectura sin AUMID caduca a los 5 s y se reintenta.
    let now = std::time::Instant::now();
    ident.retain(|k, v| eligible.iter().any(|h| h.0 as isize == *k) && (v.1.is_some() || now.duration_since(v.2).as_secs() < 5));
    for h in eligible {
        let (exe, aumid, _) = ident.entry(h.0 as isize).or_insert_with(|| (exe_of(h), aumid_of(h), now)).clone();
        let aumid = explorer_aumid(aumid.as_deref(), &exe, system_explorer());
        let key = apps::group_key(aumid.as_deref(), &exe);
        let id = h.0 as isize;
        // lo que ya estaba al arrancar el dock no tiene hora de aparición: se usa la de su proceso
        opened.entry(id).or_insert_with(|| if seeded { appeared } else { process_started(h).unwrap_or(appeared) });
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
    for g in &mut groups {
        g.wins.sort_by_key(|w| (opened[w], *w));
    }
    groups.sort_by_key(|g| (opened[&g.wins[0]], g.wins[0]));
    with(|d| {
        d.ident = ident;
        d.opened = opened;
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
        reconcile_keyboard_target(d);
        let keys: Vec<String> = d.items.iter().map(|i| i.key.clone()).collect();
        for k in keys {
            if d.known.insert(k.clone()) && d.seeded && d.motion {
                d.born.insert(k, 0.0);
            }
        }
        d.known.retain(|k| d.items.iter().any(|i| &i.key == k));
        d.seeded |= !d.items.is_empty();
    });
    let titles = with(|d| d.items.iter().filter_map(|item| item.wins.first().map(|hwnd| (accessible_group(item), *hwnd))).collect::<Vec<_>>());
    let titles = titles.into_iter().filter_map(|(key, hwnd)| window_titles(&[hwnd]).into_iter().next().filter(|title| !title.is_empty()).map(|title| (key, title))).collect();
    with(|d| d.accessible_titles = titles);
    render();
    if with(|d| !d.born.is_empty()) {
        start_anim(with(|d| d.hwnd));
    }
}

fn signature(d: &Dock) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for i in &d.items {
        (&i.key, i.wins.len(), i.active, i.other_ws, i.wins.iter().any(|w| d.flashing.contains(w)), d.accessible_titles.get(&accessible_group(i))).hash(&mut h);
    }
    (&d.key_target, d.hover, d.pressed, d.icons.len(), d.scale.to_bits(), d.mon, d.hidden, d.vis.to_bits(), d.peek, d.start_visible, d.hides()).hash(&mut h);
    (&d.theme.background, &d.theme.accent, &d.theme.position).hash(&mut h);
    d.bar_glide.map(|(x, w)| (x.to_bits(), w.to_bits())).hash(&mut h);
    for i in &d.items {
        d.glide.get(&i.key).map(|v| v.to_bits()).hash(&mut h);
    }
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
            let cov = if rad <= 0.0 { 1.0 } else { (rad - ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt() + 0.5).clamp(0.0, 1.0) };
            if cov > 0.0 {
                blend(&mut buf[(y * stride + x) as usize], premul(col, alpha * cov));
            }
        }
    }
}

type MarkQuad = [(i32, i32); 4];

fn brand_mark_polygons(cell: R, scale: f32) -> [MarkQuad; 4] {
    const MARK: [[(f32, f32); 4]; 4] = [
        [(0.0, 0.0), (14.0, 0.0), (25.0, 56.0), (11.0, 56.0)],
        [(28.5, 56.0), (42.5, 56.0), (50.2, 16.8), (36.2, 16.8)],
        [(53.7, 16.8), (67.7, 16.8), (75.4, 56.0), (61.4, 56.0)],
        [(78.9, 56.0), (92.9, 56.0), (103.9, 0.0), (89.9, 0.0)],
    ];
    let w = px(24, scale).min(cell.w - px(8, scale)).max(1);
    let h = (w as f32 * 56.0 / 104.0).round() as i32;
    let (left, top) = (cell.x + (cell.w - w) / 2, cell.y + (cell.h - h) / 2);
    MARK.map(|quad| quad.map(|(x, y)| (left + (x * w as f32 / 104.0).round() as i32, top + (y * h as f32 / 56.0).round() as i32)))
}

fn quad_contains(points: MarkQuad, x: f32, y: f32) -> bool {
    let mut inside = false;
    let mut j = 3;
    for i in 0..4 {
        let (xi, yi) = (points[i].0 as f32, points[i].1 as f32);
        let (xj, yj) = (points[j].0 as f32, points[j].1 as f32);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn fill_quad_a(buf: &mut [u32], stride: i32, rows: i32, points: MarkQuad, color: (u8, u8, u8), alpha: f32) {
    let left = points.iter().map(|p| p.0).min().unwrap_or(0).max(0);
    let right = points.iter().map(|p| p.0).max().unwrap_or(0).min(stride);
    let top = points.iter().map(|p| p.1).min().unwrap_or(0).max(0);
    let bottom = points.iter().map(|p| p.1).max().unwrap_or(0).min(rows);
    for y in top..bottom {
        for x in left..right {
            let coverage = [(0.25f32, 0.25f32), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)]
                .into_iter()
                .filter(|(sx, sy)| quad_contains(points, x as f32 + sx, y as f32 + sy))
                .count();
            if coverage > 0 {
                blend(&mut buf[(y * stride + x) as usize], premul(color, alpha * coverage as f32 / 4.0));
            }
        }
    }
}

fn brand_mark_colors(foreground: (u8, u8, u8), accent: (u8, u8, u8)) -> [(u8, u8, u8); 4] {
    [foreground, foreground, foreground, accent]
}

fn draw_brand_mark(buf: &mut [u32], stride: i32, rows: i32, cell: R, scale: f32, foreground: (u8, u8, u8), accent: (u8, u8, u8)) {
    for (quad, color) in brand_mark_polygons(cell, scale).into_iter().zip(brand_mark_colors(foreground, accent)) {
        fill_quad_a(buf, stride, rows, quad, color, 1.0);
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

// shortcut: only Files redraws its own window icon with the theme; other apps keep the shell icon of their exe, which is sharper. Generalize if more apps do the same.
fn is_files(i: &Item) -> bool {
    i.exe.to_lowercase().ends_with("winarchy-files.exe")
}

fn icon_sources(i: &Item, pins: &[Pin]) -> Vec<String> {
    let mut v = Vec::new();
    if is_files(i) {
        v.extend(i.wins.first().map(|w| format!("window:{w}")));
    }
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

fn draw_attention_marker(buf: &mut [u32], w: i32, h: i32, marker: R, border: i32, urgent: (u8, u8, u8)) {
    fill_round_a(buf, w, h, R { x: marker.x - border, y: marker.y - border, w: marker.w + 2 * border, h: marker.h + 2 * border }, (marker.h + 2 * border) / 2, crate::theme::contrasting_color(urgent), 1.0);
    fill_round_a(buf, w, h, marker, marker.h / 2, urgent, 1.0);
}

// Texto centrado en una celda: GDI dibuja blanco sobre negro y la luminancia pasa a cobertura (como el tooltip).
pub(crate) unsafe fn draw_label(buf: &mut [u32], stride: i32, rows: i32, cell: R, text: &str, family: &str, scale: f32, fg: (u8, u8, u8), font_size: i32) {
    let (w, h) = (cell.w, cell.h);
    let wide: Vec<u16> = text.encode_utf16().collect();
    let family: Vec<u16> = family.encode_utf16().chain(std::iter::once(0)).collect();
    let screen = GetDC(None);
    let mem = CreateCompatibleDC(screen);
    let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: 0, ..Default::default() }, ..Default::default() };
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    if let Ok(dib) = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
        let old = SelectObject(mem, dib);
        let font = CreateFontW(-px(font_size.max(11), scale), 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, ANTIALIASED_QUALITY.0 as u32, 0, PCWSTR(family.as_ptr()));
        let old_font = SelectObject(mem, font);
        let src = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
        src.fill(0);
        SetBkMode(mem, TRANSPARENT);
        SetTextColor(mem, COLORREF(0x00FF_FFFF));
        let mut rc = RECT { left: 0, top: 0, right: w, bottom: h };
        let mut tmp = wide.clone();
        DrawTextW(mem, &mut tmp, &mut rc, DT_CENTER | DT_VCENTER | DT_NOPREFIX | DT_SINGLELINE | DT_END_ELLIPSIS);
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
    let (hwnd, theme, items, layout_, hover, scale, want_vis, motion, pressed) = with(|d| {
        let reserve = crate::cluster::owns(&d.theme.cluster_monitor, d.mon, CUR.get());
        d.layout = layout_for(d.items.len(), d.scale, d.mon, d.theme.position != "bottom", style(&d.theme).gap, reserve);
        if let Some(target) = d.key_target.as_ref() {
            let visible = d.layout.cells.len().saturating_sub(usize::from(d.layout.overflow > 0));
            let normalized = normalize_keyboard_target(target, &d.items, visible, d.layout.overflow);
            if &normalized != target { d.key_witness = None; }
            d.key_target = Some(normalized);
        }
        (d.hwnd, d.theme.clone(), d.items.clone(), Layout { bar: d.layout.bar, brand: d.layout.brand, tray: d.layout.tray, cells: d.layout.cells.clone(), overflow: d.layout.overflow }, d.hover, d.scale, d.want_visible(), d.motion, d.pressed)
    });
    if layout_.tray.w == 0 {
        // el botón de bandeja vive en el cluster: el foco de teclado se dibuja sobre su chevron
        crate::cluster::set_key_focus(with(|d| d.key_target == Some(KeyTarget::Tray)));
    }
    let (xs, bar_disp, glide_busy) = with(|d| glide_view(d));
    if glide_busy && !with(|d| d.anim_on) {
        unsafe { start_anim(hwnd) };
    }
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
    let (titles, focused) = with(|d| (d.accessible_titles.clone(), d.key_target.clone()));
    publish_accessibility(hwnd, &items, &display_layout(&layout_, &xs, bar_disp), &titles, focused, want_vis);
    let want_vis = if want_vis { 1.0 } else { 0.0 };
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
        let (mon, index) = with(|d| (d.mon, CUR.get()));
        let cluster = crate::cluster::owns(&theme.cluster_monitor, mon, index);
        if vis == 0.0 && want_vis == 0.0 {
            let _ = ShowWindow(hwnd, SW_HIDE);
            if cluster {
                crate::cluster::sync(&theme, mon, scale, bar_disp.x + bar_disp.w, 0.0, 0);
            }
            return;
        }
        let (w, h) = (bar_disp.w, bar_disp.h);
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
        let urgent = theme.urgent.as_deref().map(rgb).unwrap_or(accent);
        fill_round_a(buf, w, h, R { x: 0, y: 0, w, h }, px(st.radius, scale), bg, st.opacity);
        let key_target = with(|d| d.key_target.clone());
        let brand_alpha = if key_target == Some(KeyTarget::Brand) { 0.50 } else if pressed == Some(HUB_INDEX) { 0.28 } else if hover == Some(HUB_INDEX) { 0.16 } else { 0.0 };
        if brand_alpha > 0.0 {
            fill_round_a(buf, w, h, layout_.brand, px(10, scale), accent, brand_alpha);
        }
        draw_brand_mark(buf, w, h, layout_.brand, scale, fg, accent);
        let tray = R { x: w - (layout_.bar.w - layout_.tray.x), ..layout_.tray };
        let tray_alpha = if layout_.tray.w == 0 { 0.0 } else if key_target == Some(KeyTarget::Tray) { 0.50 } else if pressed == Some(TRAY_INDEX) { 0.28 } else if hover == Some(TRAY_INDEX) { 0.16 } else { 0.0 };
        if tray_alpha > 0.0 { fill_round_a(buf, w, h, tray, px(10, scale), accent, tray_alpha); }
        if tray.w > 0 {
            let (cx, cy, s) = (tray.x + tray.w / 2, tray.y + tray.h / 2, px(1, scale).max(1));
            fill_quad_a(buf, w, h, [(cx - 6*s, cy + 2*s), (cx, cy - 4*s), (cx + s, cy - 2*s), (cx - 5*s, cy + 4*s)], fg, 1.0);
            fill_quad_a(buf, w, h, [(cx, cy - 4*s), (cx + 6*s, cy + 2*s), (cx + 5*s, cy + 4*s), (cx - s, cy - 2*s)], fg, 1.0);
        }
        DOCKS.with(|v| {
            let v = v.borrow();
            let d = &v[CUR.get().min(v.len() - 1)];
            let icons = &d.icons;
            for (idx, cell) in layout_.cells.iter().enumerate() {
                let cell = &R { x: xs[idx], ..*cell };
                if layout_.overflow > 0 && idx == layout_.cells.len() - 1 {
                    fill_round_a(buf, w, h, *cell, px(10, scale), if key_target == Some(KeyTarget::Overflow) { accent } else { fg }, if key_target == Some(KeyTarget::Overflow) { 0.50 } else { 0.12 });
                    draw_label(buf, w, h, *cell, &format!("+{}", layout_.overflow), &theme.font_family, scale, fg, theme.font_size);
                    continue;
                }
                let Some(item) = items.get(idx) else { continue };
                let (hv, entry) = if motion { (ease(d.hov.get(&item.key).copied().unwrap_or(0.0)), ease(d.born.get(&item.key).copied().unwrap_or(1.0))) } else { ((hover == Some(idx)) as i32 as f32, 1.0) };
                let attn = !item.active && item.wins.iter().any(|w| flashing.contains(w));
                if key_target.as_ref() == Some(&KeyTarget::Item(group_id(item))) {
                    fill_round_a(buf, w, h, *cell, px(10, scale), accent, 0.50);
                } else if item.active {
                    fill_round_a(buf, w, h, *cell, px(10, scale), accent, 0.28);
                } else if attn {
                    fill_round_a(buf, w, h, *cell, px(10, scale), urgent, 0.38);
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
                let marker = R { x: cell.x + (cell.w - dw) / 2, y: cell.y + cell.h - dot - px(3, scale), w: dw, h: dot };
                if attn {
                    draw_attention_marker(buf, w, h, marker, px(1, scale).max(1), urgent);
                } else {
                    fill_round_a(buf, w, h, marker, dot / 2, if item.active { accent } else { fg }, if item.active { 1.0 } else { 0.6 });
                }
            }
        });
        // En `hide` el dock se desliza hacia su borde al esconderse.
        let slide = if with(|d| d.hides()) { ((1.0 - ease(vis)) * px(16, scale) as f32) as i32 } else { 0 };
        let pos = POINT { x: bar_disp.x, y: bar_disp.y + if theme.position != "bottom" { slide } else { -slide } };
        let sz = SIZE { cx: w, cy: h };
        let src = POINT::default();
        let bf = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: (vis * 255.0).round() as u8, AlphaFormat: 1 };
        let _ = UpdateLayeredWindow(hwnd, None, Some(&pos), Some(&sz), mem, Some(&src), COLORREF(0), Some(&bf), ULW_ALPHA);
        SelectObject(mem, old);
        let _ = DeleteObject(dib);
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        if cluster {
            crate::cluster::sync(&theme, mon, scale, bar_disp.x + bar_disp.w, vis, slide);
        }
    }
}

fn drain_ready() {
    let ready: Vec<_> = std::mem::take(&mut *READY.lock().unwrap());
    if ready.is_empty() {
        return;
    }
    for_each_dock(|| {
        with(|d| {
            for (k, size, ic) in &ready {
                d.icons.insert((k.clone(), *size), ic.clone());
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
    });
}

// The window icon changes at runtime (theme), so the icons of such windows are asked for again every tick; the worker skips unchanged ones.
fn refresh_window_icons() {
    let asks: Vec<Ask> = with(|d| {
        let size = px(style(&d.theme).icon, d.scale);
        d.items.iter().filter(|i| is_files(i) && !i.wins.is_empty()).map(|i| (i.key.clone(), size, icon_sources(i, &d.pins))).collect()
    });
    if let Some(tx) = ASKS.lock().unwrap().as_ref() {
        for a in asks {
            let _ = tx.send(a);
        }
    }
}

static WINDOW_ICONS: Mutex<Option<HashMap<String, usize>>> = Mutex::new(None);

fn window_icon_unchanged(sources: &[String]) -> bool {
    let Some(source) = sources.first().filter(|s| s.starts_with("window:")) else { return false };
    let hwnd: isize = source[7..].parse().unwrap_or(0);
    let mut handle = 0usize;
    unsafe {
        let _ = SendMessageTimeoutW(HWND(hwnd as *mut _), WM_GETICON, WPARAM(ICON_BIG as usize), LPARAM(0), SMTO_ABORTIFHUNG, 200, Some(&mut handle));
    }
    let mut known = WINDOW_ICONS.lock().unwrap();
    let known = known.get_or_insert_with(HashMap::new);
    handle != 0 && known.get(source) == Some(&handle)
}

fn spawn_icon_worker(target: isize) {
    let (tx, rx) = std::sync::mpsc::channel::<Ask>();
    *ASKS.lock().unwrap() = Some(tx);
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        while let Ok((key, size, sources)) = rx.recv() {
            if window_icon_unchanged(&sources) {
                continue;
            }
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

unsafe fn fetch_window_icon(source: &str) -> Option<Icon> {
    let hwnd: isize = source[7..].parse().ok()?;
    let mut handle = 0usize;
    let _ = SendMessageTimeoutW(HWND(hwnd as *mut _), WM_GETICON, WPARAM(ICON_BIG as usize), LPARAM(0), SMTO_ABORTIFHUNG, 200, Some(&mut handle));
    if handle == 0 {
        return None;
    }
    let mut info = ICONINFO::default();
    GetIconInfo(HICON(handle as *mut _), &mut info).ok()?;
    let _ = DeleteObject(info.hbmMask);
    let icon = icon_from_bitmap(info.hbmColor);
    if icon.is_some() {
        WINDOW_ICONS.lock().unwrap().get_or_insert_with(HashMap::new).insert(source.to_owned(), handle);
    }
    icon
}

unsafe fn fetch_icon(source: &str, size: i32) -> Option<Icon> {
    if source.starts_with("window:") {
        return fetch_window_icon(source);
    }
    let wide: Vec<u16> = source.encode_utf16().chain(std::iter::once(0)).collect();
    let item: IShellItemImageFactory = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None).ok()?;
    let bmp = item.GetImage(SIZE { cx: size, cy: size }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK).ok()?;
    icon_from_bitmap(bmp)
}

unsafe fn icon_from_bitmap(bmp: HBITMAP) -> Option<Icon> {
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
        shell_like: fg == GetShellWindow() || matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "WinarchyDesktopWallpaper"),
        on_dock_monitor: (mi.rcMonitor.left, mi.rcMonitor.top) == (mon.0, mon.1),
    })
}

// La taskbar nativa en auto-hide, al aparecer, queda sobre el dock: mientras se vea (o su overflow) no se reafirma el orden.
unsafe fn native_taskbar_visible(mon: MonRect) -> bool {
    let over = Cell::new(false);
    for_native_taskbars(|t| {
        let mut rc = RECT::default();
        if GetWindowRect(t, &mut rc).is_ok() && IsWindowVisible(t).as_bool() && rc.top < mon.3 - 4 && rc.bottom > mon.3 - 4 && rc.left < mon.2 && rc.right > mon.0 {
            over.set(true);
        }
    });
    over.get() || FindWindowW(w!("TopLevelWindowForOverflowXamlIsland"), PCWSTR::null()).is_ok_and(|o| IsWindowVisible(o).as_bool())
}

unsafe fn tick() {
    let cur = CUR.get();
    let reconciled = sync_docks();
    CUR.set(cur);
    if !reconciled { return }
    // shortcut: sin Explorer no llegan los mensajes del shell hook; este sondeo de 1 s mantiene la lista al día
    queue_refresh(with(|d| d.hwnd));
    let rects = crate::monitor_rects();
    let Some(r0) = rects.get(cur).copied() else { return };
    let (hwnd, hidden) = with(|d| (d.hwnd, d.hidden));
    let m0 = (r0.left, r0.top, r0.right, r0.bottom);
    let busy = matches!(SHQueryUserNotificationState(), Ok(s) if s == QUNS_BUSY || s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_PRESENTATION_MODE);
    let hide = apps::should_hide(dock_foreground(m0).as_ref(), m0, busy);
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
    let want_hide = with(|d| d.theme.hide_native_taskbar);
    let tray_open = TRAY_SESSION.get().is_some();
    if want_hide && !tray_open {
        set_native_taskbar(true);
    } else if !want_hide {
        restore_native_taskbar();
    }
    if hide && !hidden {
        if KEY_SESSION.get().is_some_and(|(owner, _, _)| owner == hwnd.0 as isize) { end_keyboard(false); }
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
pub(crate) unsafe fn raise(hwnd: HWND) {
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

unsafe fn set_activatable(h: HWND, on: bool) {
    let ex = GetWindowLongW(h, GWL_EXSTYLE);
    let want = if on { ex & !(WS_EX_NOACTIVATE.0 as i32) } else { ex | WS_EX_NOACTIVATE.0 as i32 };
    if want != ex {
        SetWindowLongW(h, GWL_EXSTYLE, want);
    }
}

unsafe fn start_in_front() -> bool {
    apps::is_start_surface(exe_of(GetForegroundWindow()).rsplit(['\\', '/']).next().unwrap_or(""))
}

pub(crate) unsafe fn focus_window(hwnd: HWND) {
    let dock = with(|d| d.hwnd);
    if !IsWindow(hwnd).as_bool() {
        queue_refresh(dock);
        return;
    }
    // El botón puede ser un owner invisible (aLogin) cuya ventana real gestiona komorebi: se enfoca esa, como Alt+Tab.
    let popup = GetLastActivePopup(hwnd);
    let (hwnd, id) = match komorebi::window_slot(hwnd.0 as isize) {
        None if komorebi::window_slot(popup.0 as isize).is_some() => (popup, popup.0 as isize),
        _ => (hwnd, hwnd.0 as isize),
    };
    if IsIconic(hwnd).as_bool() {
        let _ = ShowWindow(hwnd, SW_RESTORE);
    }
    if is_cloaked(hwnd) {
        komorebi::bring_to_front(id);
    } else {
        raise(hwnd);
    }
}

enum Replay { Click(String), Menu(String), Overflow }

// Con Inicio al frente Windows rechaza el foco y el menú: la acción espera a que Inicio deje el frente (lo cierra la
// activación del dock en WM_MOUSEACTIVATE); sobre un juego tarda más de un segundo, de ahí hasta 10 ticks (~2 s).
unsafe fn replay(act: Replay, tries: u8) {
    let dock = with(|d| {
        d.pending_click = None;
        d.hwnd
    });
    let _ = KillTimer(dock, T_CLICK);
    if tries < 10 && start_in_front() {
        with(|d| d.pending_click = Some((act, tries + 1)));
        SetTimer(dock, T_CLICK, FOCUS_DELAY_MS, None);
        return;
    }
    set_activatable(dock, false);
    let index = |key: &str| with(|d| d.items.iter().position(|i| i.key == key));
    let ctx = match act {
        Replay::Click(key) => {
            // Al cerrar Inicio el foco vuelve a la app de antes: clickearla tiene que traerla, no minimizarla.
            if let Some(i) = index(&key) { activate(i, tries > 0); }
            return;
        }
        Replay::Menu(key) => index(&key).and_then(|i| menu_ctx(i, false)),
        Replay::Overflow => with(|d| (d.layout.overflow > 0).then(|| d.layout.cells.len() - 1)).and_then(|first| overflow_ctx(first)),
    };
    if let Some(ctx) = ctx {
        hide_tip();
        crate::dock_menu::show(ctx);
    }
}

unsafe fn click(idx: usize) {
    if let Some(key) = with(|d| d.items.get(idx).map(|i| i.key.clone())) { replay(Replay::Click(key), 0); }
}

unsafe fn activate(idx: usize, after_start: bool) {
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
    if wins[0] == fg && !IsIconic(h).as_bool() && !after_start {
        let _ = ShowWindowAsync(h, SW_MINIMIZE);
    } else if let Some(t) = pick_target(&wins, fg, last) {
        focus_window(HWND(t as *mut _));
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) enum GroupId { Pin(String), App(String) }

fn group_id(item: &Item) -> GroupId {
    item.pin.as_ref().map(|id| GroupId::Pin(id.clone())).unwrap_or_else(|| GroupId::App(item.key.clone()))
}

fn accessible_group(item: &Item) -> String {
    match group_id(item) {
        GroupId::Pin(id) => format!("pin:{id}"),
        GroupId::App(id) => format!("app:{id}"),
    }
}

fn accessible_node(label: String, rect: R) -> accesskit::Node {
    let mut node = accesskit::Node::new(accesskit::Role::Button);
    node.set_label(label);
    node.set_bounds(accesskit::Rect::new(rect.x as f64, rect.y as f64, (rect.x + rect.w) as f64, (rect.y + rect.h) as f64));
    node.add_action(accesskit::Action::Click);
    node.add_action(accesskit::Action::Focus);
    node
}

fn publish_accessibility(hwnd: HWND, items: &[Item], layout: &Layout, titles: &HashMap<String, String>, focused: Option<KeyTarget>, visible: bool) {
    use crate::i18n::{t, K};
    let mut nodes = Vec::new();
    if visible {
        nodes.push(("start".into(), accessible_node(t(K::Start).into(), layout.brand)));
        for (i, cell) in layout.cells.iter().enumerate() {
            if layout.overflow > 0 && i + 1 == layout.cells.len() {
                nodes.push(("overflow".into(), accessible_node(format!("{} · {}", t(K::MoreApps), layout.overflow), *cell)));
            } else if let Some(item) = items.get(i) {
                let key = accessible_group(item);
                let name = titles.get(&key).cloned().unwrap_or_else(|| {
                    std::path::Path::new(&item.exe).file_stem().and_then(|s| s.to_str()).filter(|s| !s.is_empty()).unwrap_or(&item.key).to_string()
                });
                nodes.push((key, accessible_node(name, *cell)));
            }
        }
        if layout.tray.w > 0 { nodes.push(("tray".into(), accessible_node(t(K::SystemTray).into(), layout.tray))); }
    }
    let focus = focused.and_then(|target| match target {
        KeyTarget::Brand => Some("start".into()),
        KeyTarget::Tray => Some("tray".into()),
        KeyTarget::Overflow => Some("overflow".into()),
        KeyTarget::Item(id) => items.iter().find(|item| group_id(item) == id).map(accessible_group),
    });
    crate::accessibility::publish(hwnd, "Winarchy", (layout.bar.w, layout.bar.h), nodes, focus.as_deref());
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum KeyTarget { Brand, Item(GroupId), Overflow, Tray }

fn pointer_target(d: &Dock, hit: Option<Hit>) -> Option<KeyTarget> {
    match hit? {
        Hit::Brand => Some(KeyTarget::Brand),
        Hit::Tray => Some(KeyTarget::Tray),
        Hit::Overflow => Some(KeyTarget::Overflow),
        Hit::Item(index) => d.items.get(index).map(|item| KeyTarget::Item(group_id(item))),
    }
}

fn finish_pointer_gesture(down: &mut Option<KeyTarget>, up: Option<KeyTarget>, dragged: bool) -> Option<KeyTarget> {
    let pressed = down.take()?;
    (!dragged && Some(&pressed) == up.as_ref()).then_some(pressed)
}

unsafe fn reconcile_keyboard_target(d: &mut Dock) {
    let Some(KeyTarget::Item(id)) = d.key_target.as_ref() else { return };
    let index = resolve_overflow_group(&d.items, id, d.key_witness, |hwnd| {
        let mut pid = 0;
        GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid));
        pid
    });
    d.key_target = Some(index.map(|i| KeyTarget::Item(group_id(&d.items[i]))).unwrap_or(KeyTarget::Brand));
    remember_keyboard_target(d);
}

unsafe fn remember_keyboard_target(d: &mut Dock) {
    d.key_witness = match d.key_target.as_ref() {
        Some(KeyTarget::Item(id)) => d.items.iter().find(|i| &group_id(i) == id).and_then(|i| i.wins.first()).map(|hwnd| {
            let mut pid = 0;
            GetWindowThreadProcessId(HWND(*hwnd as *mut _), Some(&mut pid));
            (*hwnd, pid)
        }),
        _ => None,
    };
}

fn normalize_keyboard_target(target: &KeyTarget, items: &[Item], visible: usize, overflow: usize) -> KeyTarget {
    match target {
        KeyTarget::Item(id) if overflow > 0 && items.iter().position(|item| &group_id(item) == id).is_some_and(|i| i >= visible) => KeyTarget::Overflow,
        KeyTarget::Overflow if overflow == 0 => KeyTarget::Tray,
        _ => target.clone(),
    }
}

fn keyboard_targets(items: &[Item], visible: usize, overflow: usize) -> Vec<KeyTarget> {
    let mut targets = vec![KeyTarget::Brand];
    targets.extend(items.iter().take(visible).map(|item| KeyTarget::Item(group_id(item))));
    if overflow > 0 { targets.push(KeyTarget::Overflow); }
    targets.push(KeyTarget::Tray);
    targets
}

fn keyboard_step(targets: &[KeyTarget], current: &KeyTarget, direction: i32) -> KeyTarget {
    let Some(i) = targets.iter().position(|t| t == current) else { return KeyTarget::Brand };
    let next = if direction < 0 { (i + targets.len() - 1) % targets.len() } else { (i + 1) % targets.len() };
    targets[next].clone()
}

// Atajo del ancla N (1-based): lleva a esa app como un clic en su celda; la lanza si no corre.
pub fn launch_pin(n: usize) {
    unsafe {
        let mut at = POINT::default();
        let _ = GetCursorPos(&mut at);
        let owner = DOCKS.with(|v| {
            let v = v.borrow();
            let m = MonitorFromPoint(at, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let mon = GetMonitorInfoW(m, &mut info).as_bool().then_some((info.rcMonitor.left, info.rcMonitor.top, info.rcMonitor.right, info.rcMonitor.bottom));
            v.iter().find(|d| Some(d.mon) == mon).or(v.first()).map(|d| d.hwnd.0 as isize)
        });
        let Some(owner) = owner else { return };
        on_dock(owner, || {
            let idx = with(|d| d.items.iter().enumerate().filter(|(_, i)| i.pin.is_some()).nth(n.saturating_sub(1)).map(|(i, _)| i));
            if let Some(i) = idx {
                activate(i, false);
            }
        });
    }
}

pub fn focus_keyboard() {
    if KEY_SESSION.get().is_some() { end_keyboard(true); return; }
    unsafe {
        let previous = GetForegroundWindow();
        let monitor = if previous.0.is_null() || previous == GetShellWindow() {
            let mut point = POINT::default();
            let _ = GetCursorPos(&mut point);
            MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST)
        } else { MonitorFromWindow(previous, MONITOR_DEFAULTTONEAREST) };
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() { return }
        let owner = DOCKS.with(|d| d.borrow().iter().find(|d| d.mon == (info.rcMonitor.left, info.rcMonitor.top, info.rcMonitor.right, info.rcMonitor.bottom) && !d.hidden).map(|d| d.hwnd.0 as isize));
        let Some(owner) = owner else { return };
        let mut pid = 0;
        GetWindowThreadProcessId(previous, Some(&mut pid));
        KEY_SESSION.set(Some((owner, previous.0 as isize, pid)));
        on_dock(owner, || {
            with(|d| {
                let visible = d.layout.cells.len().saturating_sub(usize::from(d.layout.overflow > 0));
                d.key_target = Some(d.items.iter().position(|item| item.wins.contains(&(previous.0 as isize))).map(|i| {
                    if i < visible { KeyTarget::Item(group_id(&d.items[i])) } else { KeyTarget::Overflow }
                }).unwrap_or(KeyTarget::Brand));
                remember_keyboard_target(d);
            });
            render();
            let h = HWND(owner as *mut _);
            let _ = SetForegroundWindow(h);
            if GetForegroundWindow() != h { end_keyboard(false); }
        });
    }
}

fn focus_accessible(owner: isize, target: KeyTarget) {
    unsafe {
        let prior = KEY_SESSION.get();
        if prior.is_some_and(|(old, _, _)| old != owner) { end_keyboard(false); }
        let previous = prior.map(|(_, hwnd, pid)| (hwnd, pid)).unwrap_or_else(|| {
            let hwnd = GetForegroundWindow();
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            (hwnd.0 as isize, pid)
        });
        KEY_SESSION.set(Some((owner, previous.0, previous.1)));
        on_dock(owner, || {
            with(|d| { d.key_target = Some(target); remember_keyboard_target(d); });
            render();
            let hwnd = HWND(owner as *mut _);
            let _ = SetForegroundWindow(hwnd);
            if GetForegroundWindow() != hwnd { end_keyboard(false); }
        });
    }
}

pub(crate) fn end_keyboard(restore: bool) {
    let Some((owner, previous, expected_pid)) = KEY_SESSION.take() else { return };
    if restore && previous != 0 && expected_pid != 0 {
        unsafe {
            let h = HWND(previous as *mut _);
            let mut pid = 0;
            GetWindowThreadProcessId(h, Some(&mut pid));
            if pid == expected_pid && IsWindow(h).as_bool() && owns_keyboard_foreground(owner, crate::dock_menu::identity(), GetForegroundWindow().0 as isize) {
                let _ = SetForegroundWindow(h);
            }
        }
    }
    on_dock(owner, || { with(|d| { d.key_target = None; d.key_witness = None; }); render(); });
}

pub(crate) fn return_keyboard() {
    if let Some((owner, _, _)) = KEY_SESSION.get() {
        unsafe {
            let h = HWND(owner as *mut _);
            let _ = SetForegroundWindow(h);
            if GetForegroundWindow() != h { end_keyboard(false); }
        }
    }
}

unsafe fn keyboard_action(context: bool) {
    let target = with(|d| d.key_target.clone());
    match target {
        Some(KeyTarget::Brand) => { end_keyboard(false); if context { request_control_center(); } else { request_start(); } }
        Some(KeyTarget::Tray) if !context => { end_keyboard(false); open_tray(); }
        Some(KeyTarget::Item(id)) => {
            let index = with(|d| d.items.iter().position(|item| group_id(item) == id));
            if let Some(index) = index {
                let wins = with(|d| d.items[index].wins.clone());
                if context || wins.len() > 1 {
                    if let Some(ctx) = menu_ctx(index, !context) { crate::dock_menu::show(ctx); }
                } else {
                    end_keyboard(false);
                    if let Some(id) = wins.first() { focus_window(HWND(*id as *mut _)); }
                    else { with(|d| launch_of(&d.pins, &d.items[index])).iter().for_each(|(o, a)| crate::dock_menu::launch(o, a)); }
                }
            }
        }
        Some(KeyTarget::Overflow) if !context => {
            let visible = with(|d| (d.layout.overflow > 0).then(|| d.layout.cells.len().saturating_sub(1)));
            if let Some(ctx) = visible.and_then(|visible| overflow_ctx(visible)) { crate::dock_menu::show(ctx); }
        }
        _ => {}
    }
}

fn overflow_groups(items: &[Item], from: usize) -> Vec<(GroupId, Option<isize>, String, usize)> {
    items.iter().skip(from).map(|item| {
        let name = [&item.exe, item.pin.as_deref().unwrap_or("")].into_iter()
            .filter_map(|p| std::path::Path::new(p).file_stem().and_then(|s| s.to_str()).filter(|s| !s.is_empty()))
            .next().unwrap_or(&item.key).to_string();
        (group_id(item), item.wins.first().copied(), name, item.wins.len())
    }).collect()
}

#[derive(Clone, Copy)]
pub(crate) struct PopupAnchor {
    pub owner: isize,
    pub cx: i32,
    pub bar: R,
    pub bottom: bool,
    pub mon: MonRect,
    pub scale: f32,
}

// Qué lanzaría soltar archivos en el punto de pantalla (sx, sy) de este dock; None si no hay celda o la app no los acepta.
pub(crate) fn drop_launch(owner: isize, sx: i32, sy: i32) -> Option<(String, String)> {
    let mut out = None;
    on_dock(owner, || unsafe {
        let mut p = POINT { x: sx, y: sy };
        let _ = ScreenToClient(HWND(owner as *mut _), &mut p);
        out = with(|d| match display_hit(d, p.x, p.y) {
            Some(Hit::Item(i)) => d.items.get(i).and_then(|it| launch_of(&d.pins, it)),
            _ => None,
        })
        .filter(|(o, a)| crate::dock_drop::droppable(o, a));
    });
    out
}

pub(crate) fn on_dock(owner: isize, f: impl FnOnce()) {
    let index = DOCKS.with(|d| d.borrow().iter().position(|d| d.hwnd.0 as isize == owner));
    if let Some(index) = index {
        let previous = CUR.replace(index);
        f();
        CUR.set(previous);
    }
}

fn resolve_overflow_group(items: &[Item], id: &GroupId, witness: Option<(isize, u32)>, current_pid: impl Fn(isize) -> u32) -> Option<usize> {
    items.iter().position(|item| &group_id(item) == id).or_else(|| {
        let (hwnd, pid) = witness?;
        if pid == 0 || current_pid(hwnd) != pid { return None; }
        items.iter().position(|item| item.wins.contains(&hwnd))
    })
}

pub(crate) fn select_overflow_group(owner: isize, id: &GroupId, witness: Option<(isize, u32)>, anchor: PopupAnchor) {
    on_dock(owner, || unsafe {
        refresh();
        let selected = with(|d| resolve_overflow_group(&d.items, id, witness, |hwnd| {
            let mut pid = 0;
            GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid));
            pid
        }).map(|i| (i, d.items[i].wins.clone(), launch_of(&d.pins, &d.items[i]))));
        let Some((index, wins, launch)) = selected else { return };
        match wins.len() {
            0 => launch.iter().for_each(|(o, a)| crate::dock_menu::launch(o, a)),
            1 => focus_window(HWND(wins[0] as *mut _)),
            _ => if let Some(ctx) = menu_ctx_at(index, true, Some(anchor)) { crate::dock_menu::show(ctx); },
        }
    });
}

unsafe fn overflow_ctx(first_hidden: usize) -> Option<crate::dock_menu::Ctx> {
    let (mut ctx, groups) = with(|d| {
        let (xs, bar, _) = glide_view(d);
        let shown = display_layout(&d.layout, &xs, bar);
        let cell = *shown.cells.last()?;
        let groups = overflow_groups(&d.items, first_hidden);
        Some((crate::dock_menu::Ctx {
            owner: d.hwnd.0 as isize,
            groups: Vec::new(),
            picker: true,
            titles: Vec::new(),
            active: None,
            theme: d.theme.clone(),
            scale: d.scale,
            cx: bar.x + cell.x + cell.w / 2,
            bar,
            bottom: d.theme.position != "bottom",
            mon: d.mon,
            wins: Vec::new(),
            closeable_wins: Vec::new(),
            last: None,
            key: String::new(),
            pinned: false,
            pinnable: false,
            launch: None,
            closable: false,
            recents: Vec::new(),
        }, groups))
    })?;
    ctx.groups = groups.into_iter().map(|(id, first, fallback, count)| {
        let title = first.and_then(|w| window_titles(&[w]).into_iter().next()).filter(|t| !t.is_empty()).unwrap_or(fallback);
        let witness = first.map(|hwnd| {
            let mut pid = 0;
            GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid));
            (hwnd, pid)
        });
        (id, if count > 1 { format!("{title} · {count}") } else { title }, witness)
    }).collect();
    Some(ctx)
}

unsafe fn window_titles(wins: &[isize]) -> Vec<String> {
    wins.iter()
        .map(|w| {
            let h = HWND(*w as *mut _);
            let len = GetWindowTextLengthW(h).clamp(0, 32767) as usize;
            let mut buf = vec![0u16; len + 1];
            let n = GetWindowTextW(h, &mut buf).max(0) as usize;
            String::from_utf16_lossy(&buf[..n])
        })
        .collect()
}

// Datos del menú de una celda. Los títulos se leen fuera del préstamo del estado.
unsafe fn menu_ctx(idx: usize, picker: bool) -> Option<crate::dock_menu::Ctx> {
    menu_ctx_at(idx, picker, None)
}

unsafe fn menu_ctx_at(idx: usize, picker: bool, anchor: Option<PopupAnchor>) -> Option<crate::dock_menu::Ctx> {
    let (mut ctx, aumid) = with(|d| {
        let it = d.items.get(idx)?;
        let cell = *d.layout.cells.get(idx).or_else(|| d.layout.cells.last())?;
        let anchor = anchor.unwrap_or(PopupAnchor { owner: d.hwnd.0 as isize, cx: d.layout.bar.x + cell.x + cell.w / 2, bar: d.layout.bar, bottom: d.theme.position != "bottom", mon: d.mon, scale: d.scale });
        Some((crate::dock_menu::Ctx {
            owner: anchor.owner,
            groups: Vec::new(),
            picker,
            titles: Vec::new(),
            active: None,
            theme: d.theme.clone(),
            scale: anchor.scale,
            cx: anchor.cx,
            bar: anchor.bar,
            bottom: anchor.bottom,
            mon: anchor.mon,
            wins: it.wins.clone(),
            closeable_wins: Vec::new(),
            last: d.last_active.get(&it.key).copied(),
            key: it.key.clone(),
            pinned: it.pin.is_some(),
            pinnable: pin_for(it, |p| p.exists()).is_some(),
            launch: launch_of(&d.pins, it),
            closable: true,
            recents: Vec::new(),
        }, it.aumid.clone()))
    })?;
    ctx.closeable_wins = ctx.wins.iter().copied().filter(|w| IsWindow(HWND(*w as *mut _)).as_bool() && !is_elevated(HWND(*w as *mut _))).collect();
    ctx.closable = !ctx.closeable_wins.is_empty();
    ctx.active = Some(GetForegroundWindow().0 as isize);
    ctx.titles = window_titles(&ctx.wins);
    if !picker {
        ctx.recents = aumid.map(|a| crate::dock_recent::list(&a)).unwrap_or_default();
    }
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
    let (wins, last, pin) = with(|d| d.items.get(idx).map(|i| (i.wins.clone(), previewed(d, i), launch_of(&d.pins, i).map(|(o, _)| o))))?;
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
    let tip = with(|d| {
        d.tip_born = None;
        d.tip
    });
    let _ = ShowWindow(tip, SW_HIDE);
    crate::dock_thumb::hide();
}

// Ventana que enfocaría el clic en la celda, para su miniatura; ni el hub, ni la bandeja ni el desborde la tienen.
// Ventana que se previsualiza: la elegida con la rueda mientras siga abierta, si no la última activa.
fn previewed(d: &Dock, item: &Item) -> Option<isize> {
    d.preview_pick.as_ref().filter(|(k, w)| *k == item.key && item.wins.contains(w)).map(|(_, w)| *w).or_else(|| d.last_active.get(&item.key).copied())
}

unsafe fn thumb_target(idx: usize) -> Option<isize> {
    if idx == HUB_INDEX || idx == TRAY_INDEX || with(|d| d.layout.overflow > 0 && idx + 1 == d.layout.cells.len()) {
        return None;
    }
    let (wins, last) = with(|d| d.items.get(idx).map(|i| (i.wins.clone(), previewed(d, i))))?;
    pick_target(&wins, 0, last)
}

// Tooltip propio (ventana layered chica): el texto se dibuja con GDI sobre negro y su luminancia pasa a alfa.
unsafe fn show_tip(idx: usize) {
    if crate::dock_menu::is_open() {
        return;
    }
    let text = if idx == HUB_INDEX { Some(crate::i18n::t(crate::i18n::K::StartMenu).to_string()) } else if idx == TRAY_INDEX { Some(crate::i18n::t(crate::i18n::K::SystemTray).to_string()) } else { tip_text(idx) };
    let Some(text) = text else { return hide_tip() };
    let (tip, theme, scale, bar, cell, bottom) = with(|d| (d.tip, d.theme.clone(), d.scale, d.layout.bar, if idx == HUB_INDEX { Some(d.layout.brand) } else if idx == TRAY_INDEX { Some(d.layout.tray) } else { d.layout.cells.get(idx).copied() }, d.theme.position != "bottom"));
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
    let animate = with(|d| d.motion);
    let bf = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: if animate { 0 } else { 255 }, AlphaFormat: 1 };
    let _ = UpdateLayeredWindow(tip, None, Some(&pos), Some(&sz), mem, Some(&src), COLORREF(0), Some(&bf), ULW_ALPHA);
    SelectObject(mem, old);
    SelectObject(mem, old_font);
    let _ = DeleteObject(dib);
    let _ = DeleteObject(font);
    let _ = DeleteDC(mem);
    ReleaseDC(None, screen);
    let _ = ShowWindow(tip, SW_SHOWNOACTIVATE);
    match thumb_target(idx) {
        Some(target) => crate::dock_thumb::show(target, bar.x + cell.x + cell.w / 2, (x, y, w, h), bottom, mon, scale, &theme),
        None => crate::dock_thumb::hide(),
    }
    if animate {
        let dock = with(|d| {
            d.tip_born = Some(std::time::Instant::now());
            d.hwnd
        });
        SetTimer(dock, T_TIP, ANIM_MS, None);
    }
}

// Fundido del tooltip: solo cambia el alfa constante; el contenido ya está en la ventana.
unsafe fn tip_tick(h: HWND) {
    let (tip, born) = with(|d| (d.tip, d.tip_born));
    let Some(born) = born else {
        let _ = KillTimer(h, T_TIP);
        return;
    };
    let t = (born.elapsed().as_millis() as f32 / TIP_FADE_MS as f32).min(1.0);
    let bf = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: (ease(t) * 255.0).round() as u8, AlphaFormat: 1 };
    let _ = UpdateLayeredWindow(tip, None, None, None, None, None, COLORREF(0), Some(&bf), ULW_ALPHA);
    if t >= 1.0 {
        let _ = KillTimer(h, T_TIP);
        with(|d| d.tip_born = None);
    }
}

// Los mensajes llegan anidados (ShowWindow envía a su propia ventana): `CUR` se devuelve a lo que era al salir.
unsafe extern "system" fn dock_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if m == WM_GETOBJECT { if let Some(result) = crate::accessibility::get_object(h, wp, lp) { return result; } }
    if m == WM_NCDESTROY { crate::accessibility::destroy(h); }
    let keep = CUR.get();
    let r = dock_msg(h, m, wp, lp);
    CUR.set(keep);
    r
}

unsafe fn dock_msg(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(i) = DOCKS.with(|v| v.try_borrow().ok().and_then(|v| v.iter().position(|d| d.hwnd == h))) else { return DefWindowProcW(h, m, wp, lp) };
    CUR.set(i);
    let (shell_msg, created_msg) = with(|d| (d.shell_msg, d.created_msg));
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
        crate::accessibility::WM_ACTION => {
            let actions = crate::accessibility::actions(h, wp.0);
            for (key, action) in actions {
                if action == accesskit::Action::Focus {
                    let target = match key.as_str() {
                        "start" => Some(KeyTarget::Brand),
                        "tray" => Some(KeyTarget::Tray),
                        "overflow" => Some(KeyTarget::Overflow),
                        _ => with(|d| d.items.iter().find(|item| accessible_group(item) == key).map(|item| KeyTarget::Item(group_id(item)))),
                    };
                    if let Some(target) = target { focus_accessible(h.0 as isize, target); }
                    continue;
                }
                if action != accesskit::Action::Click { continue }
                match key.as_str() {
                    "start" => request_start(),
                    "tray" => open_tray(),
                    "overflow" => {
                        let first = with(|d| (d.layout.overflow > 0).then(|| d.layout.cells.len().saturating_sub(1)));
                        if let Some(ctx) = first.and_then(|first| overflow_ctx(first)) { crate::dock_menu::show(ctx); }
                    }
                    _ => {
                        let index = with(|d| d.items.iter().position(|item| accessible_group(item) == key));
                        if let Some(index) = index { click(index); }
                    }
                }
            }
            LRESULT(0)
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            crate::accessibility::focus(h, m == WM_SETFOCUS);
            DefWindowProcW(h, m, wp, lp)
        }
        WM_TIMER if wp.0 == T_REFRESH => {
            let _ = KillTimer(h, T_REFRESH);
            refresh();
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_ANIM => {
            anim_tick(h);
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_TIP => {
            tip_tick(h);
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_EDGE => {
            edge_tick(h);
            LRESULT(0)
        }
        WM_TRAY_DISMISS => {
            if TRAY_SESSION.get().is_some_and(|(_, generation)| generation == wp.0) && !tray_surface(HWND(lp.0 as *mut _)) { dismiss_tray(); }
            LRESULT(0)
        }
        WM_TRAY_FOREGROUND => {
            if TRAY_SESSION.get().is_some_and(|(_, generation)| generation == wp.0) {
                let mut cls = [0u16; 128];
                let n = GetClassNameW(HWND(lp.0 as *mut _), &mut cls).max(0) as usize;
                if matches!(String::from_utf16_lossy(&cls[..n]).as_str(), "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") { TRAY_FOCUS_SEEN.set(true); }
                if TRAY_FOCUS_SEEN.get() || TRAY_PENDING.get() || TRAY_FALLBACK.get() { SetTimer(h, T_TRAY_FOCUS, 200, None); }
            }
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_TRAY_ACTIVATE => {
            activate_tray(h);
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_TRAY_FOCUS => {
            let (same_focus, tray_window, menu, taskbar) = tray_foreground();
            if taskbar { TRAY_FOCUS_SEEN.set(true); }
            if !TRAY_FOCUS_SEEN.get() && TRAY_FOCUS_STARTED.get().is_some_and(|t| t.elapsed().as_secs() >= 1) {
                TRAY_FOCUS_STARTED.set(None);
                TRAY_FALLBACK.set(true);
            }
            let waiting = TRAY_PENDING.get() || TRAY_FALLBACK.get();
            if apps::tray_focus_dismisses(TRAY_FOCUS_SEEN.get(), tray_window, menu) || apps::tray_wait_cancel(waiting, same_focus, tray_window, menu) { dismiss_tray(); }
            else if !menu && (TRAY_FOCUS_SEEN.get() || waiting) { let _ = KillTimer(h, T_TRAY_FOCUS); }
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_TICK => {
            tick();
            refresh_window_icons();
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_CLICK => {
            let _ = KillTimer(h, T_CLICK);
            if let Some((act, tries)) = with(|d| d.pending_click.take()) {
                replay(act, tries);
            }
            LRESULT(0)
        }
        WM_ICON => {
            drain_ready();
            LRESULT(0)
        }
        x if x == komorebi::WM_WINDOW_MAP => {
            let all: Vec<HWND> = DOCKS.with(|v| v.borrow().iter().map(|d| d.hwnd).collect());
            all.into_iter().for_each(|d| queue_refresh(d));
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
        WM_KEYDOWN if KEY_SESSION.get().is_some_and(|(owner, _, _)| owner == h.0 as isize) => {
            let key = wp.0 as u16;
            if key == VK_ESCAPE.0 {
                end_keyboard(true);
            } else if [VK_LEFT.0, VK_RIGHT.0, VK_TAB.0, VK_HOME.0, VK_END.0].contains(&key) {
                with(|d| {
                    let visible = d.layout.cells.len().saturating_sub(usize::from(d.layout.overflow > 0));
                    let targets = keyboard_targets(&d.items, visible, d.layout.overflow);
                    let current = d.key_target.as_ref().unwrap_or(&KeyTarget::Brand);
                    d.key_target = Some(if key == VK_HOME.0 { KeyTarget::Brand } else if key == VK_END.0 { targets.last().cloned().unwrap_or(KeyTarget::Brand) } else {
                        let backwards = key == VK_LEFT.0 || (key == VK_TAB.0 && GetAsyncKeyState(VK_SHIFT.0 as i32) < 0);
                        keyboard_step(&targets, current, if backwards { -1 } else { 1 })
                    });
                    remember_keyboard_target(d);
                });
                render();
            } else if key == VK_RETURN.0 || key == VK_SPACE.0 {
                keyboard_action(false);
            } else if key == VK_APPS.0 || (key == VK_F10.0 && GetAsyncKeyState(VK_SHIFT.0 as i32) < 0) {
                keyboard_action(true);
            }
            LRESULT(0)
        }
        WM_ACTIVATE if wp.0 & 0xffff == 0 && KEY_SESSION.get().is_some_and(|(owner, _, _)| owner == h.0 as isize) && !crate::dock_menu::is_open() => {
            end_keyboard(false);
            LRESULT(0)
        }
        // Inicio rechaza el input sintético y solo se cierra cuando otra ventana se activa, como con la taskbar nativa:
        // con Inicio al frente el dock deja de ser NOACTIVATE (WM_MOUSEMOVE, antes del clic) y el clic lo activa.
        WM_MOUSEACTIVATE if start_in_front() => LRESULT(MA_ACTIVATE as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_MOUSEMOVE => {
            let (x, y) = xy(lp);
            // El logo y la bandeja con Inicio abierto actúan sobre Inicio mismo: activar el dock los rompería.
            set_activatable(h, start_in_front() && matches!(with(|d| display_hit(d, x, y)), Some(Hit::Item(_) | Hit::Overflow)));
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
                let idx = match display_hit(d, x, y) {
                    Some(Hit::Brand) => Some(HUB_INDEX),
                    Some(Hit::Tray) => Some(TRAY_INDEX),
                    Some(Hit::Item(i)) => Some(i),
                    Some(Hit::Overflow) => d.layout.cells.len().checked_sub(1),
                    None => None,
                };
                let changed = idx != d.hover;
                if changed {
                    d.preview_pick = None;
                }
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
        WM_MOUSEWHEEL => {
            let delta = ((wp.0 >> 16) & 0xFFFF) as i16 as i32;
            let next = with(|d| {
                let item = d.items.get(d.hover?)?;
                let next = step_preview(&item.wins, previewed(d, item), delta)?;
                d.preview_pick = Some((item.key.clone(), next));
                d.hover
            });
            if let Some(idx) = next {
                show_tip(idx);
            }
            LRESULT(0)
        }
        0x02A3 => {
            with(|d| {
                d.preview_pick = None;
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
            match with(|d| display_hit(d, x, y)) {
                Some(Hit::Brand) => { hide_tip(); request_control_center(); }
                Some(Hit::Item(i)) => if let Some(key) = with(|d| d.items.get(i).map(|it| it.key.clone())) { replay(Replay::Menu(key), 0) },
                _ => {}
            }
            LRESULT(0)
        }
        WM_MBUTTONUP => {
            let (x, y) = xy(lp);
            let target = with(|d| match display_hit(d, x, y) {
                Some(Hit::Item(i)) => d.items.get(i).and_then(|it| launch_of(&d.pins, it)),
                _ => None,
            });
            target.iter().for_each(|(o, a)| crate::dock_menu::launch(o, a));
            LRESULT(0)
        }
        WM_CAPTURECHANGED | WM_CANCELMODE => {
            let changed = with(|d| {
                d.pointer_down = None;
                d.drag = None;
                d.pressed.take().is_some()
            });
            if m == WM_CANCELMODE && GetCapture() == h {
                let _ = ReleaseCapture();
            }
            if changed {
                with(|d| d.sig = 0);
                render();
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = xy(lp);
            let hit = with(|d| {
                let hit = display_hit(d, x, y);
                d.pointer_down = pointer_target(d, hit);
                hit
            });
            if let Some(Hit::Item(i)) = hit {
                with(|d| d.drag = d.items.get(i).map(|it| Drag { key: it.key.clone(), x0: x, moving: false }));
                SetCapture(h);
            } else if matches!(hit, Some(Hit::Brand | Hit::Tray | Hit::Overflow)) {
                SetCapture(h);
            }
            let pressed = match hit { Some(Hit::Brand) => Some(HUB_INDEX), Some(Hit::Tray) => Some(TRAY_INDEX), Some(Hit::Item(i)) if with(|d| d.motion) => Some(i), _ => None };
            if pressed.is_some() && with(|d| d.motion) {
                with(|d| d.pressed = pressed);
                render();
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = xy(lp);
            let (hit, activate) = with(|d| {
                let moved = d.drag.take().is_some_and(|dr| dr.moving);
                let hit = display_hit(d, x, y);
                let up = pointer_target(d, hit);
                (hit, finish_pointer_gesture(&mut d.pointer_down, up, moved).is_some())
            });
            let _ = ReleaseCapture();
            if with(|d| d.pressed.take()).is_some() {
                with(|d| d.sig = 0);
                render();
            }
            if !activate { return LRESULT(0); }
            match hit {
                Some(Hit::Brand) => request_start(),
                Some(Hit::Tray) => { hide_tip(); open_tray(); },
                Some(Hit::Item(i)) => click(i),
                Some(Hit::Overflow) => replay(Replay::Overflow, 0),
                None => {}
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

#[cfg(test)]
#[path = "dock_overflow_tests.rs"]
mod overflow_regression_tests;

#[cfg(test)]
#[path = "dock_keyboard_tests.rs"]
mod keyboard_tests;

#[cfg(test)]
#[path = "dock_explorer_tests.rs"]
mod explorer_tests;

#[cfg(test)]
#[path = "dock_monitor_tests.rs"]
mod monitor_tests;

#[cfg(test)]
#[path = "dock_attention_tests.rs"]
mod attention_tests;

#[cfg(test)]
#[path = "dock_pointer_tests.rs"]
mod pointer_tests;

#[cfg(test)]
mod tests {
    use super::*;

    const MON: MonRect = (0, 0, 1920, 1080);

    #[test]
    fn brand_mark_uses_foreground_for_three_strokes_and_accent_for_the_fourth() {
        assert_eq!(brand_mark_colors((12, 34, 56), (78, 90, 123)), [(12, 34, 56), (12, 34, 56), (12, 34, 56), (78, 90, 123)]);
    }

    #[test]
    fn brand_mark_fits_the_cell_and_keeps_the_svg_aspect_ratio() {
        for (cell, scale) in [(R { x: 10, y: 20, w: 40, h: 40 }, 1.0), (R { x: 15, y: 30, w: 60, h: 60 }, 1.5)] {
            let polygons = brand_mark_polygons(cell, scale);
            let points: Vec<_> = polygons.iter().flatten().copied().collect();
            let left = points.iter().map(|p| p.0).min().unwrap();
            let right = points.iter().map(|p| p.0).max().unwrap();
            let top = points.iter().map(|p| p.1).min().unwrap();
            let bottom = points.iter().map(|p| p.1).max().unwrap();
            assert!(left >= cell.x && right <= cell.x + cell.w && top >= cell.y && bottom <= cell.y + cell.h);
            assert!((left + right - (2 * cell.x + cell.w)).abs() <= 1);
            assert!((top + bottom - (2 * cell.y + cell.h)).abs() <= 1);
            assert!(right > left && bottom > top);
            assert!(((right - left) * 56 - (bottom - top) * 104).abs() <= 104);
        }
    }

    #[test]
    fn brand_mark_keeps_the_four_svg_strokes_in_paint_order() {
        let polygons = brand_mark_polygons(R { x: 0, y: 0, w: 40, h: 40 }, 1.0);
        assert_eq!(polygons.map(|p| p[0].1 < p[2].1), [true, false, true, false]);
        let centers = polygons.map(|p| p.iter().map(|v| v.0).sum::<i32>());
        assert!(centers.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(polygons.map(|p| p[0].0 < p[1].0 && p[3].0 < p[2].0), [true; 4]);
    }

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
    fn the_wheel_cycles_the_previewed_window_and_wraps() {
        let wins = [10, 20, 30];
        assert_eq!(step_preview(&wins, Some(10), -120), Some(20));
        assert_eq!(step_preview(&wins, Some(30), -120), Some(10));
        assert_eq!(step_preview(&wins, Some(10), 120), Some(30));
        assert_eq!(step_preview(&wins, None, -120), Some(20));
        assert_eq!(step_preview(&[10], Some(10), -120), None);
        assert_eq!(step_preview(&wins, Some(10), 0), None);
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
    fn overflow_groups_keep_hidden_groups_in_order_and_use_their_first_window() {
        let visible = item("exe:visible", r"C:\Apps\Visible.exe");
        let mut editor = item("exe:editor", r"C:\Apps\Editor.exe");
        editor.wins = vec![21, 22, 23];
        let mut browser = item("exe:browser", r"C:\Apps\Browser.exe");
        browser.wins = vec![31, 32];
        assert_eq!(overflow_groups(&[visible, editor, browser], 1), vec![
            (GroupId::App("exe:editor".into()), Some(21), "Editor".into(), 3),
            (GroupId::App("exe:browser".into()), Some(31), "Browser".into(), 2),
        ]);
    }

    #[test]
    fn overflow_groups_include_pins_without_windows_and_fall_back_to_their_id() {
        let mut pin = item("pin:calculator", "");
        pin.wins.clear();
        pin.pin = Some(r"C:\Shortcuts\Calculator.lnk".into());
        assert_eq!(overflow_groups(&[pin], 0), vec![
            (GroupId::Pin(r"C:\Shortcuts\Calculator.lnk".into()), None, "Calculator".into(), 0),
        ]);
    }

    #[test]
    fn overflow_groups_keep_distinct_keys_for_apps_with_the_same_name() {
        let first = item("exe:first", r"C:\First\Tool.exe");
        let second = item("exe:second", r"C:\Second\Tool.exe");
        assert_eq!(overflow_groups(&[first, second], 0), vec![
            (GroupId::App("exe:first".into()), Some(1), "Tool".into(), 1),
            (GroupId::App("exe:second".into()), Some(1), "Tool".into(), 1),
        ]);
    }

    #[test]
    fn overflow_groups_use_the_key_when_no_executable_or_pin_is_available() {
        assert_eq!(overflow_groups(&[item("aumid:calculator", "")], 0), vec![
            (GroupId::App("aumid:calculator".into()), Some(1), "aumid:calculator".into(), 1),
        ]);
    }

    #[test]
    fn overflow_groups_are_empty_at_or_beyond_the_end() {
        let items = [item("exe:editor", "Editor.exe")];
        assert_eq!(overflow_groups(&items, 1), vec![]);
        assert_eq!(overflow_groups(&items, usize::MAX), vec![]);
        assert_eq!(overflow_groups(&[], 0), vec![]);
    }

    #[test]
    fn group_id_keeps_a_pin_identity_when_its_window_key_changes() {
        let mut closed = item("pin:calculator", "");
        closed.wins.clear();
        closed.pin = Some(r"C:\Shortcuts\Calculator.lnk".into());
        let mut running = item("exe:calculator", r"C:\Apps\Calculator.exe");
        running.pin = Some(r"C:\Shortcuts\Calculator.lnk".into());
        assert_eq!([group_id(&closed), group_id(&running)], [
            GroupId::Pin(r"C:\Shortcuts\Calculator.lnk".into()),
            GroupId::Pin(r"C:\Shortcuts\Calculator.lnk".into()),
        ]);
    }

    #[test]
    fn group_id_keeps_apps_with_the_same_executable_name_distinct() {
        let first = item("exe:first", r"C:\First\Tool.exe");
        let second = item("exe:second", r"C:\Second\Tool.exe");
        assert_eq!([group_id(&first), group_id(&second)], [
            GroupId::App("exe:first".into()),
            GroupId::App("exe:second".into()),
        ]);
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
    fn the_dock_that_hosts_the_cluster_has_no_tray_button_of_its_own() {
        for n in [0, 3, 200] {
            let l = layout_for(n, 1.0, MON, true, 4, true);
            assert_eq!(l.tray.w, 0);
            assert_eq!(hit_test(&l, l.bar.x + l.bar.w - 2, l.bar.y + l.bar.h / 2), None);
            assert!(l.bar.x >= MON.0 && l.bar.x + l.bar.w <= MON.2);
        }
    }

    #[test]
    fn the_tray_button_is_clickable_with_no_apps_and_with_overflow() {
        for n in [0, 3, 200] {
            let l = layout(n, 1.0, MON, true, 4);
            assert_eq!(hit_test(&l, l.bar.x + l.tray.x + l.tray.w / 2, l.bar.y + l.tray.y + l.tray.h / 2), Some(Hit::Tray));
        }
    }

    #[test]
    fn the_tray_button_has_its_own_space_inside_the_monitor() {
        for (mon, scale) in [(MON, 1.0), ((-1280, -200, 0, 824), 1.5)] {
            for n in [0, 3, 200] {
                for bottom in [false, true] {
                    let l = layout(n, scale, mon, bottom, 4);
                    assert!(l.tray.w > 0 && l.tray.h > 0);
                    assert!(l.tray.x >= 0 && l.tray.y >= 0 && l.tray.x + l.tray.w <= l.bar.w && l.tray.y + l.tray.h <= l.bar.h);
                    for c in std::iter::once(&l.brand).chain(l.cells.iter()) {
                        assert!(c.x + c.w <= l.tray.x || l.tray.x + l.tray.w <= c.x || c.y + c.h <= l.tray.y || l.tray.y + l.tray.h <= c.y);
                    }
                    assert!(l.bar.x >= mon.0 && l.bar.y >= mon.1 && l.bar.x + l.bar.w <= mon.2 && l.bar.y + l.bar.h <= mon.3);
                }
            }
        }
    }

    #[test]
    fn hit_testing_follows_cells_while_the_bar_glides() {
        let old = layout(3, 1.0, MON, true, 4);
        let target = layout(2, 1.0, MON, true, 4);
        let xs: Vec<i32> = old.cells[1..].iter().map(|c| c.x).collect();
        let shown = display_layout(&target, &xs, old.bar);
        let b = old.cells[1];
        let c = old.cells[2];
        assert_eq!(hit_test(&shown, old.bar.x + b.x + 5, old.bar.y + b.y + 5), Some(Hit::Item(0)));
        assert_eq!(hit_test(&shown, old.bar.x + c.x + 5, old.bar.y + c.y + 5), Some(Hit::Item(1)));
    }

    #[test]
    fn an_app_overlapping_the_tray_during_expansion_receives_the_hit() {
        let old = layout(2, 1.0, MON, true, 4);
        let target = layout(3, 1.0, MON, true, 4);
        let xs = [old.cells[0].x, old.cells[1].x, target.bar.x + target.cells[2].x - old.bar.x];
        let shown = display_layout(&target, &xs, old.bar);
        let y = shown.bar.y + shown.tray.y + shown.tray.h / 2;
        assert_eq!(hit_test(&shown, shown.bar.x + shown.tray.x + 1, y), Some(Hit::Item(2)));
        assert_eq!(hit_test(&shown, shown.bar.x + shown.tray.x + shown.tray.w - 1, y), Some(Hit::Tray));
    }

    #[test]
    fn the_reveal_edge_is_two_pixels_on_the_dock_side_of_its_own_monitor() {
        let bar = R { x: 800, y: 1026, w: 300, h: 48 };
        assert_eq!(reveal_zones(MON, bar, true, (900, 1079)), (true, true));
        assert_eq!(reveal_zones(MON, bar, true, (900, 900)), (false, false));
        assert_eq!(reveal_zones(MON, bar, true, (2000, 1079)), (false, false));
        assert_eq!(reveal_zones(MON, bar, true, (760, 1079)), (true, false));
        assert_eq!(reveal_zones(MON, bar, true, (900, 1030)), (false, true));
        assert_eq!(reveal_zones(MON, bar, false, (760, 0)), (true, false));
    }

    #[test]
    fn brand_button_remains_hit_testable_without_windows() {
        let l = layout(0, 1.5, MON, true, 4);
        assert!(l.cells.is_empty());
        assert_eq!(hit_test(&l, l.bar.x + l.brand.x + 1, l.bar.y + l.brand.y + 1), Some(Hit::Brand));
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
