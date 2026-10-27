#![windows_subsystem = "windows"]

mod extras;
mod metrics;
mod komorebi;
mod pipe;
mod spirit;
mod theme;

use std::cell::RefCell;
use std::path::PathBuf;
use std::time::SystemTime;
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

const WM_APPBAR: u32 = WM_APP + 1;
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
const STRIP_DIP: i32 = 8;
const PILL_DIP: i32 = 32;
const PILL_MAX_W: i32 = 1400;

struct State {
    theme: Theme,
    theme_path: PathBuf,
    theme_mtime: Option<SystemTime>,
    scale: f32,
    taskbar_created: u32,
    pill: HWND,
    strip: HWND,
    cur_w: f32,
    vel: f32,
    target_w: f32,
    anim_last: Option<std::time::Instant>,
    last_minute: u16,
    hidden: bool,
    expand_until: Option<std::time::Instant>,
    sampler: metrics::Sampler,
    last_vol: Option<(u32, bool)>,
    osd_until: Option<std::time::Instant>,
    last_label: Vec<u16>,
    tracking: bool,
    strips: Vec<(HWND, RECT)>,
    pill_mon: RECT,
    layout_sig: Vec<(i32, i32, i32, i32, u32)>,
}

thread_local! { static STATE: RefCell<Option<State>> = const { RefCell::new(None) }; }

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(s.borrow_mut().as_mut().expect("state")))
}

fn px(dip: i32, scale: f32) -> i32 {
    (dip as f32 * scale).round() as i32
}

fn colorref(hex: &str) -> COLORREF {
    let (r, g, b) = rgb(hex);
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}

fn ar_u32(r: u8, g: u8, b: u8) -> u32 {
    0xFF00_0000 | (r as u32) << 16 | (g as u32) << 8 | b as u32
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

// Firma de la disposición de monitores y sus escalas: si cambia, hay que reconstruir las barras.
unsafe fn layout_sig() -> Vec<(i32, i32, i32, i32, u32)> {
    monitor_rects().iter().map(|r| (r.left, r.top, r.right, r.bottom, (mon_scale(r) * 100.0) as u32)).collect()
}

unsafe fn register_strip(h: HWND) {
    let (bottom, mon) = with(|s| (s.theme.position == "bottom", s.strips.iter().find(|(w, _)| *w == h).map(|(_, r)| *r)));
    let mon = mon.unwrap_or_else(|| monitor_rects()[0]);
    let scale = mon_scale(&mon);
    let mut d = APPBARDATA { cbSize: std::mem::size_of::<APPBARDATA>() as u32, hWnd: h, ..Default::default() };
    d.uCallbackMessage = WM_APPBAR;
    SHAppBarMessage(ABM_NEW, &mut d);
    let strip = px(STRIP_DIP, scale);
    d.uEdge = if bottom { ABE_BOTTOM } else { ABE_TOP };
    d.rc = RECT { left: mon.left, top: if bottom { mon.bottom - strip } else { mon.top }, right: mon.right, bottom: if bottom { mon.bottom } else { mon.top + strip } };
    SHAppBarMessage(ABM_QUERYPOS, &mut d);
    if bottom {
        d.rc.top = d.rc.bottom - strip;
    } else {
        d.rc.bottom = d.rc.top + strip;
    }
    SHAppBarMessage(ABM_SETPOS, &mut d);
    let _ = SetWindowPos(h, HWND_TOPMOST, d.rc.left, d.rc.top, d.rc.right - d.rc.left, strip, SWP_NOACTIVATE | SWP_SHOWWINDOW);
}

unsafe fn add_extra_strips(rects: &[RECT]) {
    let inst = GetModuleHandleW(PCWSTR::null()).unwrap();
    for r in &rects[1..] {
        if let Ok(w) = CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE, w!("WinarchyShellStrip"), w!(""), WS_POPUP, 0, 0, 1, px(STRIP_DIP, mon_scale(r)), None, None, inst, None) {
            with(|s| s.strips.push((w, *r)));
            register_strip(w);
        }
    }
}

