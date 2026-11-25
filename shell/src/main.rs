#![windows_subsystem = "windows"]

mod apps;
#[cfg(test)]
mod bench_tests;
mod accessibility;
mod dock;
mod desktop;
mod desktop_view;
mod dock_drop;
mod dock_menu;
mod dock_recent;
mod dock_thumb;
mod brightness;
mod calendar;
mod cluster;
mod world;
mod extras;
mod i18n;
mod ics;
mod layout;
mod metrics;
mod komorebi;
mod pipe;
mod spirit;
mod system;
mod theme;
mod tray;
mod tray_flyout;
mod tray_host;
mod tray_notifications;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use i18n::{lang, t as tr, K};
use layout::{px, Hit, Inputs, Layout, PanelShape, R};
use theme::{rgb, Theme};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_CMD_RELOAD: u32 = WM_APP + 2;
const WM_CMD_QUIT: u32 = WM_APP + 3;
const WM_CMD_RESTART: u32 = WM_APP + 11;

enum RestartAction { Reload, Exit }

fn restart_action(session_shell: bool) -> RestartAction {
    if session_shell { RestartAction::Reload } else { RestartAction::Exit }
}
const WM_CMD_RETURN: u32 = WM_APP + 12;
const WM_CMD_RETURN_READY: u32 = WM_APP + 44;
const WM_CMD_RETURN_FAILED: u32 = WM_APP + 45;
const WM_WORKSPACES: u32 = WM_APP + 4;
const WM_CMD_TRAY: u32 = WM_APP + 8;
const WM_CMD_TRAY_DISMISS: u32 = WM_APP + 9;
const WM_CMD_TOGGLE_DESKTOP: u32 = WM_APP + 174;
const WM_CMD_DOCK_FOCUS: u32 = WM_APP + 10;
const WM_CMD_DOCK_PIN: u32 = WM_APP + 43;
const WM_EXPAND: u32 = WM_APP + 5;
const WM_COLLAPSE: u32 = WM_APP + 6;
const WM_BRIGHTNESS: u32 = WM_APP + 7;
const WM_MOUSELEAVE: u32 = 0x02A3;
const EXPAND_SECS: u64 = 6;
const TIMER_TICK: usize = 1;
const TIMER_ANIM: usize = 2;
const TIMER_SPIRIT: usize = 3;
const TIMER_FLASH: usize = 4;
const TIMER_HOVER: usize = 5;
const FLASH_MS: u32 = 800;
const HOVER_MS: u32 = 120;
const PILL_MAX_W: i32 = 1400;
const FADE_SECS: f32 = 0.18;
static RETURN_TO_EXPLORER_PENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

struct State {
    session_shell: bool,
    state_dir: PathBuf,
    theme: Theme,
    theme_path: PathBuf,
    theme_mtime: Option<SystemTime>,
    scale: f32,
    pill: HWND,
    strip: HWND,
    cur_w: f32,
    vel: f32,
    target_w: f32,
    cur_h: f32,
    vel_h: f32,
    target_h: f32,
    anim_last: Option<std::time::Instant>,
    last_minute: u16,
    hidden: bool,
    expand_until: Option<std::time::Instant>,
    sampler: metrics::Sampler,
    last_vol: Option<(u32, bool)>,
    osd_until: Option<std::time::Instant>,
    last_ws: Option<usize>,
    flash_title: Option<String>,
    last_label: Vec<u16>,
    tracking: bool,
    pinned: bool,
    panel: Option<Panel>,
    panel_shown: Option<Panel>,
    last_layout: String,
    last_mode: Option<komorebi::Mode>,
    layout: Layout,
    pill_mon: RECT,
    layout_sig: Vec<(i32, i32, i32, i32, u32)>,
    fade: f32,
    was_open: bool,
    clock_open: bool,
    cal_ym: (i64, u32),
    cal_sel: i64,
    countdown: Option<String>,
    countdown_minute: u16,
}

// Contenido del panel expandido; se rearma en cada tick mientras la isla está abierta.
#[derive(Clone)]
struct Panel {
    media: Option<extras::Media>,
    weather: Option<extras::Weather>,
    meters: Vec<(Meter, u32, String)>,
    notes: String,
    cal: Option<CalView>,
}

// Panel del reloj: mes con marcas de días con eventos y la agenda del día elegido.
#[derive(Clone)]
struct CalView {
    title: String,
    rows: usize,
    cells: Vec<Option<u32>>,
    today: Option<u32>,
    sel: Option<u32>,
    marks: Vec<bool>,
    agenda: Vec<String>,
    world: String,
}

#[derive(Debug, PartialEq, Eq)]
enum DockChange {
    Enable,
    Disable,
    Update,
    None,
}

fn dock_change(previous: bool, current: bool) -> DockChange {
    match (previous, current) {
        (false, true) => DockChange::Enable,
        (true, false) => DockChange::Disable,
        (true, true) => DockChange::Update,
        (false, false) => DockChange::None,
    }
}

const AGENDA_LINES: usize = 3;

fn day_in_month(days: i64, (y, m): (i64, u32)) -> Option<u32> {
    let (dy, dm, dd) = calendar::civil_from_days(days);
    ((dy, dm) == (y, m)).then_some(dd)
}

// Día que se elige al abrir un mes: hoy si es el actual, si no el 1.
pub(crate) unsafe fn toggle_clock() {
    with(|s| {
        s.clock_open = !s.clock_open;
        s.fade = 0.0;
        if s.clock_open {
            let today = calendar::now_local().0;
            let (y, m, _) = calendar::civil_from_days(today);
            s.cal_ym = (y, m);
            s.cal_sel = today;
        }
        s.expand_until = Some(std::time::Instant::now() + std::time::Duration::from_secs(EXPAND_SECS));
    });
    tick(with(|s| s.strip));
}

pub(crate) fn session_shell_active() -> bool {
    with(|s| s.session_shell)
}

fn default_sel((y, m): (i64, u32)) -> i64 {
    let today = calendar::now_local().0;
    if day_in_month(today, (y, m)).is_some() { today } else { calendar::days_from_civil(y, m, 1) }
}

fn short(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else { format!("{}…", s.chars().take(max - 1).collect::<String>()) }
}

fn calendar_view(ym: (i64, u32), sel: i64, configured: bool, world: String) -> CalView {
    use calendar::{days_from_civil, days_in_month, MIN_DAY};
    let (rows, cells) = calendar::month_grid(ym.0, ym.1);
    let first = days_from_civil(ym.0, ym.1, 1);
    let dim = days_in_month(ym.0, ym.1) as i64;
    let feed = ics::FEED.lock().unwrap();
    let mut marks = vec![false; 32];
    for e in feed.occurrences(first * MIN_DAY, (first + dim) * MIN_DAY) {
        for d in e.start.div_euclid(MIN_DAY).max(first)..=(e.end - 1).max(e.start).div_euclid(MIN_DAY).min(first + dim - 1) {
            marks[(d - first + 1) as usize] = true;
        }
    }
    let day_events = feed.occurrences(sel * MIN_DAY, (sel + 1) * MIN_DAY);
    let line = |e: &ics::Event| {
        let when = if e.all_day { tr(K::AllDay).to_string() } else { let m = e.start.rem_euclid(MIN_DAY); format!("{:02}:{:02}", m / 60, m % 60) };
        format!("{when}  {}", short(&e.title, 34))
    };
    let agenda = match (configured, day_events.len()) {
        (false, _) => vec![tr(K::NoCalendar).to_string()],
        (true, 0) => vec![tr(K::NoEvents).to_string()],
        (true, n) if n <= AGENDA_LINES => day_events.iter().map(line).collect(),
        (true, n) => day_events.iter().take(AGENDA_LINES - 1).map(line).chain(std::iter::once(format!("+{} {}", n - (AGENDA_LINES - 1), tr(K::More)))).collect(),
    };
    CalView {
        title: format!("{} {}", i18n::month_name(ym.1, lang()), ym.0),
        rows,
        cells,
        today: day_in_month(calendar::now_local().0, ym),
        sel: day_in_month(sel, ym),
        marks,
        agenda,
        world,
    }
}

fn clock_view() -> Option<CalView> {
    let (open, ym, sel, configured, clocks) = with(|s| (s.clock_open, s.cal_ym, s.cal_sel, !s.theme.calendar.is_empty(), s.theme.clocks.clone()));
    open.then(|| calendar_view(ym, sel, configured, world::line(&clocks)))
}

// Aviso en la barra cuando una reunión empieza en <= 15 min.
fn countdown() -> Option<String> {
    use calendar::MIN_DAY;
    let (day, min) = calendar::now_local();
    let now = day * MIN_DAY + min as i64;
    let feed = ics::FEED.lock().unwrap();
    let e = feed.occurrences(now, now + 16).into_iter().find(|e| !e.all_day && e.start >= now && e.start <= now + 15)?;
    let left = e.start - now;
    Some(if left == 0 { format!("{} · {}", short(&e.title, 28), tr(K::Now)) } else { format!("{} · {left} min", short(&e.title, 28)) })
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Meter {
    Cpu,
    Ram,
    Vol,
    Bri,
}

impl Meter {
    fn name(self) -> &'static str {
        tr(match self {
            Meter::Cpu => K::Cpu,
            Meter::Ram => K::Ram,
            Meter::Vol => K::Vol,
            Meter::Bri => K::Bri,
        })
    }
}

