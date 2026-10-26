mod extras;
mod metrics;
mod komorebi;
mod pipe;
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
const STRIP_DIP: i32 = 8;
const PILL_DIP: i32 = 32;
const PILL_MAX_W: i32 = 1000;

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
    ai: extras::AiSessions,
    last_vol: Option<(u32, bool)>,
    osd_until: Option<std::time::Instant>,
    last_label: Vec<u16>,
    tracking: bool,
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

unsafe fn register_strip(h: HWND) {
    let (scale, bottom) = with(|s| (s.scale, s.theme.position == "bottom"));
    let mut d = APPBARDATA { cbSize: std::mem::size_of::<APPBARDATA>() as u32, hWnd: h, ..Default::default() };
    d.uCallbackMessage = WM_APPBAR;
    SHAppBarMessage(ABM_NEW, &mut d);
    let (sw, sh, strip) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN), px(STRIP_DIP, scale));
    d.uEdge = if bottom { ABE_BOTTOM } else { ABE_TOP };
    d.rc = RECT { left: 0, top: if bottom { sh - strip } else { 0 }, right: sw, bottom: if bottom { sh } else { strip } };
    SHAppBarMessage(ABM_QUERYPOS, &mut d);
    if bottom {
        d.rc.top = d.rc.bottom - strip;
    } else {
        d.rc.bottom = d.rc.top + strip;
    }
    SHAppBarMessage(ABM_SETPOS, &mut d);
    let _ = SetWindowPos(h, HWND_TOPMOST, d.rc.left, d.rc.top, d.rc.right - d.rc.left, strip, SWP_NOACTIVATE | SWP_SHOWWINDOW);
}

unsafe fn render_pill(pill: HWND) {
    let (theme, scale) = with(|s| (s.theme.clone(), s.scale));
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
    let island = pipe::ISLAND.lock().unwrap().clone();
    let label: Vec<u16> = island.as_ref().map(|i| if i.detail.is_empty() { i.title.clone() } else { format!("{}  {}", i.title, i.detail) }).unwrap_or_default().encode_utf16().collect();
    let mut lext = SIZE::default();
    if !label.is_empty() {
        let _ = GetTextExtentPoint32W(mem, &label, &mut lext);
    }
    let focused_ws = komorebi::CURRENT.lock().unwrap().clone().unwrap_or_default();
    let dots: Vec<bool> = focused_ws.occupied.iter().enumerate().filter(|(i, o)| **o || *i == focused_ws.focused).map(|(i, _)| i == focused_ws.focused).collect();
    let (dot, gap, active, pad, sep) = (px(6, scale), px(5, scale), px(18, scale), px(16, scale), px(12, scale));
    let dots_w: i32 = dots.iter().map(|f| if *f { active } else { dot }).sum::<i32>() + gap * (dots.len() as i32 - 1).max(0);
    let lead = if dots.is_empty() { 0 } else { dots_w + sep };
    let label_w = if label.is_empty() { 0 } else { lext.cx + sep };
    let need = (pad * 2 + lead + label_w + ext.cx).min(px(PILL_MAX_W, scale));
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
    let mut x0 = pad;
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
        let _ = ExtTextOutW(mem, pad + lead, (h - lext.cy) / 2, ETO_CLIPPED, Some(&clip), PCWSTR(label.as_ptr()), label.len() as u32, None);
    }
    let _ = TextOutW(mem, width - pad - ext.cx, (h - ext.cy) / 2, &text);
    // GDI no escribe alfa: dentro de la pastilla el texto debe quedar opaco.
    for (p, a) in pixels.iter_mut().zip(&mask) {
        if *a >= 1.0 {
            *p |= 0xFF00_0000;
        }
    }

    let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
    let y = if theme.position == "bottom" { sh - h - px(2, scale) } else { px(2, scale) };
    let pos = POINT { x: (sw - width) / 2, y };
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
        let acc = OMEGA * OMEGA * (s.target_w - s.cur_w) - 2.0 * OMEGA * s.vel;
        s.vel += acc * dt;
        s.cur_w += s.vel * dt;
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

