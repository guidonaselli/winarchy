// Cluster de sistema del dock (dirección D1): una píldora aparte en el borde derecho, con el estado del
// sistema y el reloj. Reemplaza lo que la taskbar de Windows muestra a la derecha. No hace nada por su
// cuenta: abre Quick Settings y las notificaciones pidiéndole a AHK el atajo nativo, y ajusta el volumen.
use crate::dock::{approach, draw_label, ease, fill_round_a, style, BAR_DIP, HOVER_MS, MARGIN_DIP};
use crate::i18n::{lang, t, Lang, K};
use crate::layout::{px, R};
use crate::system::{self, Snapshot};
use crate::theme::{rgb, Theme};
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::UI::Controls::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;

const ICONS: &str = "Segoe MDL2 Assets";
const GLYPH_CHEVRON: &str = "\u{E70E}";
const GLYPH_NET: &str = "\u{E701}";
const GLYPH_VOLUME: &str = "\u{E767}";
const GLYPH_MUTE: &str = "\u{E74F}";
const GLYPH_BATTERY: &str = "\u{E83F}";
const GLYPH_CHARGING: &str = "\u{E83E}";

const GLYPH_DIP: i32 = 20;
const SIDE_DIP: i32 = 12;
const CLOCK_DIP: i32 = 84;
const BADGE_DIP: i32 = 20;
const BATTERY_DIP: i32 = 38;
const TRAY_CELL_DIP: i32 = 28;
const TRAY_ICON_DIP: i32 = 18;
const DESKTOP_DIP: i32 = 10;
const PART_INSET_DIP: i32 = 6;
const GAP_DIP: i32 = 4;
const PAD_DIP: i32 = 4;
const ANIM_MS: u32 = 16;
const SECOND_MS: u32 = 1000;
// Sin manifest comctl32 es v5 y rechaza el TTTOOLINFOW completo: hasta lpszText (offset de lParam).
const TOOLINFO_V1_SIZE: u32 = 56;
const T_SECOND: usize = 1;
const T_ANIM: usize = 2;
const MOUSE_LEAVE: u32 = 0x02A3;
pub const WM_SYNC: u32 = WM_APP + 31;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Part {
    Status,
    Clock,
    Desktop,
    Chevron,
    Tray(usize),
}