// Tras un cambio de monitores: se destruyen las barras secundarias y se recrean a medida.
unsafe fn rebuild_strips() {
    with(|s| s.layout_sig = layout_sig());
    let (hub, extra) = with(|s| (s.strip, s.strips.iter().map(|(w, _)| *w).filter(|w| *w != s.strip).collect::<Vec<_>>()));
    for w in extra {
        let _ = DestroyWindow(w);
    }
    let rects = monitor_rects();
    with(|s| s.strips = vec![(hub, rects[0])]);
    register_strip(hub);
    add_extra_strips(&rects);
    render_pill(with(|s| s.pill));
}

unsafe fn render_pill(pill: HWND) {
    let mon = focus_monitor();
    let scale = mon_scale(&mon);
    let theme = with(|s| {
        if (s.scale - scale).abs() > 0.01 {
            s.scale = scale;
            s.cur_w = 0.0;
            s.vel = 0.0;
        }
        s.theme.clone()
    });
    let t = GetLocalTime();
    let text: Vec<u16> = format!("{:02}:{:02}", t.wHour, t.wMinute).encode_utf16().collect();
    let h = px(PILL_DIP, scale);
    let screen = GetDC(None);
    let mem = CreateCompatibleDC(screen);
    let family = widestr(&theme.font_family);
    let font = CreateFontW(-px(theme.font_size, scale), 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, CLEARTYPE_QUALITY.0 as u32, 0, PCWSTR(family.as_ptr()));
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
    let dots: Vec<bool> = focused_ws.occupied.iter().enumerate().filter(|(i, o)| **o || *i == focused_ws.focused).map(|(i, _)| i == focused_ws.focused).collect();
    let (dot, gap, active, pad, sep) = (px(6, scale), px(5, scale), px(18, scale), px(16, scale), px(12, scale));
    let dots_w: i32 = dots.iter().map(|f| if *f { active } else { dot }).sum::<i32>() + gap * (dots.len() as i32 - 1).max(0);
    let orb_lead = if assist == spirit::Assist::Idle { 0 } else { h };
    let lead = orb_lead + if dots.is_empty() { 0 } else { dots_w + sep };
    let label_w = if label.is_empty() { 0 } else { lext.cx + sep };
    let need_raw = pad * 2 + lead + label_w + ext.cx;
    let need = need_raw.min(px(PILL_MAX_W, scale)).min((mon.right - mon.left) * 3 / 4);
    let base = pad * 2 + lead + ext.cx;
    let (width, animating, strip, label) = with(|s| {
        s.target_w = need as f32;
        if s.cur_w == 0.0 {
            s.cur_w = need as f32;
        }
        let width = (s.cur_w.round() as i32).max(base);
        // Al colapsar se sigue mostrando la última etiqueta, recortada, hasta que el ancho llega a la base.
        if !label.is_empty() {
            s.last_label = label;
        } else if width <= base {
            s.last_label.clear();
        }
        (width, (s.cur_w - s.target_w).abs() > 0.5, s.strip, s.last_label.clone())
    });
    if animating {
        SetTimer(strip, TIMER_ANIM, 8, None);
    } else {
        let name = widestr(&format!("Winarchy {} {}", String::from_utf16_lossy(&text), String::from_utf16_lossy(&label)));
        let _ = SetWindowTextW(pill, PCWSTR(name.as_ptr()));
    }
    let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: width, biHeight: -h, biPlanes: 1, biBitCount: 32, ..Default::default() }, ..Default::default() };
    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let dib = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
    let old_bmp = SelectObject(mem, dib);
    let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (width * h) as usize);

    let (br, bg, bb) = rgb(&theme.background);
    let r = h as f32 / 2.0;
    let mask: Vec<f32> = (0..h * width)
        .map(|i| {
            let (x, y) = ((i % width) as f32 + 0.5, (i / width) as f32 + 0.5);
            let cx = x.clamp(r, width as f32 - r);
            (r - ((x - cx).powi(2) + (y - r).powi(2)).sqrt() + 0.5).clamp(0.0, 1.0)
        })
        .collect();
    let premul = |c: u8, a: f32| (c as f32 * a) as u32;
    for (p, a) in pixels.iter_mut().zip(&mask) {
        *p = ((*a * 255.0) as u32) << 24 | premul(br, *a) << 16 | premul(bg, *a) << 8 | premul(bb, *a);
    }
    let (ar, ag, ab) = rgb(&theme.accent);
    let (mr, mg, mb) = rgb(&theme.muted);
    let mut x0 = pad + orb_lead;
    for focused in &dots {
        let w = if *focused { active } else { dot };
        let (cr, cg, cb) = if *focused { (ar, ag, ab) } else { (mr, mg, mb) };
        let (rad, cy) = (dot as f32 / 2.0, h as f32 / 2.0);
        for y in 0..h {
            for x in x0..x0 + w {
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                let cx = fx.clamp(x0 as f32 + rad, (x0 + w) as f32 - rad);
                let cov = (rad - ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt() + 0.5).clamp(0.0, 1.0);
                if cov > 0.0 {
                    let i = (y * width + x) as usize;
                    let mix = |bg: u8, c: u8| (bg as f32 * (1.0 - cov) + c as f32 * cov) as u32;
                    pixels[i] = mix(br, cr) << 16 | mix(bg, cg) << 8 | mix(bb, cb);
                }
            }
        }
        x0 += w + gap;
    }
    let animate = animations_enabled();
    if assist != spirit::Assist::Idle {
        spirit::paint(pixels, width, pad / 2, h, &spirit::shapes(assist, if animate { assist_t } else { 0.0 }), (ar, ag, ab));
        SetTimer(strip, TIMER_SPIRIT, if animate { 33 } else { 250 }, None);
    } else {
        let _ = KillTimer(strip, TIMER_SPIRIT);
    }
    if let Some(p) = island.as_ref().and_then(|i| i.progress) {
        let (bar_h, bar_w) = (px(2, scale), ((width - 2 * pad) as f64 * p) as i32);
        for y in h - bar_h - px(3, scale)..h - px(3, scale) {
            for x in pad..pad + bar_w {
                pixels[(y * width + x) as usize] = ar_u32(ar, ag, ab);
            }
        }
    }
    SetBkMode(mem, TRANSPARENT);
    SetTextColor(mem, colorref(&theme.foreground));
    let avail = width - pad - ext.cx - sep - (pad + lead);
    if !label.is_empty() && avail > 0 {
        let clip = RECT { left: pad + lead, top: 0, right: pad + lead + avail, bottom: h };
        if lext.cx == 0 {
            let _ = GetTextExtentPoint32W(mem, &label, &mut lext);
        }
        if need_raw > need && width >= need {
            let (mut text, mut rc) = (label.clone(), clip);
            DrawTextW(mem, &mut text, &mut rc, DT_END_ELLIPSIS | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
        } else {
            let (mut x, y) = (pad + lead, (h - lext.cy) / 2);
            for (seg, dim) in segments(&String::from_utf16_lossy(&label)) {
                let w: Vec<u16> = seg.encode_utf16().collect();
                SetTextColor(mem, colorref(if dim { &theme.muted } else { &theme.foreground }));
                let _ = ExtTextOutW(mem, x, y, ETO_CLIPPED, Some(&clip), PCWSTR(w.as_ptr()), w.len() as u32, None);
                let mut e = SIZE::default();
                let _ = GetTextExtentPoint32W(mem, &w, &mut e);
                x += e.cx;
            }
        }
    }
    let _ = TextOutW(mem, width - pad - ext.cx, (h - ext.cy) / 2, &text);
    // GDI no escribe alfa: dentro de la pastilla el texto debe quedar opaco.
    for (p, a) in pixels.iter_mut().zip(&mask) {
        if *a >= 1.0 {
            *p |= 0xFF00_0000;
        }
    }

    with(|s| s.pill_mon = mon);
    let y = if theme.position == "bottom" { mon.bottom - h - px(2, scale) } else { mon.top + px(2, scale) };
    let pos = POINT { x: mon.left + (mon.right - mon.left - width) / 2, y };
    let size = SIZE { cx: width, cy: h };
    let src = POINT::default();
    let bf = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, SourceConstantAlpha: (theme.opacity.clamp(0.0, 1.0) * 255.0) as u8, AlphaFormat: AC_SRC_ALPHA as u8, ..Default::default() };
    let _ = UpdateLayeredWindow(pill, None, Some(&pos), Some(&size), mem, Some(&src), COLORREF(0), Some(&bf), ULW_ALPHA);

    SelectObject(mem, old_bmp);
    SelectObject(mem, old_font);
    let _ = DeleteObject(dib);
    let _ = DeleteObject(font);
    let _ = DeleteDC(mem);
    ReleaseDC(None, screen);
}