impl Panel {
    fn shape(&self) -> PanelShape {
        PanelShape { media: self.media.is_some(), weather: self.weather.is_some(), meters: self.meters.len(), cal_rows: self.cal.as_ref().map_or(0, |c| c.rows), agenda: if self.cal.is_some() { AGENDA_LINES } else { 0 }, world: self.cal.as_ref().is_some_and(|c| !c.world.is_empty()) }
    }
}

thread_local! { static STATE: RefCell<Option<State>> = const { RefCell::new(None) }; }

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(s.borrow_mut().as_mut().expect("state")))
}

fn colorref(hex: &str) -> COLORREF {
    let (r, g, b) = rgb(hex);
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}

fn widestr(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe extern "system" fn collect_monitor(m: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    if GetMonitorInfoW(m, &mut mi).as_bool() {
        (*(data.0 as *mut Vec<RECT>)).push(mi.rcMonitor);
    }
    BOOL(1)
}

// Rectángulos de todos los monitores, el primario primero.
pub(crate) unsafe fn monitor_rects() -> Vec<RECT> {
    let mut v: Vec<RECT> = Vec::new();
    let _ = EnumDisplayMonitors(None, None, Some(collect_monitor), LPARAM(&mut v as *mut _ as isize));
    v.sort_by_key(|r| (r.left != 0 || r.top != 0, r.left));
    if v.is_empty() {
        v.push(RECT { left: 0, top: 0, right: GetSystemMetrics(SM_CXSCREEN), bottom: GetSystemMetrics(SM_CYSCREEN) });
    }
    v
}

// Monitor de la ventana en foco: ahí vive la pastilla.
unsafe fn focus_monitor() -> RECT {
    let fg = GetForegroundWindow();
    if fg.0.is_null() {
        return with(|s| s.pill_mon);
    }
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    if GetMonitorInfoW(MonitorFromWindow(fg, MONITOR_DEFAULTTOPRIMARY), &mut mi).as_bool() {
        mi.rcMonitor
    } else {
        monitor_rects()[0]
    }
}

// Escala del monitor que contiene el rectángulo: cada monitor puede tener su propio DPI.
pub(crate) unsafe fn mon_scale(r: &RECT) -> f32 {
    let m = MonitorFromPoint(POINT { x: r.left + 1, y: r.top + 1 }, MONITOR_DEFAULTTONEAREST);
    let (mut x, mut y) = (0u32, 0u32);
    if GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut x, &mut y).is_ok() && x > 0 {
        x as f32 / 96.0
    } else {
        1.0
    }
}

fn mode_label(m: komorebi::Mode) -> &'static str {
    tr(match m {
        komorebi::Mode::Tiling => K::ModeTiling,
        komorebi::Mode::Scrolling => K::ModeScrolling,
        komorebi::Mode::Floating => K::ModeFloating,
    })
}

// Firma de la disposición de monitores y sus escalas: si cambia, hay que redibujar la pastilla.
unsafe fn layout_sig() -> Vec<(i32, i32, i32, i32, u32)> {
    monitor_rects().iter().map(|r| (r.left, r.top, r.right, r.bottom, (mon_scale(r) * 100.0) as u32)).collect()
}

fn tint(base: (u8, u8, u8), over: (u8, u8, u8), amount: f32) -> (u8, u8, u8) {
    let m = |b: u8, o: u8| (b as f32 * (1.0 - amount) + o as f32 * amount) as u8;
    (m(base.0, over.0), m(base.1, over.1), m(base.2, over.2))
}

// Rectángulo redondeado antialiased sobre un fondo plano conocido (`base`).
fn fill_round(pixels: &mut [u32], stride: i32, rows: i32, r: R, rad: i32, col: (u8, u8, u8), base: (u8, u8, u8)) {
    let rad = (rad as f32).min(r.w as f32 / 2.0).min(r.h as f32 / 2.0);
    for y in r.y.max(0)..(r.y + r.h).min(rows) {
        for x in r.x.max(0)..(r.x + r.w).min(stride) {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let cx = fx.clamp(r.x as f32 + rad, (r.x + r.w) as f32 - rad);
            let cy = fy.clamp(r.y as f32 + rad, (r.y + r.h) as f32 - rad);
            let cov = (rad - ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt() + 0.5).clamp(0.0, 1.0);
            if cov > 0.0 {
                let mix = |b: u8, c: u8| (b as f32 * (1.0 - cov) + c as f32 * cov) as u32;
                pixels[(y * stride + x) as usize] = mix(base.0, col.0) << 16 | mix(base.1, col.1) << 8 | mix(base.2, col.2);
            }
        }
    }
}

unsafe fn draw_text(mem: HDC, s: &str, r: R, color: COLORREF, flags: DRAW_TEXT_FORMAT) {
    // DrawTextW con un slice vacío (puntero colgante) viola el acceso dentro de user32.
    if s.is_empty() {
        return;
    }
    let mut text: Vec<u16> = s.encode_utf16().collect();
    let mut rc = RECT { left: r.x, top: r.y, right: r.x + r.w, bottom: r.y + r.h };
    SetTextColor(mem, color);
    DrawTextW(mem, &mut text, &mut rc, flags | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS);
}