impl Part {
    fn index(self) -> usize {
        match self {
            Part::Status => 0,
            Part::Clock => 1,
            Part::Desktop => 2,
            Part::Chevron => 3,
            Part::Tray(i) => 4 + i,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Shape {
    pub chevron: R,
    pub tray: Vec<R>,
    pub status: R,
    pub clock: R,
    pub desktop: R,
    pub width: i32,
}

pub fn shape(scale: f32, battery: bool, badge: bool, pinned: usize, height: i32) -> Shape {
    let (side, glyph, gap, inset) = (px(SIDE_DIP, scale), px(GLYPH_DIP, scale), px(GAP_DIP, scale), px(PART_INSET_DIP, scale));
    let battery_w = if battery { px(BATTERY_DIP, scale) + gap } else { 0 };
    let status_w = 2 * side + 2 * glyph + gap + battery_w;
    let clock_w = px(CLOCK_DIP, scale) + if badge { px(BADGE_DIP, scale) + gap } else { 0 };
    let pad = px(PAD_DIP, scale);
    let cell = px(TRAY_CELL_DIP, scale);
    let chevron = R { x: pad, y: inset, w: cell, h: height - 2 * inset };
    let tray: Vec<R> = (0..pinned as i32).map(|i| R { x: pad + (i + 1) * cell, y: inset, w: cell, h: height - 2 * inset }).collect();
    let start = pad + (pinned as i32 + 1) * cell + 2 * gap;
    let status = R { x: start, y: inset, w: status_w, h: height - 2 * inset };
    let clock = R { x: start + status_w + gap, y: inset, w: clock_w, h: height - 2 * inset };
    let desktop = R { x: clock.x + clock.w + gap, y: inset, w: px(DESKTOP_DIP, scale), h: height - 2 * inset };
    Shape { chevron, tray, status, clock, desktop, width: desktop.x + desktop.w + pad }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatusPart {
    Network,
    Volume,
    Battery,
}

// Qué glifo de la píldora de estado está bajo `x` (coordenadas del cluster): red, volumen o batería.
pub fn status_part(s: &Shape, scale: f32, x: i32) -> StatusPart {
    let (side, glyph, gap) = (px(SIDE_DIP, scale), px(GLYPH_DIP, scale), px(GAP_DIP, scale));
    let rel = x - s.status.x;
    let has_battery = s.status.w > 2 * side + 2 * glyph + gap;
    if rel < side + glyph + gap / 2 {
        StatusPart::Network
    } else if !has_battery || rel < side + 2 * glyph + gap + gap / 2 {
        StatusPart::Volume
    } else {
        StatusPart::Battery
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuEntry {
    Open(&'static str),
    Mute,
}

pub fn context_entries(part: Part, status: StatusPart) -> Vec<(K, MenuEntry)> {
    match (part, status) {
        (Part::Status, StatusPart::Network) => vec![(K::MenuNetworkSettings, MenuEntry::Open("ms-settings:network")), (K::MenuNetworkConnections, MenuEntry::Open("ncpa.cpl"))],
        (Part::Status, StatusPart::Volume) => vec![
            (K::MenuSoundSettings, MenuEntry::Open("ms-settings:sound")),
            (K::MenuVolumeMixer, MenuEntry::Open("ms-settings:apps-volume")),
            (K::MenuMute, MenuEntry::Mute),
        ],
        (Part::Status, StatusPart::Battery) => vec![(K::MenuPowerSettings, MenuEntry::Open("ms-settings:powersleep"))],
        (Part::Clock, _) => vec![(K::MenuDateTime, MenuEntry::Open("ms-settings:dateandtime")), (K::MenuNotificationSettings, MenuEntry::Open("ms-settings:notifications"))],
        _ => Vec::new(),
    }
}

// Los botones ocupan toda la altura de la píldora: el margen vertical es hover, no un hueco muerto.
pub fn hit(s: &Shape, x: i32, y: i32, height: i32) -> Option<Part> {
    let inside = |r: R| x >= r.x && x < r.x + r.w && y >= 0 && y < height;
    if inside(s.chevron) {
        Some(Part::Chevron)
    } else if let Some(i) = s.tray.iter().position(|r| inside(*r)) {
        Some(Part::Tray(i))
    } else if inside(s.status) {
        Some(Part::Status)
    } else if inside(s.clock) {
        Some(Part::Clock)
    } else if inside(s.desktop) {
        Some(Part::Desktop)
    } else {
        None
    }
}

// Dónde va la píldora: borde derecho del monitor, a la altura de la barra del dock. No se muestra si pisaría al dock.
// Ancho en dip que el dock deja libre a cada lado para que el cluster siempre entre.
// Es el ancho máximo de `shape` (batería, contador y todos los íconos fijados) más el margen de cada lado y holgura por el redondeo de px().
pub const RESERVE_DIP: i32 = PAD_DIP + (crate::tray::PINNED_MAX as i32 + 1) * TRAY_CELL_DIP + 2 * GAP_DIP
    + 2 * SIDE_DIP + 2 * GLYPH_DIP + GAP_DIP + BATTERY_DIP + GAP_DIP + GAP_DIP + CLOCK_DIP + BADGE_DIP + GAP_DIP + GAP_DIP + DESKTOP_DIP + PAD_DIP
    + 2 * crate::dock::MARGIN_DIP + 8;

pub fn place(mon: (i32, i32, i32, i32), scale: f32, bottom: bool, dock_right: i32, width: i32) -> Option<(i32, i32)> {
    let margin = px(MARGIN_DIP, scale);
    let x = mon.2 - margin - width;
    let y = if bottom { mon.3 - margin - px(BAR_DIP, scale) } else { mon.1 + margin };
    (x >= dock_right + margin).then_some((x, y))
}

pub fn clock_text(h: u16, m: u16) -> String {
    format!("{h:02}:{m:02}")
}

pub fn date_text(d: u16, m: u16, y: u16, lang: Lang) -> String {
    match lang {
        Lang::Es => format!("{d:02}/{m:02}/{y}"),
        Lang::En => format!("{m:02}/{d:02}/{y}"),
    }
}

pub fn volume_label(v: Option<(u32, bool)>) -> String {
    match v {
        Some((_, true)) => t(K::MuteValue).to_string(),
        Some((p, false)) => format!("{p}%"),
        None => String::new(),
    }
}

pub fn status_label(s: &Snapshot) -> String {
    let net = match s.online {
        Some(true) => t(K::Online),
        Some(false) => t(K::NetOff),
        None => "",
    };
    [t(K::QuickSettings).to_string(), net.to_string(), volume_label(s.volume)].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" · ")
}

pub fn wheel_step(delta: i32) -> i32 {
    delta.signum() * (delta.abs() / 60).max(1)
}

// Qué dock lleva el cluster: `primary` es el monitor con origen (0,0); un número es la posición 1-based entre los docks.
pub fn owns(setting: &str, mon: (i32, i32, i32, i32), index: usize) -> bool {
    match setting {
        "off" => false,
        "primary" | "" => (mon.0, mon.1) == (0, 0),
        n => n.parse::<usize>().is_ok_and(|n| n == index + 1),
    }
}

struct Cluster {
    hwnd: HWND,
    state_dir: std::path::PathBuf,
    theme: Theme,
    mon: (i32, i32, i32, i32),
    scale: f32,
    bottom: bool,
    dock_right: i32,
    vis: f32,
    slide: i32,
    hover: Option<Part>,
    hover_t: Vec<f32>,
    pressed: Option<Part>,
    tracking: bool,
    last: Option<std::time::Instant>,
    sig: u64,
    shape: Shape,
    pos: Option<(i32, i32)>,
    anim_on: bool,
    tip: HWND,
}

// El hilo de muestreo no ve el thread_local: avisa por el HWND.
// El dock que aloja el cluster pone el foco de teclado sobre el chevron (el botón de bandeja ya no es suyo).
static KEY_FOCUS: AtomicBool = AtomicBool::new(false);

pub fn set_key_focus(on: bool) {
    if KEY_FOCUS.swap(on, Ordering::Relaxed) != on {
        notify();
    }
}

static HWND_ATOMIC: AtomicIsize = AtomicIsize::new(0);

thread_local! { static CLUSTER: RefCell<Option<Cluster>> = const { RefCell::new(None) }; }

fn with<R>(f: impl FnOnce(&mut Cluster) -> R) -> Option<R> {
    CLUSTER.with(|c| c.try_borrow_mut().ok().and_then(|mut c| c.as_mut().map(f)))
}

pub fn init(theme: &Theme, state_dir: &std::path::Path) {
    let _ = STATE_DIR.set(state_dir.to_path_buf());
    if theme.cluster_monitor == "off" {
        return;
    }
    unsafe {
        let Ok(inst) = GetModuleHandleW(PCWSTR::null()) else { return };
        let class = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: inst.into(), hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(), lpszClassName: w!("WinarchyShellCluster"), style: CS_DBLCLKS, ..Default::default() };
        RegisterClassW(&class);
        let ex = WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_LAYERED;
        let Ok(hwnd) = CreateWindowExW(ex, w!("WinarchyShellCluster"), w!(""), WS_POPUP, 0, 0, 1, 1, None, None, inst, None) else { return };
        crate::accessibility::create(hwnd, "Winarchy", accesskit::Role::Toolbar);
        let blank = Shape { chevron: R::default(), tray: Vec::new(), status: R::default(), clock: R::default(), desktop: R::default(), width: 0 };
        let c = Cluster { hwnd, state_dir: state_dir.to_path_buf(), theme: theme.clone(), mon: (0, 0, 0, 0), scale: 1.0, bottom: true, dock_right: 0, vis: 0.0, slide: 0, hover: None, hover_t: Vec::new(), pressed: None, tracking: false, last: None, sig: 0, shape: blank, pos: None, anim_on: false, tip: make_tip(hwnd) };
        CLUSTER.with(|s| *s.borrow_mut() = Some(c));
        HWND_ATOMIC.store(hwnd.0 as isize, Ordering::Relaxed);
        SetTimer(hwnd, T_SECOND, SECOND_MS, None);
    }
}

static STATE_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

// `cluster_monitor: off` destruye la píldora; al volver a otro valor se recrea.
pub fn set_theme(theme: &Theme) {
    if theme.cluster_monitor == "off" {
        destroy();
        return;
    }
    if with(|_| ()).is_none() {
        if let Some(dir) = STATE_DIR.get() {
            init(theme, dir);
        }
    }
    with(|c| {
        c.theme = theme.clone();
        c.sig = 0;
    });
    paint();
}

pub fn destroy() {
    let Some((hwnd, tip)) = with(|c| (c.hwnd, c.tip)) else { return };
    HWND_ATOMIC.store(0, Ordering::Relaxed);
    CLUSTER.with(|c| *c.borrow_mut() = None);
    unsafe {
        let _ = DestroyWindow(hwnd);
        let _ = DestroyWindow(tip);
    }
}

// Lo llama el dock dueño al terminar de dibujarse: dónde quedó su barra, cuánto se ve y cuánto se desliza.
pub fn sync(theme: &Theme, mon: (i32, i32, i32, i32), scale: f32, dock_right: i32, vis: f32, slide: i32) {
    let known = with(|c| {
        c.theme = theme.clone();
        c.mon = mon;
        c.scale = scale;
        c.bottom = theme.position != "bottom";
        c.dock_right = dock_right;
        c.vis = vis;
        c.slide = slide;
    });
    if known.is_some() {
        paint();
    }
}

// El dock mantiene su borde asomado mientras el mouse esté sobre la píldora.
pub fn contains(p: POINT) -> bool {
    with(|c| c.pos.is_some_and(|(x, y)| c.vis > 0.0 && p.x >= x && p.x < x + c.shape.width && p.y >= y && p.y < y + px(BAR_DIP, c.scale))).unwrap_or(false)
}

// El borde que revela al dock también vale frente a la píldora: queda a la derecha de la barra, fuera de su zona.
pub fn at_edge(mon: (i32, i32, i32, i32), p: POINT) -> bool {
    with(|c| c.pos.is_some_and(|(x, _)| c.mon == mon && edge_zone(mon, c.bottom, x, c.shape.width, px(BAR_DIP, c.scale), (p.x, p.y)))).unwrap_or(false)
}

fn edge_zone(mon: (i32, i32, i32, i32), bottom: bool, x: i32, width: i32, height: i32, p: (i32, i32)) -> bool {
    let edge = if bottom { p.1 >= mon.3 - 2 } else { p.1 < mon.1 + 2 };
    p.0 >= mon.0 && p.0 < mon.2 && edge && p.0 >= x - height && p.0 < x + width + height
}

fn now() -> (u16, u16, u16, u16, u16) {
    let t = unsafe { GetLocalTime() };
    (t.wHour, t.wMinute, t.wDay, t.wMonth, t.wYear)
}

fn paint() {
    let Some((hwnd, theme, scale, vis, slide, bottom, dock_right, mon, hover_t)) = with(|c| (c.hwnd, c.theme.clone(), c.scale, c.vis, c.slide, c.bottom, c.dock_right, c.mon, c.hover_t.clone())) else { return };
    let snap = system::snapshot();
    let height = px(BAR_DIP, scale);
    let badge = snap.notifications.is_some_and(|n| n > 0);
    let pinned = crate::tray::with_pinned(|i| i.len());
    let shape = shape(scale, snap.battery.is_some(), badge, pinned, height);
    let pos = place(mon, scale, bottom, dock_right, shape.width);
    let (hh, mm, dd, mo, yy) = now();
    let sig = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (snap.volume, snap.online, snap.battery, snap.notifications, (hh, mm, dd, mo, yy), (vis.to_bits(), slide, pos, hover_t.iter().map(|t| t.to_bits()).collect::<Vec<_>>(), shape.width, crate::tray::revision()), (lang() == Lang::Es, &theme.accent, &theme.background, &theme.foreground), KEY_FOCUS.load(Ordering::Relaxed)).hash(&mut h);
        h.finish()
    };
    if with(|c| std::mem::replace(&mut c.sig, sig) == sig).unwrap_or(true) {
        return;
    }
    with(|c| {
        c.shape = shape.clone();
        c.pos = pos;
    });
    unsafe {
        let Some((x, y)) = pos.filter(|_| vis > 0.0) else {
            let _ = ShowWindow(hwnd, SW_HIDE);
            return;
        };
        let (w, h) = (shape.width, height);
        let (bg, fg, accent) = (rgb(&theme.background), rgb(&theme.foreground), rgb(&theme.accent));
        let st = style(&theme);
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(screen);
        let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: 0, ..Default::default() }, ..Default::default() };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        if let Ok(dib) = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            let old = SelectObject(mem, dib);
            let buf = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
            buf.fill(0);
            fill_round_a(buf, w, h, R { x: 0, y: 0, w, h }, px(st.radius, scale), bg, st.opacity);
            let parts = [(Part::Status, shape.status), (Part::Clock, shape.clock), (Part::Desktop, shape.desktop), (Part::Chevron, shape.chevron)].into_iter().chain(shape.tray.iter().enumerate().map(|(i, r)| (Part::Tray(i), *r)));
            for (part, r) in parts {
                let t = hover_t.get(part.index()).copied().unwrap_or(0.0);
                if t > 0.0 {
                    fill_round_a(buf, w, h, r, px(10, scale), fg, 0.12 * ease(t));
                }
            }
            if KEY_FOCUS.load(Ordering::Relaxed) {
                fill_round_a(buf, w, h, shape.chevron, px(10, scale), accent, 0.5);
            }
            let rule = R { x: shape.desktop.x + shape.desktop.w / 2 - px(1, scale), y: shape.desktop.y + px(4, scale), w: px(2, scale), h: shape.desktop.h - 2 * px(4, scale) };
            fill_round_a(buf, w, h, rule, px(1, scale), fg, 0.3);
            draw_label(buf, w, h, shape.chevron, GLYPH_CHEVRON, ICONS, scale, fg, theme.font_size.max(11) + 1);
            draw_tray(buf, w, h, &shape.tray, &hover_t, scale);
            draw_status(buf, w, h, shape.status, &snap, &theme, scale, fg, accent);
            draw_clock(buf, w, h, shape.clock, snap.notifications.unwrap_or(0), (hh, mm, dd, mo, yy), &theme, scale, fg, accent);
            let pos = POINT { x, y: y + if bottom { slide } else { -slide } };
            let sz = SIZE { cx: w, cy: h };
            let src = POINT::default();
            let bf = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: (vis.clamp(0.0, 1.0) * 255.0).round() as u8, AlphaFormat: 1 };
            let _ = UpdateLayeredWindow(hwnd, None, Some(&pos), Some(&sz), mem, Some(&src), COLORREF(0), Some(&bf), ULW_ALPHA);
            SelectObject(mem, old);
            let _ = DeleteObject(dib);
        }
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        if !IsWindowVisible(hwnd).as_bool() {
            let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    publish(&snap, &shape, height);
}

// Apagar un glifo es bajarle el color: el texto se compone con su propio alfa, sin conocer el fondo.
fn dim(c: (u8, u8, u8), alpha: f32) -> (u8, u8, u8) {
    let f = |v: u8| (v as f32 * alpha) as u8;
    (f(c.0), f(c.1), f(c.2))
}

fn draw_tray(buf: &mut [u32], w: i32, h: i32, cells: &[R], hover_t: &[f32], scale: f32) {
    let side = px(TRAY_ICON_DIP, scale);
    crate::tray::with_pinned(|icons| {
        for (i, (icon, cell)) in icons.iter().zip(cells).enumerate() {
            let lift = (hover_t.get(4 + i).copied().unwrap_or(0.0) * px(1, scale) as f32) as i32;
            let (ox, oy) = (cell.x + (cell.w - side) / 2, cell.y + (cell.h - side) / 2 - lift);
            for (k, p) in crate::tray_flyout::scaled(&icon.bgra, icon.size, side).into_iter().enumerate() {
                let (x, y) = (ox + (k as i32 % side), oy + (k as i32 / side));
                if x >= 0 && x < w && y >= 0 && y < h {
                    crate::dock::blend(&mut buf[(y * w + x) as usize], p);
                }
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
unsafe fn draw_status(buf: &mut [u32], w: i32, h: i32, r: R, snap: &Snapshot, theme: &Theme, scale: f32, fg: (u8, u8, u8), accent: (u8, u8, u8)) {
    let (side, glyph, gap) = (px(SIDE_DIP, scale), px(GLYPH_DIP, scale), px(GAP_DIP, scale));
    let cell = |x: i32, wd: i32| R { x, y: r.y, w: wd, h: r.h };
    let font = theme.font_size.max(11) + 3;
    let mut x = r.x + side;
    let muted_col = rgb(&theme.muted);
    let net = if snap.online == Some(false) { muted_col } else { fg };
    draw_label(buf, w, h, cell(x, glyph), GLYPH_NET, ICONS, scale, net, font);
    x += glyph + gap;
    let muted = snap.volume.is_some_and(|(_, m)| m);
    draw_label(buf, w, h, cell(x, glyph), if muted { GLYPH_MUTE } else { GLYPH_VOLUME }, ICONS, scale, if muted { muted_col } else { fg }, font);
    x += glyph;
    if let Some((pct, charging)) = snap.battery {
        x += gap;
        let col = if charging { accent } else if pct < 20 { theme.urgent.as_deref().map(rgb).unwrap_or(accent) } else { fg };
        let width = px(BATTERY_DIP, scale);
        draw_label(buf, w, h, cell(x, width), &format!("{} {pct}", if charging { GLYPH_CHARGING } else { GLYPH_BATTERY }), ICONS, scale, col, font - 4);
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn draw_clock(buf: &mut [u32], w: i32, h: i32, r: R, count: usize, now: (u16, u16, u16, u16, u16), theme: &Theme, scale: f32, fg: (u8, u8, u8), accent: (u8, u8, u8)) {
    let (hh, mm, dd, mo, yy) = now;
    let gap = px(GAP_DIP, scale);
    let badge = count > 0;
    let bell_w = if badge { px(BADGE_DIP, scale) + gap } else { 0 };
    let half = r.h / 2;
    let top = R { x: r.x, y: r.y + px(1, scale), w: r.w - bell_w, h: half };
    let bottom = R { y: r.y + half, h: half - px(1, scale), ..top };
    draw_label(buf, w, h, top, &clock_text(hh, mm), &theme.font_family, scale, fg, theme.font_size);
    draw_label(buf, w, h, bottom, &date_text(dd, mo, yy, lang()), &theme.font_family, scale, dim(fg, 0.7), (theme.font_size - 2).max(10));
    if badge {
        let d = px(BADGE_DIP, scale);
        let b = R { x: r.x + r.w - d - px(2, scale), y: r.y + (r.h - d) / 2, w: d, h: d };
        fill_round_a(buf, w, h, b, d / 2, accent, 1.0);
        let label = if count > 9 { "9+".to_string() } else { count.to_string() };
        draw_label(buf, w, h, b, &label, &theme.font_family, scale, crate::theme::contrasting_color(accent), 11);
    }
}

fn publish(snap: &Snapshot, shape: &Shape, height: i32) {
    let Some(hwnd) = with(|c| c.hwnd) else { return };
    let node = |label: String, r: R| {
        let mut n = accesskit::Node::new(accesskit::Role::Button);
        n.set_label(label);
        n.set_bounds(accesskit::Rect::new(r.x as f64, 0.0, (r.x + r.w) as f64, height as f64));
        n.add_action(accesskit::Action::Click);
        n
    };
    let (hh, mm, dd, mo, yy) = now();
    let clock = format!("{} {} {}", t(K::DateTime), clock_text(hh, mm), date_text(dd, mo, yy, lang()));
    let clock = match snap.notifications {
        Some(n) if n > 0 => format!("{clock} · {n}"),
        _ => clock,
    };
    let mut nodes = crate::tray::with_pinned(|icons| icons.iter().zip(&shape.tray).enumerate().map(|(i, (icon, r))| (format!("tray:{i}"), node(if icon.tip.is_empty() { icon.exe.clone() } else { icon.tip.lines().next().unwrap_or_default().to_string() }, *r))).collect::<Vec<_>>());
    nodes.push(("chevron".to_string(), node(t(K::SystemTray).to_string(), shape.chevron)));
    nodes.push(("status".to_string(), node(status_label(snap), shape.status)));
    nodes.push(("clock".to_string(), node(clock, shape.clock)));
    nodes.push(("desktop".to_string(), node(t(K::ShowDesktop).to_string(), shape.desktop)));
    crate::accessibility::publish(hwnd, "Winarchy", (shape.width, height), nodes, None);
}

// Menú contextual nativo, como el de la taskbar de Windows: los ajustes del glifo bajo el cursor.
unsafe fn context_menu(h: HWND, entries: &[(K, MenuEntry)]) {
    if entries.is_empty() {
        return;
    }
    let Ok(menu) = CreatePopupMenu() else { return };
    for (i, (key, _)) in entries.iter().enumerate() {
        let text: Vec<u16> = t(*key).encode_utf16().chain(std::iter::once(0)).collect();
        let _ = AppendMenuW(menu, MF_STRING, i + 1, PCWSTR(text.as_ptr()));
    }
    let mut p = POINT::default();
    let _ = GetCursorPos(&mut p);
    let previous = GetForegroundWindow();
    let _ = SetForegroundWindow(h);
    let chosen = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, p.x, p.y, 0, h, None).0;
    let _ = DestroyMenu(menu);
    let _ = PostMessageW(h, WM_NULL, WPARAM(0), LPARAM(0));
    // sin esto la app del usuario queda desactivada y el teclado no llega a ningún lado hasta el próximo clic
    if GetForegroundWindow() == h && IsWindow(previous).as_bool() {
        let _ = SetForegroundWindow(previous);
    }
    match chosen.checked_sub(1).and_then(|i| entries.get(i as usize)) {
        Some((_, MenuEntry::Open(target))) => crate::dock_menu::launch(target, ""),
        Some((_, MenuEntry::Mute)) => {
            let _ = crate::metrics::toggle_mute();
        }
        None => {}
    }
}

fn send_tray(index: usize, gesture: crate::tray::Gesture) {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    if let Some((hwnd, uid, guid)) = crate::tray::with_pinned(|v| v.get(index).map(|i| (i.hwnd, i.uid, i.guid))) {
        crate::tray::click_identity(hwnd, uid, guid, gesture, (p.x, p.y));
    }
}

// Mostrar escritorio como la taskbar: minimiza todo y la segunda vez lo restaura, con los comandos de Shell_TrayWnd (sin teclas).
fn toggle_desktop() {
    static SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let cmd = if SHOWN.fetch_xor(true, Ordering::Relaxed) { 416 } else { 419 };
    unsafe {
        if let Ok(tray) = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) {
            let _ = PostMessageW(tray, WM_COMMAND, WPARAM(cmd), LPARAM(0));
        }
    }
}

// El flyout nace sobre el chevron, a la derecha como en Windows.
pub fn open_tray() {
    let Some((r, pos, mon, bottom, scale, theme, slide)) = with(|c| (c.shape.chevron, c.pos, c.mon, c.bottom, c.scale, c.theme.clone(), c.slide)) else { return };
    let Some((x, y)) = pos else { return };
    let y = y + if bottom { slide } else { -slide };
    let button = R { x: x + r.x, y: y + r.y, w: r.w, h: r.h };
    if !crate::tray_flyout::toggle(button, mon, bottom, scale, &theme) {
        crate::dock::show_tray();
    }
}

fn activate(part: Part) {
    let name = match part {
        Part::Status => "show-quick-settings.flag",
        Part::Clock => "show-notifications.flag",
        Part::Desktop => return toggle_desktop(),
        Part::Chevron => return open_tray(),
        Part::Tray(i) => return send_tray(i, crate::tray::Gesture::Left),
    };
    if let Some(dir) = with(|c| c.state_dir.clone()) {
        let _ = std::fs::OpenOptions::new().write(true).create_new(true).open(dir.join(name));
    }
}

unsafe fn start_anim(h: HWND) {
    if with(|c| std::mem::replace(&mut c.anim_on, true)) == Some(false) {
        SetTimer(h, T_ANIM, ANIM_MS, None);
    }
}

unsafe fn anim_tick(h: HWND) {
    let busy = with(|c| {
        let dt = c.last.map_or(ANIM_MS as f32, |l| l.elapsed().as_secs_f32() * 1000.0).min(64.0);
        c.last = Some(std::time::Instant::now());
        let mut busy = false;
        let parts = [Part::Status, Part::Clock, Part::Desktop, Part::Chevron].into_iter().chain((0..c.shape.tray.len()).map(Part::Tray));
        c.hover_t.resize(4 + c.shape.tray.len(), 0.0);
        for part in parts {
            let target = if c.hover == Some(part) { 1.0 } else { 0.0 };
            let i = part.index();
            c.hover_t[i] = approach(c.hover_t[i], target, dt, HOVER_MS);
            busy |= c.hover_t[i] != target;
        }
        busy
    })
    .unwrap_or(false);
    paint();
    if !busy {
        let _ = KillTimer(h, T_ANIM);
        with(|c| {
            c.anim_on = false;
            c.last = None;
        });
    }
}

// Tooltip nativo con una sola herramienta que sigue a la parte bajo el mouse (TTF_SUBCLASS: él mismo recibe los mensajes del mouse).
unsafe fn make_tip(owner: HWND) -> HWND {
    // La clase tooltips_class32 solo existe una vez cargado comctl32.
    let _ = InitCommonControlsEx(&INITCOMMONCONTROLSEX { dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32, dwICC: ICC_WIN95_CLASSES });
    let tip = CreateWindowExW(WS_EX_TOPMOST, TOOLTIPS_CLASSW, PCWSTR::null(), WINDOW_STYLE(WS_POPUP.0 | TTS_NOPREFIX | TTS_ALWAYSTIP), 0, 0, 0, 0, owner, None, GetModuleHandleW(PCWSTR::null()).map(|m| HINSTANCE(m.0)).unwrap_or_default(), None).unwrap_or_default();
    let ti = TTTOOLINFOW { cbSize: TOOLINFO_V1_SIZE, uFlags: TTF_SUBCLASS, hwnd: owner, uId: 1, ..Default::default() };
    SendMessageW(tip, TTM_ADDTOOLW, WPARAM(0), LPARAM(&ti as *const _ as isize));
    tip
}

fn part_tip(part: Part) -> String {
    match part {
        Part::Tray(i) => crate::tray::with_pinned(|icons| icons.get(i).map(|icon| if icon.tip.is_empty() { icon.exe.clone() } else { icon.tip.lines().next().unwrap_or_default().to_string() })).unwrap_or_default(),
        Part::Desktop => t(K::ShowDesktop).to_string(),
        Part::Chevron => t(K::SystemTray).to_string(),
        Part::Status | Part::Clock => String::new(),
    }
}

unsafe fn point_tip(part: Option<Part>) {
    let Some((tip, hwnd, rect)) = with(|c| {
        let r = match part {
            Some(Part::Tray(i)) => c.shape.tray.get(i).copied(),
            Some(Part::Desktop) => Some(c.shape.desktop),
            Some(Part::Chevron) => Some(c.shape.chevron),
            _ => None,
        }
        .unwrap_or_default();
        (c.tip, c.hwnd, RECT { left: r.x, top: 0, right: r.x + r.w, bottom: if r.w > 0 { px(BAR_DIP, c.scale) } else { 0 } })
    }) else { return };
    let mut text: Vec<u16> = part.map(part_tip).unwrap_or_default().encode_utf16().chain(std::iter::once(0)).collect();
    let ti = TTTOOLINFOW { cbSize: TOOLINFO_V1_SIZE, hwnd, uId: 1, rect, lpszText: PWSTR(text.as_mut_ptr()), ..Default::default() };
    SendMessageW(tip, TTM_NEWTOOLRECTW, WPARAM(0), LPARAM(&ti as *const _ as isize));
    SendMessageW(tip, TTM_UPDATETIPTEXTW, WPARAM(0), LPARAM(&ti as *const _ as isize));
}

fn hit_at(x: i32, y: i32) -> Option<Part> {
    with(|c| hit(&c.shape, x, y, px(BAR_DIP, c.scale))).flatten()
}

fn client(lp: LPARAM) -> (i32, i32) {
    ((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32)
}

unsafe extern "system" fn proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if m == WM_GETOBJECT {
        if let Some(r) = crate::accessibility::get_object(h, wp, lp) {
            return r;
        }
    }
    match m {
        WM_NCDESTROY => crate::accessibility::destroy(h),
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        WM_TIMER if wp.0 == T_SECOND => {
            paint();
            return LRESULT(0);
        }
        WM_TIMER if wp.0 == T_ANIM => {
            anim_tick(h);
            return LRESULT(0);
        }
        WM_SYNC => {
            paint();
            return LRESULT(0);
        }
        WM_MOUSEMOVE => {
            let (x, y) = client(lp);
            let part = hit_at(x, y);
            if with(|c| std::mem::replace(&mut c.hover, part)) != Some(part) {
                point_tip(part);
                start_anim(h);
            }
            if let Some(tip) = with(|c| c.tip) {
                let msg = MSG { hwnd: h, message: m, wParam: wp, lParam: lp, ..Default::default() };
                SendMessageW(tip, TTM_RELAYEVENT, WPARAM(0), LPARAM(&msg as *const _ as isize));
            }
            if with(|c| std::mem::replace(&mut c.tracking, true)) == Some(false) {
                let _ = TrackMouseEvent(&mut TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: h, dwHoverTime: 0 });
            }
            return LRESULT(0);
        }
        MOUSE_LEAVE => {
            with(|c| {
                c.hover = None;
                c.tracking = false;
                c.pressed = None;
            });
            start_anim(h);
            return LRESULT(0);
        }
        WM_LBUTTONDOWN => {
            let (x, y) = client(lp);
            let part = hit_at(x, y);
            with(|c| c.pressed = part);
            return LRESULT(0);
        }
        WM_LBUTTONDBLCLK => {
            let (x, y) = client(lp);
            let part = hit_at(x, y);
            if let Some(Part::Tray(i)) = part {
                with(|c| c.pressed = None);
                send_tray(i, crate::tray::Gesture::LeftDouble);
            } else {
                with(|c| c.pressed = part);
            }
            return LRESULT(0);
        }
        WM_LBUTTONUP => {
            let (x, y) = client(lp);
            let down = with(|c| c.pressed.take()).flatten();
            if let (Some(d), Some(u)) = (down, hit_at(x, y)) {
                if d == u {
                    activate(u);
                }
            }
            return LRESULT(0);
        }
        WM_MBUTTONUP => {
            let (x, y) = client(lp);
            match hit_at(x, y) {
                Some(Part::Status) => {
                    let _ = crate::metrics::toggle_mute();
                }
                Some(Part::Tray(i)) => send_tray(i, crate::tray::Gesture::Middle),
                _ => {}
            }
            return LRESULT(0);
        }
        WM_RBUTTONUP => {
            let (x, y) = client(lp);
            match hit_at(x, y) {
                Some(Part::Chevron) => open_tray(),
                Some(Part::Tray(i)) => send_tray(i, crate::tray::Gesture::Right),
                Some(part @ (Part::Status | Part::Clock)) => {
                    let status = with(|c| status_part(&c.shape, c.scale, x)).unwrap_or(StatusPart::Volume);
                    context_menu(h, &context_entries(part, status));
                }
                _ => {}
            }
            return LRESULT(0);
        }
        WM_MOUSEWHEEL => {
            // La rueda llega en coordenadas de pantalla.
            let mut p = POINT { x: (lp.0 & 0xFFFF) as i16 as i32, y: ((lp.0 >> 16) & 0xFFFF) as i16 as i32 };
            let _ = ScreenToClient(h, &mut p);
            if hit_at(p.x, p.y) == Some(Part::Status) {
                let delta = ((wp.0 >> 16) & 0xFFFF) as i16 as i32;
                let _ = crate::metrics::adjust_volume(wheel_step(delta));
            }
            return LRESULT(0);
        }
        m if m == crate::accessibility::WM_ACTION => {
            for (key, action) in crate::accessibility::actions(h, wp.0) {
                if action == accesskit::Action::Click {
                    activate(match key.strip_prefix("tray:").and_then(|i| i.parse().ok()) {
                        Some(i) => Part::Tray(i),
                        None if key == "status" => Part::Status,
                        None if key == "desktop" => Part::Desktop,
                        None if key == "chevron" => Part::Chevron,
                        None => Part::Clock,
                    });
                }
            }
            return LRESULT(0);
        }
        _ => {}
    }
    DefWindowProcW(h, m, wp, lp)
}

// Un cambio de estado del sistema pide repintar sin esperar al reloj de un segundo.
pub fn notify() {
    let h = HWND_ATOMIC.load(Ordering::Relaxed);
    if h != 0 {
        unsafe {
            let _ = PostMessageW(HWND(h as *mut _), WM_SYNC, WPARAM(0), LPARAM(0));
        }
    }
}

#[cfg(test)]
#[path = "cluster_tests.rs"]
mod tests;