unsafe fn fullscreen_busy() -> bool {
    matches!(SHQueryUserNotificationState(), Ok(s) if s == QUNS_BUSY || s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_PRESENTATION_MODE)
}

fn foreground_title() -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetWindowTextW(GetForegroundWindow(), &mut buf) } as usize;
    String::from_utf16_lossy(&buf[..n]).chars().take(28).collect()
}

// Mientras la isla está expandida refresca las métricas cada tick; al vencer, la limpia. Devuelve si hay que redibujar.
// Las llamadas lentas (COM, WinRT) corren fuera del borrow del estado: pueden bombear mensajes y reentrar al wndproc.
fn expand_step() -> bool {
    let now = std::time::Instant::now();
    let until = with(|s| s.expand_until);
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
            let (mut sampler, mut ai, theme_path) = with(|s| (std::mem::take(&mut s.sampler), std::mem::replace(&mut s.ai, extras::AiSessions::new()), s.theme_path.clone()));
            let cpu = sampler.cpu().map(|c| format!("{c:.0}%")).unwrap_or_else(|| "--".into());
            let ram = metrics::ram_percent().map(|r| format!("{r}%")).unwrap_or_else(|| "--".into());
            let mut rest = Vec::new();
            if let Some(l) = komorebi::CURRENT.lock().unwrap().as_ref().map(|w| w.layout.clone()).filter(|l| !l.is_empty()) {
                rest.push(l);
            }
            let title = foreground_title();
            if !title.is_empty() {
                rest.push(title);
            }
            if let Some(m) = extras::media() {
                rest.push(m);
            }
            if let Some(w) = extras::WEATHER.lock().unwrap().clone() {
                rest.push(w);
            }
            if let Some(a) = ai.label() {
                rest.push(a);
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
                s.ai = ai;
            });
            *pipe::ISLAND.lock().unwrap() = Some(pipe::Island { title: format!("CPU {cpu}  RAM {ram}"), detail: rest.join("  "), progress: None });
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

unsafe fn tick(strip: HWND) {
    if IN_TICK.with(|t| t.replace(true)) {
        return;
    }
    tick_inner(strip);
    IN_TICK.with(|t| t.set(false));
}

unsafe fn tick_inner(strip: HWND) {
    let (pill, hidden, last) = with(|s| (s.pill, s.hidden, s.last_minute));
    let busy = fullscreen_busy();
    if busy != hidden {
        let _ = ShowWindow(pill, if busy { SW_HIDE } else { SW_SHOWNOACTIVATE });
        with(|s| s.hidden = busy);
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
    let expanding = expand_step();
    let minute = GetLocalTime().wMinute;
    if reloaded {
        register_strip(strip);
        let _ = InvalidateRect(strip, None, true);
    }
    if !busy && (reloaded || expanding || minute != last) {
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
        WM_CMD_QUIT => {
            let _ = DestroyWindow(h);
            LRESULT(0)
        }
        WM_DESTROY => {
            let mut d = APPBARDATA { cbSize: std::mem::size_of::<APPBARDATA>() as u32, hWnd: h, ..Default::default() };
            SHAppBarMessage(ABM_REMOVE, &mut d);
            PostQuitMessage(0);
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
        let strip = CreateWindowExW(ex, strip_class.lpszClassName, w!(""), WS_POPUP, 0, 0, 1, px(STRIP_DIP, scale), None, None, inst, None).unwrap();
        let pill = CreateWindowExW(ex | WS_EX_LAYERED, pill_class.lpszClassName, w!(""), WS_POPUP, 0, 0, 1, px(PILL_DIP, scale), None, None, inst, None).unwrap();

        STATE.with(|s| {
            *s.borrow_mut() = Some(State { theme_mtime: Theme::stamp(&theme_path), theme, theme_path, scale, taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")), pill, strip, cur_w: 0.0, vel: 0.0, target_w: 0.0, anim_last: None, last_minute: 99, hidden: false, expand_until: None, sampler: Default::default(), ai: extras::AiSessions::new(), last_vol: None, osd_until: None, last_label: Vec::new(), tracking: false })
        });
        register_strip(strip);
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