unsafe fn render_pill(pill: HWND) {
    let mon = focus_monitor();
    let scale = mon_scale(&mon);
    let theme = with(|s| {
        if (s.scale - scale).abs() > 0.01 {
            s.scale = scale;
            s.cur_w = 0.0;
            s.vel = 0.0;
            s.cur_h = 0.0;
            s.vel_h = 0.0;
        }
        s.theme.clone()
    });
    let t = GetLocalTime();
    let text: Vec<u16> = format!("{:02}:{:02}", t.wHour, t.wMinute).encode_utf16().collect();
    let bar_h = layout::bar_h(scale);
    let flip = theme.position == "bottom";
    let screen = GetDC(None);
    let mem = CreateCompatibleDC(screen);
    let family = widestr(&theme.font_family);
    let icons = widestr("Segoe MDL2 Assets");
    let mkfont = |size: i32, weight: i32, face: &[u16]| CreateFontW(-px(size, scale), 0, 0, 0, weight, 0, 0, 0, 1, 0, 0, CLEARTYPE_QUALITY.0 as u32, 0, PCWSTR(face.as_ptr()));
    let font = mkfont(theme.font_size, 600, &family);
    let small = mkfont((theme.font_size - 2).max(9), 400, &family);
    let glyphs = mkfont(14, 400, &icons);
    let old_font = SelectObject(mem, font);
    let mut ext = SIZE::default();
    let _ = GetTextExtentPoint32W(mem, &text, &mut ext);
    let (assist, assist_t) = spirit::current();
    let island = pipe::ISLAND.lock().unwrap().clone().or_else(|| (assist != spirit::Assist::Idle).then(|| pipe::Island { title: assist.label().into(), ..Default::default() }))
        .or_else(|| with(|s| s.countdown.clone()).map(|title| pipe::Island { title, ..Default::default() }));
    let label: Vec<u16> = island.as_ref().map(|i| if i.detail.is_empty() { i.title.clone() } else { format!("{}  {}", i.title, i.detail) }).unwrap_or_default().encode_utf16().collect();
    let mut lext = SIZE::default();
    if !label.is_empty() {
        let _ = GetTextExtentPoint32W(mem, &label, &mut lext);
    }
    let focused_ws = komorebi::CURRENT.lock().unwrap().clone().unwrap_or_default();
    let dots: Vec<(usize, bool)> = focused_ws.occupied.iter().enumerate().filter(|(i, o)| **o || *i == focused_ws.focused).map(|(i, _)| (i, i == focused_ws.focused)).collect();
    let orb = assist != spirit::Assist::Idle && theme.pet;
    let progress = island.as_ref().and_then(|i| i.progress);
    let live_panel = with(|s| s.panel.clone());
    let temp = extras::WEATHER.lock().unwrap().as_ref().map(|w| format!("{}°", w.temp)).unwrap_or_default();
    let temp_u16: Vec<u16> = temp.encode_utf16().collect();
    let mut text_ext = SIZE::default();
    let temp_w = if temp.is_empty() { 0 } else { let _ = GetTextExtentPoint32W(mem, &temp_u16, &mut text_ext); text_ext.cx };
    let need_raw = layout::bar_width(scale, orb, temp_w, &dots, lext.cx, ext.cx) + if live_panel.is_some() { layout::pin_w(scale) } else { 0 };
    let widest = need_raw.max(if live_panel.is_some() { px(layout::PANEL_W_DIP, scale) } else { 0 });
    let need = widest.min(px(PILL_MAX_W, scale)).min((mon.right - mon.left) * 3 / 4);
    let base = layout::bar_width(scale, orb, temp_w, &dots, 0, ext.cx);
    let (width, height, animating, strip, label, panel) = with(|s| {
        if live_panel.is_some() != s.was_open {
            s.was_open = live_panel.is_some();
            if s.was_open {
                s.fade = 0.0;
            }
        }
        if let Some(p) = &live_panel {
            s.panel_shown = Some(p.clone());
        }
        let extra = match (&live_panel, progress) {
            (Some(p), _) => layout::panel_height(scale, p.shape()),
            (None, Some(_)) => layout::progress_band(scale),
            _ => 0,
        };
        s.target_w = need as f32;
        s.target_h = (bar_h + extra) as f32;
        if s.cur_w == 0.0 {
            s.cur_w = s.target_w;
            s.cur_h = s.target_h;
        }
        let width = (s.cur_w.round() as i32).max(base);
        let height = (s.cur_h.round() as i32).max(bar_h);
        // Al colapsar se sigue mostrando la última etiqueta y el último panel, recortados, hasta que la caja llega a su tamaño final.
        if !label.is_empty() {
            s.last_label = label;
        } else if width <= base {
            s.last_label.clear();
        }
        if live_panel.is_none() && height as f32 <= s.target_h + 1.0 {
            s.panel_shown = None;
        }
        (width, height, (s.cur_w - s.target_w).abs() > 0.5 || (s.cur_h - s.target_h).abs() > 0.5 || s.fade < 1.0, s.strip, s.last_label.clone(), s.panel_shown.clone())
    });
    if animating {
        SetTimer(strip, TIMER_ANIM, 8, None);
    } else {
        let name = widestr(&format!("Winarchy {} {}", String::from_utf16_lossy(&text), String::from_utf16_lossy(&label)));
        let _ = SetWindowTextW(pill, PCWSTR(name.as_ptr()));
    }
    if lext.cx == 0 && !label.is_empty() {
        let _ = GetTextExtentPoint32W(mem, &label, &mut lext);
    }
    let lay = Layout::new(&Inputs { scale, width, height, flip, clock_w: ext.cx, orb, temp_w, dots: &dots, panel: panel.as_ref().map(Panel::shape) });
    let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: width, biHeight: -height, biPlanes: 1, biBitCount: 32, ..Default::default() }, ..Default::default() };
    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let dib = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
    let old_bmp = SelectObject(mem, dib);
    let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (width * height) as usize);

    let bg = rgb(&theme.background);
    let (fg, accent, muted) = (rgb(&theme.foreground), rgb(&theme.accent), rgb(&theme.muted));
    let rad = layout::corner(scale, height) as f32;
    let mask: Vec<f32> = (0..height * width).map(|i| layout::notch_coverage((i % width) as f32 + 0.5, (i / width) as f32 + 0.5, width as f32, height as f32, rad, flip)).collect();
    let premul = |c: u8, a: f32| (c as f32 * a) as u32;
    for (p, a) in pixels.iter_mut().zip(&mask) {
        *p = ((*a * 255.0) as u32) << 24 | premul(bg.0, *a) << 16 | premul(bg.1, *a) << 8 | premul(bg.2, *a);
    }
    for d in &lay.dots {
        fill_round(pixels, width, height, lay.place(d.r), d.r.h / 2, if d.focused { accent } else { muted }, bg);
    }
    let animate = animations_enabled();
    if assist != spirit::Assist::Idle {
        if theme.pet {
            let (mut cur, mut r) = (POINT::default(), RECT::default());
            let o = lay.place(lay.orb);
            let look = if animate && GetCursorPos(&mut cur).is_ok() && GetWindowRect(pill, &mut r).is_ok() {
                let reach = 220.0 * scale;
                let (cx, cy) = ((r.left + o.x + o.w / 2) as f32, (r.top + o.y + o.h / 2) as f32);
                (((cur.x as f32 - cx) / reach).clamp(-1.0, 1.0), ((cur.y as f32 - cy) / reach).clamp(-1.0, 1.0))
            } else {
                (0.0, 0.0)
            };
            spirit::paint(&mut pixels[(o.y * width) as usize..], width, o.x, o.h, assist, if animate { assist_t } else { 0.0 }, look, accent);
        }
        SetTimer(strip, TIMER_SPIRIT, if animate { 33 } else { 250 }, None);
    } else {
        let _ = KillTimer(strip, TIMER_SPIRIT);
    }
    if let Some(p) = progress {
        let r = lay.place(lay.progress);
        fill_round(pixels, width, height, R { w: (r.w as f64 * p) as i32, ..r }, r.h / 2, accent, bg);
    }
    SetBkMode(mem, TRANSPARENT);
    let (fg_c, muted_c, bg_c) = (colorref(&theme.foreground), colorref(&theme.muted), colorref(&theme.background));
    if temp_w > 0 {
        draw_text(mem, &temp, lay.place(lay.temp), muted_c, DT_CENTER | DT_VCENTER);
    }
    if !label.is_empty() && lay.label.w > 0 {
        let clip = lay.place(lay.label);
        if need_raw > need && width >= need {
            draw_text(mem, &String::from_utf16_lossy(&label), clip, fg_c, DT_VCENTER);
        } else {
            let (mut x, y) = (clip.x, clip.y + (bar_h - lext.cy) / 2);
            let clip_rc = RECT { left: clip.x, top: clip.y, right: clip.x + clip.w, bottom: clip.y + clip.h };
            for (seg, dim) in segments(&String::from_utf16_lossy(&label)) {
                let w: Vec<u16> = seg.encode_utf16().collect();
                SetTextColor(mem, if dim { muted_c } else { fg_c });
                let _ = ExtTextOutW(mem, x, y, ETO_CLIPPED, Some(&clip_rc), PCWSTR(w.as_ptr()), w.len() as u32, None);
                let mut e = SIZE::default();
                let _ = GetTextExtentPoint32W(mem, &w, &mut e);
                x += e.cx;
            }
        }
    }
    if lay.pin.w > 0 {
        SelectObject(mem, glyphs);
        draw_text(mem, "\u{E718}", lay.place(lay.pin), if with(|s| s.pinned) { colorref(&theme.accent) } else { muted_c }, DT_CENTER | DT_VCENTER);
    }
    SelectObject(mem, font);
    SetTextColor(mem, fg_c);
    let clock = lay.place(lay.clock);
    let _ = TextOutW(mem, clock.x, clock.y + (bar_h - ext.cy) / 2, &text);

    if let (Some(p), Some(pr)) = (&panel, &lay.panel) {
        if let Some(m) = &p.media {
            draw_text(mem, &m.title, lay.place(pr.title), fg_c, DT_BOTTOM);
            SelectObject(mem, small);
            draw_text(mem, &m.artist, lay.place(pr.artist), muted_c, DT_TOP);
            SelectObject(mem, glyphs);
            for (r, g) in [(pr.prev, "\u{E892}"), (pr.toggle, if m.playing { "\u{E769}" } else { "\u{E768}" }), (pr.next, "\u{E893}")] {
                draw_text(mem, g, lay.place(r), fg_c, DT_CENTER | DT_VCENTER);
            }
        }
        if let Some(w) = &p.weather {
            SelectObject(mem, font);
            draw_text(mem, &w.title(lang()), lay.place(pr.weather_title), fg_c, DT_BOTTOM);
            draw_text(mem, &w.range(), lay.place(pr.weather_range), fg_c, DT_BOTTOM | DT_RIGHT);
            SelectObject(mem, small);
            let [d1, d2] = w.detail(lang());
            draw_text(mem, &d1, lay.place(pr.weather_detail), muted_c, DT_TOP);
            draw_text(mem, &d2, lay.place(pr.weather_detail2), muted_c, DT_TOP);
        }
        SelectObject(mem, small);
        let track = tint(bg, fg, 0.12);
        for (m, (name, pct, value)) in pr.meters.iter().zip(&p.meters) {
            draw_text(mem, name.name(), lay.place(m.label), muted_c, DT_VCENTER);
            draw_text(mem, value, lay.place(m.value), fg_c, DT_VCENTER | DT_RIGHT);
            let t = lay.place(m.track);
            fill_round(pixels, width, height, t, t.h / 2, track, bg);
            fill_round(pixels, width, height, R { w: t.w * (*pct).min(100) as i32 / 100, ..t }, t.h / 2, accent, track);
        }
        if let Some(cv) = &p.cal {
            SelectObject(mem, font);
            draw_text(mem, &cv.title, lay.place(pr.cal_title), fg_c, DT_VCENTER);
            SelectObject(mem, glyphs);
            draw_text(mem, "\u{E76B}", lay.place(pr.cal_prev), muted_c, DT_CENTER | DT_VCENTER);
            draw_text(mem, "\u{E76C}", lay.place(pr.cal_next), muted_c, DT_CENTER | DT_VCENTER);
            SelectObject(mem, small);
            for (i, r) in pr.cal_heads.iter().enumerate() {
                draw_text(mem, i18n::weekday_initial(i, lang()), lay.place(*r), muted_c, DT_CENTER | DT_VCENTER);
            }
            let (sel_bg, dot) = (tint(bg, fg, 0.14), px(3, scale));
            for (r, day) in pr.cal_cells.iter().zip(&cv.cells) {
                let Some(day) = *day else { continue };
                let r = lay.place(*r);
                let d = (r.h - 2).min(r.w - 2);
                let disc = R { x: r.x + (r.w - d) / 2, y: r.y + (r.h - d) / 2, w: d, h: d };
                let is_today = cv.today == Some(day);
                if is_today {
                    fill_round(pixels, width, height, disc, d / 2, accent, bg);
                } else if cv.sel == Some(day) {
                    fill_round(pixels, width, height, disc, d / 2, sel_bg, bg);
                }
                draw_text(mem, &day.to_string(), r, if is_today { bg_c } else { fg_c }, DT_CENTER | DT_VCENTER);
                if cv.marks[day as usize] {
                    let dot_r = R { x: r.x + (r.w - dot) / 2, y: disc.y + disc.h - dot - 1, w: dot, h: dot };
                    fill_round(pixels, width, height, dot_r, dot / 2, if is_today { bg } else { accent }, if is_today { accent } else if cv.sel == Some(day) { sel_bg } else { bg });
                }
            }
            for (line, r) in cv.agenda.iter().zip(&pr.agenda) {
                draw_text(mem, line, lay.place(*r), fg_c, DT_VCENTER);
            }
            draw_text(mem, &cv.world, lay.place(pr.world), muted_c, DT_VCENTER);
        }
        let chip_bg = tint(bg, fg, 0.07);
        for c in &pr.chips {
            let r = lay.place(c.r);
            fill_round(pixels, width, height, r, px(7, scale), if c.focused { accent } else { chip_bg }, bg);
            draw_text(mem, &(c.ws + 1).to_string(), r, if c.focused { bg_c } else { muted_c }, DT_CENTER | DT_VCENTER);
        }
        let mode_r = lay.place(pr.mode);
        fill_round(pixels, width, height, mode_r, px(7, scale), chip_bg, bg);
        let mode = komorebi::CURRENT.lock().unwrap().as_ref().map(|w| w.mode).unwrap_or_default();
        draw_text(mem, mode_label(mode), mode_r, fg_c, DT_CENTER | DT_VCENTER);
        draw_text(mem, &p.notes, lay.place(pr.notes), muted_c, DT_VCENTER | DT_RIGHT);
    }
    let fade = layout::ease(with(|s| s.fade));
    if fade < 1.0 && panel.is_some() {
        let (y0, y1) = if flip { (0, height - bar_h) } else { (bar_h, height) };
        for y in y0.max(0)..y1.max(0) {
            for x in 0..width {
                let i = (y * width + x) as usize;
                if mask[i] >= 1.0 {
                    pixels[i] = layout::blend_toward(pixels[i], bg, fade);
                }
            }
        }
    }
    // GDI no escribe alfa: dentro de la isla el texto debe quedar opaco.
    for (p, a) in pixels.iter_mut().zip(&mask) {
        if *a >= 1.0 {
            *p |= 0xFF00_0000;
        }
    }

    with(|s| {
        s.pill_mon = mon;
        s.layout = lay;
    });
    let y = if flip { mon.bottom - height } else { mon.top };
    let pos = POINT { x: mon.left + (mon.right - mon.left - width) / 2, y };
    let size = SIZE { cx: width, cy: height };
    let src = POINT::default();
    let bf = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, SourceConstantAlpha: (theme.opacity.clamp(0.0, 1.0) * 255.0) as u8, AlphaFormat: AC_SRC_ALPHA as u8, ..Default::default() };
    let _ = UpdateLayeredWindow(pill, None, Some(&pos), Some(&size), mem, Some(&src), COLORREF(0), Some(&bf), ULW_ALPHA);

    SelectObject(mem, old_bmp);
    SelectObject(mem, old_font);
    let _ = DeleteObject(dib);
    for f in [font, small, glyphs] {
        let _ = DeleteObject(f);
    }
    let _ = DeleteDC(mem);
    ReleaseDC(None, screen);
}