// Resorte críticamente amortiguado sobre el ancho de la pastilla; dt real para no depender de la resolución del timer.
unsafe fn spring_step(strip: HWND) {
    const OMEGA: f32 = 30.0;
    let (pill, settled) = with(|s| {
        let now = std::time::Instant::now();
        let dt = s.anim_last.map(|t| (now - t).as_secs_f32()).unwrap_or(0.008).min(0.05);
        s.anim_last = Some(now);
        // Solución exacta del resorte críticamente amortiguado: estable para cualquier dt, aunque el hilo se atrase.
        let (d, e) = (s.cur_w - s.target_w, (-OMEGA * dt).exp());
        let k = s.vel + OMEGA * d;
        s.cur_w = s.target_w + (d + k * dt) * e;
        s.vel = (s.vel - OMEGA * k * dt) * e;
        let settled = (s.cur_w - s.target_w).abs() < 0.5 && s.vel.abs() < 1.0;
        if settled {
            s.cur_w = s.target_w;
            s.vel = 0.0;
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

fn foreground_title() -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetWindowTextW(GetForegroundWindow(), &mut buf) } as usize;
    let t = String::from_utf16_lossy(&buf[..n]);
    if t.chars().count() > 24 { t.chars().take(23).chain(std::iter::once('…')).collect() } else { t }
}

// Mientras la isla está expandida refresca las métricas cada tick; al vencer, la limpia. Devuelve si hay que redibujar.
// Las llamadas lentas (COM, WinRT) corren fuera del borrow del estado: pueden bombear mensajes y reentrar al wndproc.
fn expand_step() -> bool {
    let now = std::time::Instant::now();
    let over = unsafe { hovered(with(|s| s.pill)) };
    let until = with(|s| {
        if over && s.expand_until.is_some() {
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
                *pipe::ISLAND.lock().unwrap() = Some(pipe::Island { title: if muted { "MUTE".into() } else { "VOL".into() }, detail: format!("{v}%"), progress: Some(v as f64 / 100.0) });
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
            let cpu = sampler.cpu().map(|c| format!("{c:.0}%")).unwrap_or_else(|| "--".into());
            let ram = metrics::ram_percent().map(|r| format!("{r}%")).unwrap_or_else(|| "--".into());
            let mut rest = Vec::new();
            if let Some(l) = komorebi::CURRENT.lock().unwrap().as_ref().map(|w| w.layout.clone()).filter(|l| !l.is_empty()) {
                rest.push(l);
            }
            if let Some(m) = extras::media() {
                rest.push(m);
            }
            let title = foreground_title();
            if !title.is_empty() {
                rest.push(title);
            }
            if let Some(w) = extras::WEATHER.lock().unwrap().clone() {
                rest.push(w);
            }
            if let Some(g) = sampler.gpu() {
                rest.push(format!("GPU {g:.0}%"));
            }
            if let Some((v, muted)) = metrics::volume() {
                rest.push(if muted { "VOL mute".to_string() } else { format!("VOL {v}%") });
            }
            if let Some((b, ac)) = metrics::battery() {
                rest.push(format!("BAT {b}%{}", if ac { "+" } else { "" }));
            }
            if let Some(n) = metrics::notification_count().filter(|n| *n > 0) {
                rest.push(format!("NOTIF {n}"));
            }
            if let Some(on) = metrics::online() {
                rest.push(if on { "NET ok".into() } else { "NET off".into() });
            }
            let state_dir = theme_path.ancestors().nth(3).map(|r| r.join("state")).unwrap_or_default();
            rest.extend(metrics::flags(&state_dir).into_iter().map(String::from));
            with(|s| {
                s.sampler = sampler;
            });
            *pipe::ISLAND.lock().unwrap() = Some(pipe::Island { title: format!("CPU {cpu}  RAM {ram}"), detail: rest.join("  ·  "), progress: None });
            true
        }
        Some(_) => {
            with(|s| s.expand_until = None);
            *pipe::ISLAND.lock().unwrap() = None;
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
        rebuild_strips();
    }
    let (pill, stale) = with(|s| (s.pill, s.tracking));
    if stale && !hovered(pill) {
        with(|s| s.tracking = false);
        let _ = PostMessageW(with(|s| s.strip), WM_COLLAPSE, WPARAM(0), LPARAM(0));
    }
    let expanding = expand_step();
    let minute = GetLocalTime().wMinute;
    if reloaded {
        for w in with(|s| s.strips.iter().map(|(w, _)| *w).collect::<Vec<_>>()) {
            register_strip(w);
            let _ = InvalidateRect(w, None, true);
        }
    }
    let fm = focus_monitor();
    let moved = with(|s| (fm.left, fm.top) != (s.pill_mon.left, s.pill_mon.top));
    if !busy && (reloaded || expanding || moved || minute != last) {
        with(|s| s.last_minute = minute);
        render_pill(pill);
    }
}

unsafe extern "system" fn strip_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let tc = STATE.with(|s| s.borrow().as_ref().map(|s| s.taskbar_created)).unwrap_or(0);
    if tc != 0 && m == tc {
        register_strip(h);
        return LRESULT(0);
    }
    match m {
        WM_ERASEBKGND => {
            let mut rc = RECT::default();
            let _ = GetClientRect(h, &mut rc);
            let brush = CreateSolidBrush(colorref(&with(|s| s.theme.background.clone())));
            FillRect(HDC(wp.0 as *mut _), &rc, brush);
            let _ = DeleteObject(brush);
            LRESULT(1)
        }
        WM_TIMER if wp.0 == TIMER_SPIRIT => {
            render_pill(with(|s| s.pill));
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
                s.expand_until = None;
                s.pill
            });
            *pipe::ISLAND.lock().unwrap() = None;
            render_pill(pill);
            LRESULT(0)
        }
        WM_WORKSPACES => {
            render_pill(with(|s| s.pill));
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            if with(|s| s.strip) == h {
                rebuild_strips();
            }
            LRESULT(0)
        }
        WM_CMD_QUIT => {
            for w in with(|s| s.strips.iter().map(|(w, _)| *w).filter(|w| *w != h).collect::<Vec<_>>()) {
                let _ = DestroyWindow(w);
            }
            let _ = DestroyWindow(h);
            LRESULT(0)
        }
        WM_DESTROY => {
            let mut d = APPBARDATA { cbSize: std::mem::size_of::<APPBARDATA>() as u32, hWnd: h, ..Default::default() };
            SHAppBarMessage(ABM_REMOVE, &mut d);
            if with(|s| s.strip) == h {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

unsafe extern "system" fn pill_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
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
            }
            if !expanded {
                let _ = PostMessageW(strip, WM_EXPAND, WPARAM(0), LPARAM(0));
            }
            return LRESULT(0);
        }
        WM_MOUSELEAVE => {
            let strip = with(|s| {
                s.tracking = false;
                s.strip
            });
            let _ = PostMessageW(strip, WM_COLLAPSE, WPARAM(0), LPARAM(0));
            return LRESULT(0);
        }
        _ => {}
    }
    DefWindowProcW(h, m, wp, lp)
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
        let strip = CreateWindowExW(ex, strip_class.lpszClassName, w!(""), WS_POPUP, 0, 0, 1, px(STRIP_DIP, scale), None, None, inst, None).unwrap();
        let pill = CreateWindowExW(ex | WS_EX_LAYERED, pill_class.lpszClassName, w!(""), WS_POPUP, 0, 0, 1, px(PILL_DIP, scale), None, None, inst, None).unwrap();

        STATE.with(|s| {
            *s.borrow_mut() = Some(State { theme_mtime: Theme::stamp(&theme_path), theme, theme_path, scale, taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")), pill, strip, cur_w: 0.0, vel: 0.0, target_w: 0.0, anim_last: None, last_minute: 99, hidden: false, expand_until: None, sampler: Default::default(), last_vol: None, osd_until: None, last_label: Vec::new(), tracking: false, strips: vec![(strip, rects[0])], pill_mon: rects[0], layout_sig: layout_sig() })
        });
        register_strip(strip);
        add_extra_strips(&rects);
        extras::spawn_weather();
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
    fn segments_dim_keys_and_separators() {
        let seg = super::segments("CPU 26%  RAM 9% · el SEXO FE");
        assert_eq!(seg.iter().map(|(_, d)| *d).collect::<Vec<_>>(), [true, false, false, true, false, true, false, false, false]);
    }
}
