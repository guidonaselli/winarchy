#![windows_subsystem = "windows"]

mod extras;
mod i18n;
mod layout;
mod metrics;
mod komorebi;
mod pipe;
mod spirit;
mod theme;

use std::cell::RefCell;
use std::path::PathBuf;
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
const WM_WORKSPACES: u32 = WM_APP + 4;
const WM_EXPAND: u32 = WM_APP + 5;
const WM_COLLAPSE: u32 = WM_APP + 6;
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

struct State {
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
    layout: Layout,
    pill_mon: RECT,
    layout_sig: Vec<(i32, i32, i32, i32, u32)>,
}

// Contenido del panel expandido; se rearma en cada tick mientras la isla está abierta.
#[derive(Clone)]
struct Panel {
    media: Option<extras::Media>,
    weather: Option<extras::Weather>,
    meters: Vec<(Meter, u32, String)>,
    notes: String,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Meter {
    Cpu,
    Ram,
    Vol,
}

impl Meter {
    fn name(self) -> &'static str {
        tr(match self {
            Meter::Cpu => K::Cpu,
            Meter::Ram => K::Ram,
            Meter::Vol => K::Vol,
        })
    }
}

impl Panel {
    fn shape(&self) -> PanelShape {
        PanelShape { media: self.media.is_some(), weather: self.weather.is_some(), meters: self.meters.len() }
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
unsafe fn monitor_rects() -> Vec<RECT> {
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
unsafe fn mon_scale(r: &RECT) -> f32 {
    let m = MonitorFromPoint(POINT { x: r.left + 1, y: r.top + 1 }, MONITOR_DEFAULTTONEAREST);
    let (mut x, mut y) = (0u32, 0u32);
    if GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut x, &mut y).is_ok() && x > 0 {
        x as f32 / 96.0
    } else {
        1.0
    }
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
    let island = pipe::ISLAND.lock().unwrap().clone().or_else(|| (assist != spirit::Assist::Idle).then(|| pipe::Island { title: assist.label().into(), ..Default::default() }));
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
    let need_raw = layout::bar_width(scale, orb, &dots, lext.cx, ext.cx);
    let widest = need_raw.max(if live_panel.is_some() { px(layout::PANEL_W_DIP, scale) } else { 0 });
    let need = widest.min(px(PILL_MAX_W, scale)).min((mon.right - mon.left) * 3 / 4);
    let base = layout::bar_width(scale, orb, &dots, 0, ext.cx);
    let (width, height, animating, strip, label, panel) = with(|s| {
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
        (width, height, (s.cur_w - s.target_w).abs() > 0.5 || (s.cur_h - s.target_h).abs() > 0.5, s.strip, s.last_label.clone(), s.panel_shown.clone())
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
    let lay = Layout::new(&Inputs { scale, width, height, flip, clock_w: ext.cx, orb, dots: &dots, panel: panel.as_ref().map(Panel::shape) });
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
        let chip_bg = tint(bg, fg, 0.07);
        for c in &pr.chips {
            let r = lay.place(c.r);
            fill_round(pixels, width, height, r, px(7, scale), if c.focused { accent } else { chip_bg }, bg);
            draw_text(mem, &(c.ws + 1).to_string(), r, if c.focused { bg_c } else { muted_c }, DT_CENTER | DT_VCENTER);
        }
        draw_text(mem, &p.notes, lay.place(pr.notes), muted_c, DT_VCENTER | DT_RIGHT);
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
        if w && h {
            s.anim_last = None;
        }
        (s.pill, w && h)
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
    matches!(SHQueryUserNotificationState(), Ok(s) if s == QUNS_BUSY || s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_PRESENTATION_MODE)
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
        let vol = metrics::volume();
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
            let (mut sampler, theme_path) = with(|s| (std::mem::take(&mut s.sampler), s.theme_path.clone()));
            let cpu = sampler.cpu().map(|c| c.round() as u32);
            let ram = metrics::ram_percent();
            let mut meters = Vec::new();
            let mut meter = |kind: Meter, v: Option<u32>| meters.extend(v.map(|v| (kind, v, format!("{v}%"))));
            meter(Meter::Cpu, cpu);
            meter(Meter::Ram, ram);
            if let Some((v, muted)) = metrics::volume() {
                meters.push((Meter::Vol, if muted { 0 } else { v }, if muted { tr(K::MuteValue).into() } else { format!("{v}%") }));
            }
            // widgets.md: solo lo que cambia algo; el clima va al final.
            let mut notes = Vec::new();
            if let Some(g) = sampler.gpu().filter(|g| *g > 50.0) {
                notes.push(format!("{} {g:.0}%", tr(K::Gpu)));
            }
            if let Some((b, ac)) = metrics::battery().filter(|(b, ac)| !ac || *b < 20) {
                notes.push(format!("{} {b}%{}", tr(K::Battery), if ac { "+" } else { "" }));
            }
            if let Some(n) = metrics::notification_count().filter(|n| *n > 0) {
                notes.push(format!("{} {n}", tr(K::Notifications)));
            }
            if metrics::online() == Some(false) {
                notes.push(tr(K::NetOff).into());
            }
            let state_dir = theme_path.ancestors().nth(3).map(|r| r.join("state")).unwrap_or_default();
            notes.extend(metrics::flags(&state_dir).into_iter().map(|k| tr(k).to_string()));
            let media = extras::media();
            let weather = extras::WEATHER.lock().unwrap().clone();
            with(|s| {
                s.sampler = sampler;
                s.panel = Some(Panel { media, weather, meters, notes: notes.join("  ·  ") });
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
    let (pill, hidden, last) = with(|s| (s.pill, s.hidden, s.last_minute));
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
                s.theme = t;
                return true;
            }
        }
        false
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
    if !busy && (reloaded || expanding || moved || minute != last) {
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
        WM_CMD_RELOAD => {
            with(|s| s.theme_mtime = None);
            tick(h);
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
                }
                s.pill
            });
            render_pill(pill);
            LRESULT(0)
        }
        WM_WORKSPACES => {
            let (cur, layout) = komorebi::CURRENT.lock().unwrap().as_ref().map(|w| (Some(w.focused), w.layout.clone())).unwrap_or_default();
            let (ws_changed, layout_changed) = with(|s| {
                let ws = workspace_changed(s.last_ws, cur);
                s.last_ws = cur.or(s.last_ws);
                let lay = !ws && !layout.is_empty() && !s.last_layout.is_empty() && s.last_layout != layout;
                if !layout.is_empty() {
                    s.last_layout = layout.clone();
                }
                (ws, lay)
            });
            let title = match (ws_changed, layout_changed, cur) {
                (true, _, Some(n)) => Some(format!("{} {}", tr(K::Workspace), n + 1)),
                (_, true, _) => Some(format!("{} {layout}", tr(K::Layout))),
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
            let _ = DestroyWindow(h);
            LRESULT(0)
        }
        WM_DESTROY => {
            if with(|s| s.strip) == h {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

unsafe fn shell_open(target: &str) {
    let t = widestr(target);
    let _ = ShellExecuteW(None, w!("open"), PCWSTR(t.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
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
                        (Some(Meter::Vol), None) => {}
                        _ => shell_open("taskmgr.exe"),
                    }
                }
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
            let _ = metrics::adjust_volume(((wp.0 >> 16) as i16 as i32) / 60);
            tick(with(|s| s.strip));
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

fn main() {
    let root = std::env::current_exe().ok().and_then(|p| p.ancestors().nth(4).map(|p| p.to_path_buf())).unwrap_or_default();
    let theme_path = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| root.join("config").join("shell").join("theme.json"));
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let inst = GetModuleHandleW(PCWSTR::null()).unwrap();
        let scale = GetDpiForSystem() as f32 / 96.0;
        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
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
            *s.borrow_mut() = Some(State { theme_mtime: Theme::stamp(&theme_path), theme, theme_path: theme_path.clone(), scale, pill, strip, cur_w: 0.0, vel: 0.0, target_w: 0.0, cur_h: 0.0, vel_h: 0.0, target_h: 0.0, anim_last: None, last_minute: 99, hidden: false, expand_until: None, sampler: Default::default(), last_vol: None, osd_until: None, last_ws: None, flash_title: None, last_label: Vec::new(), tracking: false, pinned: false, panel: None, panel_shown: None, last_layout: String::new(), layout: Layout::default(), pill_mon: rects[0], layout_sig: layout_sig() })
        });
        extras::spawn_weather(theme_path);
        render_pill(pill);
        let _ = ShowWindow(pill, SW_SHOWNOACTIVATE);
        SetTimer(strip, TIMER_TICK, 1000, None);

        let target = strip.0 as isize;
        pipe::serve(move |c| {
            let m = match c {
                pipe::Command::Reload => WM_CMD_RELOAD,
                pipe::Command::Quit => WM_CMD_QUIT,
                pipe::Command::Redraw => WM_WORKSPACES,
                pipe::Command::Expand => WM_EXPAND,
            };
            let _ = PostMessageW(HWND(target as *mut _), m, WPARAM(0), LPARAM(0));
        });

        komorebi::spawn(move || {
            let _ = PostMessageW(HWND(target as *mut _), WM_WORKSPACES, WPARAM(0), LPARAM(0));
        });

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = DispatchMessageW(&msg);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn flash_only_on_a_real_workspace_change() {
        assert!(!super::workspace_changed(None, Some(0)));
        assert!(!super::workspace_changed(Some(2), Some(2)));
        assert!(!super::workspace_changed(Some(2), None));
        assert!(super::workspace_changed(Some(2), Some(3)));
    }

    #[test]
    fn segments_dim_keys_and_separators() {
        let seg = super::segments("CPU 26%  RAM 9% · el SEXO FE");
        assert_eq!(seg.iter().map(|(_, d)| *d).collect::<Vec<_>>(), [true, false, false, true, false, true, false, false, false]);
    }
}