// Resorte críticamente amortiguado sobre el ancho y el alto de la isla; dt real para no depender de la resolución del timer.
// Solución exacta: estable para cualquier dt, aunque el hilo se atrase. Devuelve si ya asentó.
fn spring(cur: &mut f32, vel: &mut f32, target: f32, dt: f32) -> bool {
    const OMEGA: f32 = 30.0;
    let (d, e) = (*cur - target, (-OMEGA * dt).exp());
    let k = *vel + OMEGA * d;
    *cur = target + (d + k * dt) * e;
    *vel = (*vel - OMEGA * k * dt) * e;
    if (*cur - target).abs() < 0.5 && vel.abs() < 1.0 {
        *cur = target;
        *vel = 0.0;
        return true;
    }
    false
}

unsafe fn spring_step(strip: HWND) {
    let (pill, settled) = with(|s| {
        let now = std::time::Instant::now();
        let dt = s.anim_last.map(|t| (now - t).as_secs_f32()).unwrap_or(0.008).min(0.05);
        s.anim_last = Some(now);
        let w = spring(&mut s.cur_w, &mut s.vel, s.target_w, dt);
        let h = spring(&mut s.cur_h, &mut s.vel_h, s.target_h, dt);
        s.fade = (s.fade + dt / FADE_SECS).min(1.0);
        let settled = w && h && s.fade >= 1.0;
        if settled {
            s.anim_last = None;
        }
        (s.pill, settled)
    });
    if settled {
        let _ = KillTimer(strip, TIMER_ANIM);
    }
    render_pill(pill);
}

// El cursor está sobre la pastilla: más fiable que esperar WM_MOUSELEAVE, que se pierde si la ventana cambia de tamaño o monitor.
unsafe fn hovered(pill: HWND) -> bool {
    let (mut p, mut r) = (POINT::default(), RECT::default());
    GetCursorPos(&mut p).is_ok() && GetWindowRect(pill, &mut r).is_ok() && PtInRect(&r, p).as_bool()
}

// "Reducir animaciones" de Windows: el espíritu queda estático.
unsafe fn animations_enabled() -> bool {
    let mut on = BOOL(1);
    let _ = SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, Some(&mut on as *mut _ as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
    on.as_bool()
}

unsafe fn fullscreen_busy() -> bool {
    !desktop::is_host_window(GetForegroundWindow()) && matches!(SHQueryUserNotificationState(), Ok(s) if s == QUNS_BUSY || s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_PRESENTATION_MODE)
}

// Parte la etiqueta en palabras; las claves en mayúsculas al inicio de un ítem ("CPU", "VOL") y los separadores van atenuados.
fn segments(label: &str) -> Vec<(String, bool)> {
    let mut item_start = true;
    label
        .split_inclusive(' ')
        .map(|w| {
            let t = w.trim_end();
            let dim = t == "·" || (item_start && t.len() >= 3 && t.chars().all(|c| c.is_ascii_uppercase()));
            item_start = t.is_empty() || t == "·";
            (w.to_string(), dim)
        })
        .collect()
}

// Mientras la isla está expandida refresca las métricas cada tick; al vencer, la limpia. Devuelve si hay que redibujar.
// Las llamadas lentas (COM, WinRT) corren fuera del borrow del estado: pueden bombear mensajes y reentrar al wndproc.
fn expand_step() -> bool {
    let now = std::time::Instant::now();
    let over = unsafe { hovered(with(|s| s.pill)) };
    let until = with(|s| {
        if (over || s.pinned) && s.expand_until.is_some() {
            s.expand_until = Some(now + std::time::Duration::from_secs(EXPAND_SECS));
        }
        s.expand_until
    });
    if !until.is_some_and(|t| t > now) {
        let vol = system::snapshot().volume;
        let early = with(|s| {
            let changed = s.last_vol.is_some() && vol.is_some() && vol != s.last_vol;
            s.last_vol = vol;
            if let (true, Some((v, muted))) = (changed, vol) {
                s.osd_until = Some(now + std::time::Duration::from_secs(2));
                *pipe::ISLAND.lock().unwrap() = Some(pipe::Island { title: if muted { tr(K::MuteOsd).into() } else { tr(K::Vol).into() }, detail: format!("{v}%"), progress: Some(v as f64 / 100.0) });
                return Some(true);
            }
            match s.osd_until {
                Some(t) if t > now => Some(false),
                Some(_) => {
                    s.osd_until = None;
                    *pipe::ISLAND.lock().unwrap() = None;
                    Some(true)
                }
                None => None,
            }
        });
        if let Some(redraw) = early {
            return redraw;
        }
    }
    match until {
        Some(t) if t > now => {
            if let Some(cal) = clock_view() {
                with(|s| s.panel = Some(Panel { media: None, weather: None, meters: Vec::new(), notes: String::new(), cal: Some(cal) }));
                return true;
            }
            let (mut sampler, theme_path) = with(|s| (std::mem::take(&mut s.sampler), s.theme_path.clone()));
            let cpu = sampler.cpu().map(|c| c.round() as u32);
            let ram = metrics::ram_percent();
            let mut meters = Vec::new();
            let mut meter = |kind: Meter, v: Option<u32>| meters.extend(v.map(|v| (kind, v, format!("{v}%"))));
            meter(Meter::Cpu, cpu);
            meter(Meter::Ram, ram);
            let snap = system::snapshot();
            if let Some((v, muted)) = snap.volume {
                meters.push((Meter::Vol, if muted { 0 } else { v }, if muted { tr(K::MuteValue).into() } else { format!("{v}%") }));
            }
            brightness::refresh_if_stale();
            if let Some(b) = brightness::level() {
                meters.push((Meter::Bri, b, format!("{b}%")));
            }
            // solo lo que cambia algo; el clima va al final.
            let mut notes = Vec::new();
            if let Some(g) = sampler.gpu().filter(|g| *g > 50.0) {
                notes.push(format!("{} {g:.0}%", tr(K::Gpu)));
            }
            if let Some((b, ac)) = snap.battery.filter(|(b, ac)| !ac || *b < 20) {
                notes.push(format!("{} {b}%{}", tr(K::Battery), if ac { "+" } else { "" }));
            }
            if let Some(n) = snap.notifications.filter(|n| *n > 0) {
                notes.push(format!("{} {n}", tr(K::Notifications)));
            }
            if snap.online == Some(false) {
                notes.push(tr(K::NetOff).into());
            }
            let state_dir = theme_path.ancestors().nth(3).map(|r| r.join("state")).unwrap_or_default();
            notes.extend(metrics::flags(&state_dir).into_iter().map(|k| tr(k).to_string()));
            let media = extras::media();
            let weather = extras::WEATHER.lock().unwrap().clone();
            with(|s| {
                s.sampler = sampler;
                s.panel = Some(Panel { media, weather, meters, notes: notes.join("  ·  "), cal: None });
            });
            true
        }
        Some(_) => {
            with(|s| {
                s.expand_until = None;
                s.panel = None;
            });
            true
        }
        None => false,
    }
}

thread_local! { static IN_TICK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

unsafe fn tick(_strip: HWND) {
    if IN_TICK.with(|t| t.replace(true)) {
        return;
    }
    tick_inner();
    IN_TICK.with(|t| t.set(false));
}

unsafe fn tick_inner() {
    desktop::tick();
    let (pill, hidden, last) = with(|s| (s.pill, s.hidden, s.last_minute));
    let minute_now = GetLocalTime().wMinute;
    if with(|s| s.countdown_minute != minute_now) {
        let countdown = countdown();
        with(|s| {
            s.countdown = countdown;
            s.countdown_minute = minute_now;
        });
    }
    let busy = fullscreen_busy();
    if busy != hidden {
        let _ = ShowWindow(pill, if busy { SW_HIDE } else { SW_SHOWNOACTIVATE });
        with(|s| s.hidden = busy);
    }
    if !busy {
        // otras ventanas topmost (launcher, overlays) nos pasan por encima: se reafirma el orden.
        let _ = SetWindowPos(pill, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
    let reloaded = with(|s| {
        let m = Theme::stamp(&s.theme_path);
        if m != s.theme_mtime {
            s.theme_mtime = m;
            if let Some(t) = Theme::load(&s.theme_path) {
                let was_dock = s.theme.dock;
                s.theme = t;
                return Some((was_dock, s.theme.dock, s.theme.clone(), s.state_dir.clone()));
            }
        }
        None
    });
    let sig = layout_sig();
    if with(|s| s.layout_sig != sig) {
        with(|s| s.layout_sig = sig);
        render_pill(pill);
    }
    let (pill, stale) = with(|s| (s.pill, s.tracking));
    if stale && !hovered(pill) {
        with(|s| s.tracking = false);
        let _ = PostMessageW(with(|s| s.strip), WM_COLLAPSE, WPARAM(0), LPARAM(0));
    }
    let expanding = expand_step();
    let minute = GetLocalTime().wMinute;
    let fm = focus_monitor();
    let moved = with(|s| (fm.left, fm.top) != (s.pill_mon.left, s.pill_mon.top));
    if let Some((was_dock, has_dock, theme, state_dir)) = &reloaded {
        desktop::set_fallback_color(&theme.background);
        if with(|s| s.session_shell) { tray_notifications::update_theme(theme.clone()); }
        match dock_change(*was_dock, *has_dock) {
            DockChange::Enable => { let _ = dock::init(theme, state_dir); }
            DockChange::Disable => dock::shutdown(),
            DockChange::Update => dock::set_theme(theme),
            DockChange::None => {}
        }
    }
    if !busy && (reloaded.is_some() || expanding || moved || minute != last) {
        with(|s| s.last_minute = minute);
        render_pill(pill);
    }
}

unsafe extern "system" fn strip_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_TIMER if wp.0 == TIMER_SPIRIT => {
            render_pill(with(|s| s.pill));
            LRESULT(0)
        }
        WM_TIMER if wp.0 == TIMER_FLASH => {
            let _ = KillTimer(h, TIMER_FLASH);
            let title = with(|s| s.flash_title.take());
            let mut island = pipe::ISLAND.lock().unwrap();
            if title.is_some() && island.as_ref().map(|i| &i.title) == title.as_ref() {
                *island = None;
            }
            drop(island);
            render_pill(with(|s| s.pill));
            LRESULT(0)
        }
        WM_TIMER if wp.0 == TIMER_HOVER => {
            let _ = KillTimer(h, TIMER_HOVER);
            if hovered(with(|s| s.pill)) {
                let _ = PostMessageW(h, WM_EXPAND, WPARAM(0), LPARAM(0));
            }
            LRESULT(0)
        }
        WM_TIMER if wp.0 == TIMER_ANIM => {
            spring_step(h);
            LRESULT(0)
        }
        WM_TIMER => {
            tick(h);
            LRESULT(0)
        }
        WM_BRIGHTNESS => {
            tick(h);
            LRESULT(0)
        }
        WM_CMD_RELOAD => {
            with(|s| s.theme_mtime = None);
            tick(h);
            LRESULT(0)
        }
        WM_CMD_DOCK_PIN => {
            dock::launch_pin(wp.0);
            LRESULT(0)
        }
        WM_CMD_DOCK_FOCUS => {
            dock::focus_keyboard();
            LRESULT(0)
        }
        WM_CMD_TRAY => {
            dock::show_tray();
            LRESULT(0)
        }
        WM_CMD_TRAY_DISMISS => {
            dock::dismiss_tray();
            LRESULT(0)
        }
        WM_CMD_TOGGLE_DESKTOP => {
            cluster::toggle_desktop();
            LRESULT(0)
        }
        WM_EXPAND => {
            with(|s| s.expand_until = Some(std::time::Instant::now() + std::time::Duration::from_secs(EXPAND_SECS)));
            tick(h);
            LRESULT(0)
        }
        WM_COLLAPSE => {
            let pill = with(|s| {
                if !s.pinned {
                    s.expand_until = None;
                    s.panel = None;
                    s.clock_open = false;
                }
                s.pill
            });
            render_pill(pill);
            LRESULT(0)
        }
        WM_WORKSPACES => {
            let (cur, layout, mode) = komorebi::CURRENT.lock().unwrap().as_ref().map(|w| (Some(w.focused), w.layout.clone(), w.mode)).unwrap_or_default();
            let (ws_changed, layout_changed, mode_changed) = with(|s| {
                let ws = workspace_changed(s.last_ws, cur);
                s.last_ws = cur.or(s.last_ws);
                let lay = !ws && !layout.is_empty() && !s.last_layout.is_empty() && s.last_layout != layout;
                if !layout.is_empty() {
                    s.last_layout = layout.clone();
                }
                let md = !ws && s.last_mode.is_some_and(|m| m != mode);
                s.last_mode = Some(mode);
                (ws, lay, md)
            });
            let title = match (ws_changed, mode_changed, layout_changed, cur) {
                (true, _, _, Some(n)) => Some(format!("{} {}", tr(K::Workspace), n + 1)),
                (_, true, _, _) => Some(mode_label(mode).to_string()),
                (_, _, true, _) => Some(format!("{} {layout}", tr(K::Layout))),
                _ => None,
            };
            if let Some(title) = title {
                let mut island = pipe::ISLAND.lock().unwrap();
                if island.is_none() {
                    *island = Some(pipe::Island { title: title.clone(), ..Default::default() });
                    with(|s| s.flash_title = Some(title));
                    SetTimer(h, TIMER_FLASH, FLASH_MS, None);
                }
            }
            render_pill(with(|s| s.pill));
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            if with(|s| s.strip) == h {
                with(|s| s.layout_sig = layout_sig());
                render_pill(with(|s| s.pill));
            }
            LRESULT(0)
        }
        WM_CMD_QUIT => {
            if with(|s| s.session_shell) {
                request_explorer_return(h);
            } else {
                let _ = DestroyWindow(h);
            }
            LRESULT(0)
        }
        WM_CMD_RESTART => {
            match restart_action(with(|s| s.session_shell)) {
                RestartAction::Reload => {
                    with(|s| s.theme_mtime = None);
                    tick(h);
                }
                RestartAction::Exit => { let _ = DestroyWindow(h); }
            }
            LRESULT(0)
        }
        WM_CMD_RETURN => {
            if with(|s| s.session_shell) {
                request_explorer_return(h);
            } else {
                let _ = DestroyWindow(h);
            }
            LRESULT(0)
        }
        WM_CMD_RETURN_READY => {
            RETURN_TO_EXPLORER_PENDING.store(false, std::sync::atomic::Ordering::Release);
            let _ = DestroyWindow(h);
            LRESULT(0)
        }
        WM_CMD_RETURN_FAILED => {
            RETURN_TO_EXPLORER_PENDING.store(false, std::sync::atomic::Ordering::Release);
            LRESULT(0)
        }
        WM_DESTROY => {
            if with(|s| s.strip) == h {
                desktop::shutdown();
                dock::restore_native_taskbar();
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

pub(crate) unsafe fn shell_open(target: &str) {
    let t = widestr(target);
    let _ = ShellExecuteW(None, w!("open"), PCWSTR(t.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
}

fn request_explorer_return(hwnd: HWND) {
    use std::sync::atomic::Ordering;
    if RETURN_TO_EXPLORER_PENDING.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
        return;
    }
    let target = hwnd.0 as isize;
    if std::thread::Builder::new().name("explorer-return".into()).spawn(move || {
        let message = if dock::start_system_explorer() { WM_CMD_RETURN_READY } else { WM_CMD_RETURN_FAILED };
        let posted = unsafe { PostMessageW(HWND(target as *mut _), message, WPARAM(0), LPARAM(0)).is_ok() };
        if message == WM_CMD_RETURN_FAILED || !posted {
            RETURN_TO_EXPLORER_PENDING.store(false, Ordering::Release);
        }
    }).is_err() {
        RETURN_TO_EXPLORER_PENDING.store(false, Ordering::Release);
    }
}

unsafe extern "system" fn pill_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let hit = || with(|s| s.layout.hit((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32));
    match m {
        WM_MOUSEMOVE => {
            let (strip, expanded, track) = with(|s| {
                let expanded = s.expand_until.is_some();
                if expanded {
                    s.expand_until = Some(std::time::Instant::now() + std::time::Duration::from_secs(EXPAND_SECS));
                }
                (s.strip, expanded, std::mem::replace(&mut s.tracking, true))
            });
            if !track {
                let mut e = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: h, dwHoverTime: 0 };
                let _ = TrackMouseEvent(&mut e);
                if !expanded {
                    // Un roce al borde de la pantalla no abre la isla: hay que quedarse HOVER_MS.
                    SetTimer(strip, TIMER_HOVER, HOVER_MS, None);
                }
            }
            return LRESULT(0);
        }
        WM_MOUSELEAVE => {
            let strip = with(|s| {
                s.tracking = false;
                s.strip
            });
            let _ = KillTimer(strip, TIMER_HOVER);
            let _ = PostMessageW(strip, WM_COLLAPSE, WPARAM(0), LPARAM(0));
            return LRESULT(0);
        }
        WM_LBUTTONUP => {
            match hit() {
                Some(Hit::Workspace(i)) => komorebi::focus_workspace(i),
                Some(Hit::Prev) => extras::media_control(extras::MediaCmd::Prev),
                Some(Hit::Toggle) => extras::media_control(extras::MediaCmd::Toggle),
                Some(Hit::Next) => extras::media_control(extras::MediaCmd::Next),
                Some(Hit::Pin) => {
                    with(|s| {
                        s.pinned = !s.pinned;
                        s.expand_until = Some(std::time::Instant::now() + std::time::Duration::from_secs(EXPAND_SECS));
                    });
                    render_pill(h);
                }
                Some(Hit::Temp) => {
                    if let Some(w) = extras::WEATHER.lock().unwrap().as_ref() {
                        shell_open(&w.url());
                    }
                }
                Some(Hit::Clock) => toggle_clock(),
                Some(Hit::CalPrev | Hit::CalNext) => {
                    let delta = if matches!(hit(), Some(Hit::CalPrev)) { -1 } else { 1 };
                    with(|s| {
                        s.cal_ym = calendar::shift_month(s.cal_ym.0, s.cal_ym.1, delta);
                        s.cal_sel = default_sel(s.cal_ym);
                    });
                    tick(with(|s| s.strip));
                }
                Some(Hit::CalCell(n)) => {
                    let day = with(|s| s.panel.as_ref().and_then(|p| p.cal.as_ref()).and_then(|c| c.cells.get(n).copied().flatten()));
                    if let Some(d) = day {
                        with(|s| s.cal_sel = calendar::days_from_civil(s.cal_ym.0, s.cal_ym.1, d));
                        tick(with(|s| s.strip));
                    }
                }
                Some(Hit::Weather) => {
                    if let Some(url) = with(|s| s.panel.as_ref().and_then(|p| p.weather.as_ref()).map(|w| w.url())) {
                        shell_open(&url);
                    }
                }
                Some(Hit::Meter(i, fraction)) => {
                    match (with(|s| s.panel.as_ref().and_then(|p| p.meters.get(i)).map(|m| m.0)), fraction) {
                        (Some(Meter::Vol), Some(f)) => {
                            let _ = metrics::set_volume((f * 100.0).round() as u32);
                            tick(with(|s| s.strip));
                        }
                        (Some(Meter::Vol | Meter::Bri), None) => {}
                        (Some(Meter::Bri), Some(f)) => brightness::set((f * 100.0).round() as u32),
                        (Some(Meter::Cpu | Meter::Ram), _) => shell_open("taskmgr.exe"),
                        (None, _) => {}
                    }
                }
                Some(Hit::Mode) => komorebi::cycle_mode(),
                Some(Hit::Surface) | None => {}
            }
            return LRESULT(0);
        }
        WM_MBUTTONUP => {
            let _ = metrics::toggle_mute();
            tick(with(|s| s.strip));
            return LRESULT(0);
        }
        WM_MOUSEWHEEL => {
            let delta = (wp.0 >> 16) as i16 as i32;
            let mut r = RECT::default();
            // La rueda trae coordenadas de pantalla: sobre el medidor BRI mueve el brillo, en cualquier otro lado el volumen.
            let over_brightness = GetWindowRect(h, &mut r).is_ok()
                && with(|s| {
                    let (x, y) = ((lp.0 & 0xFFFF) as i16 as i32 - r.left, ((lp.0 >> 16) & 0xFFFF) as i16 as i32 - r.top);
                    match (s.layout.hit(x, y), s.panel.as_ref()) {
                        (Some(Hit::Meter(i, _)), Some(p)) => p.meters.get(i).is_some_and(|m| m.0 == Meter::Bri),
                        _ => false,
                    }
                });
            if over_brightness {
                brightness::adjust(delta.signum() * (delta.abs() / 120).max(1));
            } else {
                let _ = metrics::adjust_volume(delta.signum() * (delta.abs() / 60).max(1));
                tick(with(|s| s.strip));
            }
            return LRESULT(0);
        }
        WM_SETCURSOR => {
            let (mut p, mut r) = (POINT::default(), RECT::default());
            if GetCursorPos(&mut p).is_ok() && GetWindowRect(h, &mut r).is_ok() {
                let lp = LPARAM((((p.y - r.top) as isize) << 16) | ((p.x - r.left) as isize & 0xFFFF));
                let over_control = with(|s| s.layout.hit((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32));
                let cursor = LoadCursorW(None, if matches!(over_control, Some(Hit::Surface) | None) { IDC_ARROW } else { IDC_HAND });
                SetCursor(cursor.unwrap_or_default());
                return LRESULT(1);
            }
        }
        _ => {}
    }
    DefWindowProcW(h, m, wp, lp)
}

// El destello es solo para un cambio real: el primer estado leído no cuenta.
fn workspace_changed(prev: Option<usize>, cur: Option<usize>) -> bool {
    matches!((prev, cur), (Some(p), Some(c)) if p != c)
}


fn shell_status_path(root: &Path, session_id: u32) -> PathBuf {
    root.join("state").join(format!("shell-session-{session_id}"))
}

fn shell_session_marker(root: &Path, session_id: u32) -> PathBuf {
    root.join("state").join(format!("session-shell-{session_id}"))
}

fn write_shell_status(path: &Path, pid: u32, session_shell: bool, identity: &str) -> std::io::Result<()> {
    let temp = path.with_extension(format!("tmp-{pid}"));
    std::fs::write(&temp, format!("{pid}|{}|{identity}", u8::from(session_shell)))?;
    if let Err(error) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(temp);
        return Err(error);
    }
    Ok(())
}

fn write_shell_marker(path: &Path, pid: u32, identity: &str) -> std::io::Result<()> {
    let temp = path.with_extension(format!("tmp-{pid}"));
    std::fs::write(&temp, identity)?;
    if let Err(error) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(temp);
        return Err(error);
    }
    Ok(())
}

fn session_shell_mode(marker: &Path, identity: &str) -> bool {
    std::fs::read_to_string(marker).is_ok_and(|value| value.trim() == identity)
}

fn update_shell_session_state(root: &Path, session_id: u32, pid: u32, session_shell: bool, identity: &str) -> std::io::Result<(PathBuf, PathBuf)> {
    let status = shell_status_path(root, session_id);
    let marker = shell_session_marker(root, session_id);
    std::fs::create_dir_all(status.parent().unwrap_or(root))?;
    if session_shell { write_shell_marker(&marker, pid, identity)?; }
    write_shell_status(&status, pid, session_shell, identity)?;
    Ok((status, marker))
}

#[link(name = "kernel32")]
extern "system" {
    fn ProcessIdToSessionId(process_id: u32, session_id: *mut u32) -> BOOL;
}

fn current_session_id() -> Option<u32> {
    let mut session_id = 0;
    unsafe { ProcessIdToSessionId(windows::Win32::System::Threading::GetCurrentProcessId(), &mut session_id).as_bool().then_some(session_id) }
}
fn current_logon_identity() -> Option<String> {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Luid { low: u32, high: i32 }
    #[repr(C)]
    struct TokenStatistics {
        token_id: Luid, authentication_id: Luid, expiration_time: i64, token_type: u32,
        impersonation_level: u32, dynamic_charged: u32, dynamic_available: u32,
        group_count: u32, privilege_count: u32, modified_id: Luid,
    }
    #[repr(C)]
    struct LsaUnicodeString { length: u16, maximum_length: u16, buffer: *mut u16 }
    #[repr(C)]
    struct LogonSessionData {
        size: u32, logon_id: Luid, user_name: LsaUnicodeString, logon_domain: LsaUnicodeString,
        authentication_package: LsaUnicodeString, logon_type: u32, session: u32,
        sid: *mut core::ffi::c_void, logon_time: i64,
    }
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn OpenProcessToken(process: HANDLE, access: u32, token: *mut HANDLE) -> BOOL;
        fn GetTokenInformation(token: HANDLE, class: u32, info: *mut core::ffi::c_void, size: u32, returned: *mut u32) -> BOOL;
    }
    #[link(name = "secur32")]
    unsafe extern "system" {
        fn LsaGetLogonSessionData(id: *const Luid, data: *mut *mut LogonSessionData) -> i32;
        fn LsaFreeReturnBuffer(data: *mut core::ffi::c_void) -> i32;
    }
    unsafe {
        let mut token = HANDLE::default();
        if !OpenProcessToken(windows::Win32::System::Threading::GetCurrentProcess(), 0x0008, &mut token).as_bool() { return None; }
        let mut stats = std::mem::MaybeUninit::<TokenStatistics>::zeroed();
        let mut returned = 0;
        let ok = GetTokenInformation(token, 10, stats.as_mut_ptr().cast(), std::mem::size_of::<TokenStatistics>() as u32, &mut returned).as_bool();
        let _ = CloseHandle(token);
        if !ok || returned < std::mem::size_of::<TokenStatistics>() as u32 { return None; }
        let authentication_id = stats.assume_init().authentication_id;
        let mut data = std::ptr::null_mut();
        if LsaGetLogonSessionData(&authentication_id, &mut data) != 0 || data.is_null() { return None; }
        let logon_time = (*data).logon_time;
        let _ = LsaFreeReturnBuffer(data.cast());
        (logon_time > 0).then(|| format!("{:08x}{:08x}-{:016x}", authentication_id.high as u32, authentication_id.low, logon_time as u64))
    }
}


fn shell_paths(args: &[String], executable: Option<PathBuf>) -> (PathBuf, PathBuf) {
    let arg_value = |key: &str| args.windows(2).find(|pair| pair[0] == key).map(|pair| PathBuf::from(&pair[1]));
    let root = arg_value("--root").or_else(|| {
        executable.as_ref().and_then(|exe| {
            let parent = exe.parent()?;
            if parent.file_name().is_some_and(|name| name.eq_ignore_ascii_case("bin")) {
                let state = parent.parent()?;
                if state.file_name().is_some_and(|name| name.eq_ignore_ascii_case("state")) { return state.parent().map(PathBuf::from); }
            }
            exe.ancestors().find(|path| path.join("config/shell/theme.json").is_file()).map(PathBuf::from)
        })
    }).or_else(|| std::env::current_dir().ok()).unwrap_or_default();
    let theme = arg_value("--theme").or_else(|| args.iter().enumerate().find(|(i, arg)| {
        !arg.starts_with("--") && (*i == 0 || !matches!(args[*i - 1].as_str(), "--root" | "--theme"))
    }).map(|(_, arg)| PathBuf::from(arg)))
        .unwrap_or_else(|| root.join("config").join("shell").join("theme.json"));
    (root, theme)
}

fn boot_log(root: &Path, text: &str) {
    use std::io::Write;
    let path = root.join("state").join("shell-boot.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 64 * 1024) { let _ = std::fs::remove_file(&path); }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{} pid={} {text}", chrono_stamp(), std::process::id());
    }
}

fn chrono_stamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    format!("t={secs}")
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let explicit_session_shell = args.iter().any(|arg| arg == "--session-shell");
    let (root, theme_path) = shell_paths(&args, std::env::current_exe().ok());
    if !theme_path.is_file() { boot_log(&root, &format!("exit: theme missing {}", theme_path.display())); return; }
    let Some(session_id) = current_session_id() else { boot_log(&root, "exit: no session id"); return };
    let Some(identity) = current_logon_identity() else { boot_log(&root, "exit: no logon identity"); return };
    let pid = unsafe { windows::Win32::System::Threading::GetCurrentProcessId() };
    if explicit_session_shell {
        let marker = shell_session_marker(&root, session_id);
        if std::fs::create_dir_all(marker.parent().unwrap_or(&root)).is_err() || write_shell_marker(&marker, pid, &identity).is_err() { boot_log(&root, "exit: cannot write session marker"); return; }
    }
    let instance = unsafe { windows::Win32::System::Threading::CreateMutexW(None, true, w!("Local\\WinarchyShellInstance")).expect("create shell instance mutex") };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe { let _ = CloseHandle(instance); }
        boot_log(&root, "exit: another shell instance owns the mutex");
        return;
    }
    let session_shell = explicit_session_shell || session_shell_mode(&shell_session_marker(&root, session_id), &identity);
    let (session_status, session_marker) = match update_shell_session_state(&root, session_id, pid, session_shell, &identity) {
        Ok(paths) => (Some(paths.0), Some(paths.1)),
        Err(_) => { boot_log(&root, "exit: cannot write session status"); return }
    };
    boot_log(&root, &format!("start session_shell={session_shell}"));

    let state_dir = root.join("state");
    let prev_hook = std::panic::take_hook();
    let panic_root = root.clone();
    std::panic::set_hook(Box::new(move |i| {
        boot_log(&panic_root, &format!("panic in {:?}: {i}", std::thread::current().name()));
        if std::thread::current().name() == Some("main") {
            dock::restore_native_taskbar();
        }
        prev_hook(i);
    }));
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let inst = GetModuleHandleW(PCWSTR::null()).unwrap();
        let scale = GetDpiForSystem() as f32 / 96.0;
        let com_initialized = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED).is_ok();
        // SetTimer de 8 ms rinde ~15 ms con la resolución por defecto: 1 ms hace real la animación.
        windows::Win32::Media::timeBeginPeriod(1);
        let theme = Theme::load(&theme_path).unwrap_or_default();

        let arrow = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
        let strip_class = WNDCLASSW { lpfnWndProc: Some(strip_proc), hInstance: inst.into(), hCursor: arrow, lpszClassName: w!("WinarchyShellStrip"), ..Default::default() };
        let pill_class = WNDCLASSW { lpfnWndProc: Some(pill_proc), hInstance: inst.into(), hCursor: arrow, lpszClassName: w!("WinarchyShellPill"), ..Default::default() };
        RegisterClassW(&strip_class);
        RegisterClassW(&pill_class);
        let ex = WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE;
        let rects = monitor_rects();
        let strip = CreateWindowExW(ex, strip_class.lpszClassName, w!(""), WS_POPUP, 0, 0, 1, 1, None, None, inst, None).unwrap();
        let pill = CreateWindowExW(ex | WS_EX_LAYERED, pill_class.lpszClassName, w!(""), WS_POPUP, 0, 0, 1, layout::bar_h(scale), None, None, inst, None).unwrap();

        STATE.with(|s| {
            *s.borrow_mut() = Some(State { session_shell, state_dir: state_dir.clone(), theme_mtime: Theme::stamp(&theme_path), theme: theme.clone(), theme_path: theme_path.clone(), scale, pill, strip, cur_w: 0.0, vel: 0.0, target_w: 0.0, cur_h: 0.0, vel_h: 0.0, target_h: 0.0, anim_last: None, last_minute: 99, hidden: false, expand_until: None, sampler: Default::default(), last_vol: None, osd_until: None, last_ws: None, flash_title: None, last_label: Vec::new(), tracking: false, pinned: false, panel: None, panel_shown: None, last_layout: String::new(), last_mode: None, layout: Layout::default(), pill_mon: rects[0], layout_sig: layout_sig(), fade: 1.0, was_open: false, clock_open: false, cal_ym: (1970, 1), cal_sel: 0, countdown: None, countdown_minute: 99 })
        });
        if session_shell { desktop::init(&theme.background, &root, com_initialized); }
        ics::spawn_feed(theme_path.clone());
        extras::spawn_weather(theme_path);
        render_pill(pill);
        let _ = ShowWindow(pill, SW_SHOWNOACTIVATE);
        let dock_on = with(|s| s.theme.dock);
        if dock_on {
            let _ = dock::init(&with(|s| s.theme.clone()), &state_dir);
        }
        if !dock_on || !with(|s| s.theme.hide_native_taskbar) {
            dock::restore_native_taskbar_at_start();
        }
        SetTimer(strip, TIMER_TICK, 1000, None);

        let target = strip.0 as isize;
        brightness::init(move || {
            let _ = PostMessageW(HWND(target as *mut _), WM_BRIGHTNESS, WPARAM(0), LPARAM(0));
        });
        pipe::serve(session_id, move |c| {
            let mut arg = 0;
            let m = match c {
                pipe::Command::Reload => WM_CMD_RELOAD,
                pipe::Command::Quit => WM_CMD_QUIT,
                pipe::Command::Restart => WM_CMD_RESTART,
                pipe::Command::ReturnToExplorer => WM_CMD_RETURN,
                pipe::Command::Redraw => WM_WORKSPACES,
                pipe::Command::Expand => WM_EXPAND,
                pipe::Command::Tray => WM_CMD_TRAY,
                pipe::Command::TrayDismiss => WM_CMD_TRAY_DISMISS,
                pipe::Command::ToggleDesktop => WM_CMD_TOGGLE_DESKTOP,
                pipe::Command::DockFocus => WM_CMD_DOCK_FOCUS,
                pipe::Command::DockPin(n) => {
                    arg = n;
                    WM_CMD_DOCK_PIN
                }
            };
            let _ = PostMessageW(HWND(target as *mut _), m, WPARAM(arg), LPARAM(0));
        });

        komorebi::spawn(session_id, move || {
            let _ = PostMessageW(HWND(target as *mut _), WM_WORKSPACES, WPARAM(0), LPARAM(0));
        });
        komorebi::follow_foreground();
        system::spawn(cluster::notify);
        tray::load_pins(&state_dir);
        if session_shell {
            if tray_host::spawn(theme.clone(), scale) {
                let _ = tray::spawn_cleanup(|| {
                    tray_flyout::notify();
                    cluster::notify();
                });
            } else {
                boot_log(&root, "tray host failed; returning to Explorer");
                request_explorer_return(strip);
            }
        } else {
            let _ = tray::spawn(|| {
                tray_flyout::notify();
                cluster::notify();
            });
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if !desktop_view::translate_accelerator(&msg) {
                let _ = TranslateMessage(&msg);
                let _ = DispatchMessageW(&msg);
            }
        }
        boot_log(&root, "message loop ended");
        if let Some(status) = &session_status { let _ = std::fs::remove_file(status); }
        if session_shell {
            if let Some(marker) = &session_marker { let _ = std::fs::remove_file(marker); }
        }
        if com_initialized {
            windows::Win32::System::Com::CoUninitialize();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    #[test]
    fn restart_reloads_session_shell_and_exits_normal_shell() {
        assert!(matches!(super::restart_action(true), super::RestartAction::Reload));
        assert!(matches!(super::restart_action(false), super::RestartAction::Exit));
    }

    #[test]
    fn flash_only_on_a_real_workspace_change() {
        assert!(!super::workspace_changed(None, Some(0)));
        assert!(!super::workspace_changed(Some(2), Some(2)));
        assert!(!super::workspace_changed(Some(2), None));
        assert!(super::workspace_changed(Some(2), Some(3)));
    }

    #[test]
    fn shell_paths_resolve_installed_and_explicit_roots() {
        let root = PathBuf::from(r"C:\Winarchy");
        assert_eq!(super::shell_status_path(&root, 3), root.join(r"state\shell-session-3"));
        assert_eq!(super::shell_session_marker(&root, 3), root.join(r"state\session-shell-3"));
        let (installed, theme) = super::shell_paths(&[], Some(root.join(r"state\bin\winarchy-shell.exe")));
        assert_eq!(installed, root);
        assert_eq!(theme, root.join(r"config\shell\theme.json"));
        let args = ["--root".into(), root.to_string_lossy().into_owned()];
        let (explicit_root, default_theme) = super::shell_paths(&args, None);
        assert_eq!(explicit_root, root);
        assert_eq!(default_theme, root.join(r"config\shell\theme.json"));
        let explicit_theme = PathBuf::from(r"C:\Themes\custom.json");
        let args = ["--root".into(), root.to_string_lossy().into_owned(), "--theme".into(), explicit_theme.to_string_lossy().into_owned()];
        let (explicit, theme) = super::shell_paths(&args, None);
        assert_eq!(explicit, root);
        assert_eq!(theme, explicit_theme);
    }

    #[test]
    fn current_logon_identity_is_present_well_formed_and_stable() {
        let identity = super::current_logon_identity().expect("current logon identity");
        assert_eq!(identity.len(), 33);
        assert_eq!(identity.as_bytes()[16], b'-');
        assert!(identity.bytes().enumerate().all(|(i, byte)| i == 16 || byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
        assert_eq!(super::current_logon_identity(), Some(identity.clone()));
        if let Ok(expected) = std::env::var("WINARCHY_EXPECTED_LOGON_IDENTITY") {
            assert_eq!(identity, expected);
        }
    }

    #[test]
    fn shell_status_creates_a_missing_status_file() {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("winarchy-status-create-{}-{unique}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("shell-session-3");
        let result = super::write_shell_status(&path, 42, true, "00000000000000ab-01dc000000000001");
        let content = std::fs::read_to_string(&path);
        std::fs::remove_dir_all(&root).unwrap();
        result.unwrap();
        assert_eq!(content.unwrap(), "42|1|00000000000000ab-01dc000000000001");
    }

    #[test]
    fn shell_status_replaces_malformed_existing_status() {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("winarchy-status-replace-{}-{unique}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("shell-session-3");
        std::fs::write(&path, "malformed status").unwrap();
        let result = super::write_shell_status(&path, 43, false, "00000000000000ab-01dc000000000001");
        let content = std::fs::read_to_string(&path);
        std::fs::remove_dir_all(&root).unwrap();
        result.unwrap();
        assert_eq!(content.unwrap(), "43|0|00000000000000ab-01dc000000000001");
    }

    #[test]
    fn shell_marker_replaces_stale_identity() {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("winarchy-marker-write-{}-{unique}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("session-shell-3");
        std::fs::write(&path, "00000000000000ab-01dc000000000000").unwrap();
        let result = super::write_shell_marker(&path, 42, "00000000000000ab-01dc000000000001");
        let content = std::fs::read_to_string(&path);
        std::fs::remove_dir_all(&root).unwrap();
        result.unwrap();
        assert_eq!(content.unwrap(), "00000000000000ab-01dc000000000001");
    }

    #[test]
    fn session_shell_mode_requires_a_matching_logon_identity() {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("winarchy-marker-mode-{}-{unique}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("session-shell-3");
        let identity = "00000000000000ab-01dc000000000001";
        let mut results = vec![super::session_shell_mode(&path, identity)];
        for content in [identity, "00000000000000ab-01dc000000000000", "malformed", ""] {
            std::fs::write(&path, content).unwrap();
            results.push(super::session_shell_mode(&path, identity));
        }
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(results, [false, true, false, false, false]);
    }

    #[test]
    fn segments_dim_keys_and_separators() {
        let seg = super::segments("CPU 26%  RAM 9% · el SEXO FE");
        assert_eq!(seg.iter().map(|(_, d)| *d).collect::<Vec<_>>(), [true, false, false, true, false, true, false, false, false]);
    }

    #[test]
    fn reload_reconciles_each_dock_enabled_transition() {
        assert_eq!(super::dock_change(false, true), super::DockChange::Enable);
        assert_eq!(super::dock_change(true, false), super::DockChange::Disable);
        assert_eq!(super::dock_change(true, true), super::DockChange::Update);
        assert_eq!(super::dock_change(false, false), super::DockChange::None);
    }
}

#[cfg(test)]
mod draw_tests {
    use super::*;
    #[test]
    fn empty_text_is_skipped_not_drawn() {
        unsafe {
            let mem = CreateCompatibleDC(None);
            let bmp = CreateCompatibleBitmap(GetDC(None), 100, 50);
            SelectObject(mem, bmp);
            for fl in [DT_VCENTER, DT_BOTTOM, DT_TOP, DT_BOTTOM | DT_RIGHT, DT_VCENTER | DT_RIGHT, DT_CENTER | DT_VCENTER] {
                draw_text(mem, "", layout::R { x: 0, y: 0, w: 50, h: 20 }, COLORREF(0), fl);
            }
        }
    }
}
