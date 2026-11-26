use crate::dock::{blend, draw_label, draw_text, fill_round_a};
use windows::Win32::Graphics::Gdi::DT_LEFT;
use crate::layout::{px, R};
use crate::theme::{rgb, Theme};
use crate::tray::{self, BalloonRequest};
use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::{Duration, Instant};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC, SND_NODEFAULT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::SHQueryUserNotificationState;
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_START: u32 = WM_APP + 91;
const WM_DISMISS: u32 = WM_APP + 92;
const WM_THEME: u32 = WM_APP + 93;
const TICK: usize = 1;
const TICK_MS: u32 = 250;
const NIIF_ICON_MASK: u32 = 0xF;
const NIIF_INFO: u32 = 0x1;
const NIIF_WARNING: u32 = 0x2;
const NIIF_ERROR: u32 = 0x3;
const NIIF_NOSOUND: u32 = 0x10;
const NIIF_RESPECT_QUIET_TIME: u32 = 0x80;
const NIN_BALLOONSHOW: u32 = 0x402;
const NIN_BALLOONHIDE: u32 = 0x403;
const NIN_BALLOONTIMEOUT: u32 = 0x404;
const NIN_BALLOONUSERCLICK: u32 = 0x405;
const QUNS_UNKNOWN: i32 = 0;
const QUNS_ACCEPTS_NOTIFICATIONS: i32 = 5;
const QUNS_QUIET_TIME: i32 = 6;
const BALLOON_MS: u32 = 0x2016;
const CLOSE_GLYPH: &str = "\u{E711}";
const ICON_FONT: &str = "Segoe MDL2 Assets";

struct Surface {
    hwnd: HWND,
    owner: HWND,
    theme: Theme,
    scale: f32,
    request: Option<BalloonRequest>,
    generation: u64,
    visible: bool,
    ever_shown: bool,
    remaining: Option<Duration>,
    last_tick: Option<Instant>,
    rect: R,
}

static HWND_ATOMIC: AtomicIsize = AtomicIsize::new(0);
static THEME_UPDATE: std::sync::Mutex<Option<Theme>> = std::sync::Mutex::new(None);
#[cfg(test)]
static NOTIFICATION_STATE_OVERRIDE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);
thread_local! { static SURFACE: RefCell<Option<Surface>> = const { RefCell::new(None) }; }

fn with<R>(f: impl FnOnce(&mut Surface) -> R) -> Option<R> {
    SURFACE.with(|s| s.try_borrow_mut().ok().and_then(|mut s| s.as_mut().map(f)))
}

#[cfg(test)]
pub fn set_notification_state_for_test(state: Option<i32>) {
    NOTIFICATION_STATE_OVERRIDE.store(state.unwrap_or(-1), Ordering::Relaxed);
}

fn notification_state() -> Option<i32> {
    #[cfg(test)]
    {
        let state = NOTIFICATION_STATE_OVERRIDE.load(Ordering::Relaxed);
        if state != -1 { return Some(state); }
    }
    unsafe { SHQueryUserNotificationState().ok().map(|state| state.0) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability { Show, Wait, Hide, Discard }

pub fn availability(request: &BalloonRequest) -> Availability {
    availability_for_state(notification_state(), request.flags, request.realtime, false)
}

fn availability_for_state(state: Option<i32>, flags: u32, realtime: bool, shown: bool) -> Availability {
    match state {
        Some(QUNS_QUIET_TIME) if flags & NIIF_RESPECT_QUIET_TIME != 0 => Availability::Hide,
        // sin Explorer nadie calcula el estado y la API devuelve 0: no hay restricción conocida, así que se muestra
        Some(QUNS_ACCEPTS_NOTIFICATIONS | QUNS_UNKNOWN) | None => Availability::Show,
        _ if realtime && !shown => Availability::Discard,
        _ => Availability::Wait,
    }
}

pub fn attach(owner: HWND, theme: Theme, scale: f32) -> bool {
    unsafe {
        let Ok(instance) = GetModuleHandleW(PCWSTR::null()) else { return false };
        let class = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: instance.into(), hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(), lpszClassName: w!("WinarchyShellNotification"), ..Default::default() };
        if RegisterClassW(&class) == 0 { return false; }
        let ex = WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_LAYERED;
        let Ok(hwnd) = CreateWindowExW(ex, class.lpszClassName, w!(""), WS_POPUP, 0, 0, 1, 1, owner, None, instance, None) else { return false };
        crate::accessibility::create(hwnd, "Winarchy notification", accesskit::Role::Alert);
        SURFACE.with(|s| *s.borrow_mut() = Some(Surface { hwnd, owner, theme, scale, request: None, generation: 0, visible: false, ever_shown: false, remaining: None, last_tick: None, rect: R::default() }));
        HWND_ATOMIC.store(hwnd.0 as isize, Ordering::Relaxed);
        true
    }
}

pub fn start() {
    let hwnd = HWND_ATOMIC.load(Ordering::Relaxed);
    if hwnd != 0 { unsafe { let _ = PostMessageW(HWND(hwnd as *mut _), WM_START, WPARAM(0), LPARAM(0)); } }
}

pub fn dismiss() {
    let hwnd = HWND_ATOMIC.load(Ordering::Relaxed);
    if hwnd != 0 { unsafe { let _ = PostMessageW(HWND(hwnd as *mut _), WM_DISMISS, WPARAM(0), LPARAM(0)); } }
}

pub fn update_theme(theme: Theme) {
    let hwnd = HWND_ATOMIC.load(Ordering::Relaxed);
    if hwnd == 0 { return; }
    *THEME_UPDATE.lock().unwrap() = Some(theme);
    unsafe { let _ = PostMessageW(HWND(hwnd as *mut _), WM_THEME, WPARAM(0), LPARAM(0)); }
}

fn apply_theme_update() -> Option<bool> {
    let theme = THEME_UPDATE.lock().unwrap().take()?;
    with(|s| {
        s.theme = theme;
        s.visible
    })
}

fn begin(hwnd: HWND) {
    let Some(request) = tray::current_balloon() else {
        hide(hwnd);
        unsafe { let _ = KillTimer(hwnd, TICK); }
        return;
    };
    hide(hwnd);
    with(|s| {
        s.request = Some(request);
        s.generation = s.generation.wrapping_add(1).max(1);
        s.visible = false;
        s.ever_shown = false;
        s.remaining = None;
        s.last_tick = None;
    });
    unsafe { SetTimer(hwnd, TICK, TICK_MS, None); }
    tick(hwnd);
}

fn tick(hwnd: HWND) {
    let Some(request) = tray::current_balloon() else { hide(hwnd); unsafe { let _ = KillTimer(hwnd, TICK); } return };
    let different = with(|s| s.request.as_ref().is_none_or(|shown| shown.sequence != request.sequence)).unwrap_or(true);
    if different { begin(hwnd); return; }
    with(|s| s.request = Some(request.clone()));
    let shown = with(|s| s.ever_shown).unwrap_or(false);
    let availability = availability_for_state(notification_state(), request.flags, request.realtime, shown);
    match availability {
        Availability::Show => {}
        Availability::Wait => { hide(hwnd); return; }
        Availability::Hide => {
            advance(hwnd, &request, Some(NIN_BALLOONHIDE));
            return;
        }
        Availability::Discard => { advance(hwnd, &request, None); return; }
    }
    if !shown {
        if show(hwnd) {
            play_sound(request.flags);
            tray::notify_balloon_owner(&request, NIN_BALLOONSHOW);
            with(|s| {
                s.ever_shown = true;
                s.remaining = Some(message_duration());
                s.last_tick = Some(Instant::now());
            });
        }
        return;
    }
    let expired = with(|s| {
        if s.visible {
            if let (Some(remaining), Some(last)) = (&mut s.remaining, s.last_tick) {
                *remaining = remaining.saturating_sub(last.elapsed());
                s.last_tick = Some(Instant::now());
            }
            s.remaining.is_some_and(|remaining| remaining.is_zero())
        } else {
            s.last_tick = Some(Instant::now());
            false
        }
    }).unwrap_or(false);
    if expired {
        advance(hwnd, &request, Some(NIN_BALLOONTIMEOUT));
    } else if !with(|s| s.visible).unwrap_or(false) {
        show(hwnd);
    }
}

fn message_duration() -> Duration {
    let mut seconds = 5u32;
    unsafe { let _ = SystemParametersInfoW(SYSTEM_PARAMETERS_INFO_ACTION(BALLOON_MS), 0, Some(&mut seconds as *mut _ as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)); }
    Duration::from_secs(seconds.max(1) as u64)
}

fn play_sound(flags: u32) {
    if flags & NIIF_NOSOUND != 0 { return; }
    let alias = match flags & NIIF_ICON_MASK {
        NIIF_WARNING => w!("SystemExclamation"),
        NIIF_ERROR => w!("SystemHand"),
        NIIF_INFO => w!("SystemAsterisk"),
        _ => return,
    };
    unsafe { let _ = PlaySoundW(alias, None, SND_ALIAS | SND_ASYNC | SND_NODEFAULT); }
}

// Entrada y salida del aviso: fundido con un deslizamiento corto, sin repintar el contenido.
fn glide(hwnd: HWND, at: (i32, i32), off: i32, entering: bool) {
    const FRAMES: i32 = 8;
    for i in 1..=FRAMES {
        let e = crate::dock::ease(i as f32 / FRAMES as f32);
        let (alpha, dy) = if entering { (e, off as f32 * (1.0 - e)) } else { (1.0 - e, off as f32 * e) };
        let pos = POINT { x: at.0, y: at.1 + dy.round() as i32 };
        let blend = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: (alpha * 255.0).round() as u8, AlphaFormat: 1 };
        unsafe { let _ = UpdateLayeredWindow(hwnd, None, Some(&pos), None, None, None, COLORREF(0), Some(&blend), ULW_ALPHA); }
        std::thread::sleep(Duration::from_millis(16));
    }
}

fn hide(hwnd: HWND) {
    if let Some((theme, scale, true)) = with(|s| (s.theme.clone(), s.scale, s.visible)) {
        if crate::dock::motion_enabled(&theme) {
            let mut r = RECT::default();
            unsafe { let _ = GetWindowRect(hwnd, &mut r); }
            glide(hwnd, (r.left, r.top), px(8, scale), false);
        }
    }
    unsafe { let _ = ShowWindow(hwnd, SW_HIDE); }
    with(|s| { s.visible = false; s.last_tick = None; });
    publish();
}

fn advance(hwnd: HWND, expected: &BalloonRequest, event: Option<u32>) {
    let Some((completed, next)) = tray::finish_balloon_if(expected) else {
        if tray::current_balloon().is_some() { begin(hwnd); }
        return;
    };
    if let Some(event) = event { tray::notify_balloon_owner(&completed, event); }
    hide(hwnd);
    if next.is_some() { start(); } else { unsafe { let _ = KillTimer(hwnd, TICK); } }
}

fn show(hwnd: HWND) -> bool {
    let Some((x, y, w, h, theme, scale)) = with(|s| {
        let size = (px(360, s.scale), px(128, s.scale));
        let mon = unsafe { MonitorFromWindow(s.owner, MONITOR_DEFAULTTOPRIMARY) };
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !unsafe { GetMonitorInfoW(mon, &mut info) }.as_bool() { return None; }
        let margin = px(16, s.scale);
        let x = info.rcWork.right - margin - size.0;
        let y = info.rcWork.bottom - margin - size.1;
        s.rect = R { x: 0, y: 0, w: size.0, h: size.1 };
        Some((x, y, size.0, size.1, s.theme.clone(), s.scale))
    }).flatten() else { return false };
    let motion = crate::dock::motion_enabled(&theme);
    let off = if motion { px(12, scale) } else { 0 };
    if !render(hwnd, x, y + off, w, h, &theme, scale, if motion { 0 } else { 255 }) { return false; }
    if motion { glide(hwnd, (x, y), off, true); }
    with(|s| s.visible = true);
    publish();
    true
}

fn render(hwnd: HWND, x: i32, y: i32, w: i32, h: i32, theme: &Theme, scale: f32, alpha: u8) -> bool {
    let Some(request) = with(|s| s.request.clone()).flatten() else { return false };
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(screen);
        let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: 0, ..Default::default() }, ..Default::default() };
        let mut bits = std::ptr::null_mut();
        let Ok(dib) = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else { let _ = DeleteDC(mem); ReleaseDC(None, screen); return false };
        let old = SelectObject(mem, dib);
        let buf = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
        buf.fill(0);
        fill_round_a(buf, w, h, R { x: 0, y: 0, w, h }, px(12, scale), rgb(&theme.background), 0.98);
        fill_round_a(buf, w, h, R { x: 0, y: 0, w: px(4, scale), h }, px(2, scale), rgb(&theme.accent), 1.0);
        let icon_rect = R { x: px(18, scale), y: px(18, scale), w: px(36, scale), h: px(36, scale) };
        if !request.title.is_empty() {
            if let Some((iw, ih, pixels)) = request.icon.as_ref() {
                let side = icon_rect.w.min(icon_rect.h);
                for (i, pixel) in crate::tray_flyout::scaled(pixels, (*iw, *ih), side).into_iter().enumerate() {
                    let (ix, iy) = (icon_rect.x + i as i32 % side, icon_rect.y + i as i32 / side);
                    blend(&mut buf[(iy * w + ix) as usize], pixel);
                }
            } else {
                let (glyph, color) = match request.flags & NIIF_ICON_MASK {
                    NIIF_WARNING => ("\u{E7BA}", (255, 190, 70)),
                    NIIF_ERROR => ("\u{EA39}", (255, 100, 100)),
                    _ => ("\u{E946}", rgb(&theme.accent)),
                };
                draw_label(buf, w, h, icon_rect, glyph, ICON_FONT, scale, color, 24);
            }
        }
        let text_x = if request.title.is_empty() { px(18, scale) } else { px(64, scale) };
        let close = R { x: w - px(34, scale), y: px(8, scale), w: px(26, scale), h: px(26, scale) };
        draw_label(buf, w, h, close, CLOSE_GLYPH, ICON_FONT, scale, rgb(&theme.muted), 13);
        if !request.title.is_empty() {
            draw_text(buf, w, h, R { x: text_x, y: px(16, scale), w: w - text_x - px(44, scale), h: px(25, scale) }, &request.title, &theme.font_family, scale, rgb(&theme.foreground), (theme.font_size + 1).max(12), DT_LEFT);
        }
        draw_paragraph(buf, w, h, R { x: text_x, y: px(if request.title.is_empty() { 18 } else { 44 }, scale), w: w - text_x - px(20, scale), h: h - px(56, scale) }, &request.text, &theme.font_family, scale, rgb(&theme.foreground), theme.font_size);
        let size = SIZE { cx: w, cy: h };
        let pos = POINT { x, y };
        let src = POINT::default();
        let blend = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: alpha, AlphaFormat: 1 };
        let updated = UpdateLayeredWindow(hwnd, None, Some(&pos), Some(&size), mem, Some(&src), COLORREF(0), Some(&blend), ULW_ALPHA).is_ok();
        if updated {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE);
        }
        let _ = SelectObject(mem, old);
        let _ = DeleteObject(dib);
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        updated
    }
}

unsafe fn draw_paragraph(buf: &mut [u32], stride: i32, rows: i32, rect: R, text: &str, family: &str, scale: f32, fg: (u8, u8, u8), font_size: i32) {
    let wide: Vec<u16> = text.encode_utf16().collect();
    let family: Vec<u16> = family.encode_utf16().chain(Some(0)).collect();
    let screen = GetDC(None);
    let mem = CreateCompatibleDC(screen);
    let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: rect.w, biHeight: -rect.h, biPlanes: 1, biBitCount: 32, biCompression: 0, ..Default::default() }, ..Default::default() };
    let mut bits = std::ptr::null_mut();
    if let Ok(dib) = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
        let old = SelectObject(mem, dib);
        let font = CreateFontW(-px(font_size.max(11), scale), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, ANTIALIASED_QUALITY.0 as u32, 0, PCWSTR(family.as_ptr()));
        let old_font = SelectObject(mem, font);
        let src = std::slice::from_raw_parts_mut(bits as *mut u32, (rect.w * rect.h) as usize);
        src.fill(0);
        SetBkMode(mem, TRANSPARENT);
        SetTextColor(mem, COLORREF(0x00FF_FFFF));
        let mut copy = wide.clone();
        let mut target = RECT { left: 0, top: 0, right: rect.w, bottom: rect.h };
        DrawTextW(mem, &mut copy, &mut target, DT_LEFT | DT_TOP | DT_NOPREFIX | DT_WORDBREAK | DT_END_ELLIPSIS);
        for y in 0..rect.h {
            for x in 0..rect.w {
                let p = src[(y * rect.w + x) as usize];
                let coverage = ((p >> 16) & 255).max((p >> 8) & 255).max(p & 255);
                let (dx, dy) = (rect.x + x, rect.y + y);
                if coverage > 0 && dx >= 0 && dy >= 0 && dx < stride && dy < rows {
                    blend(&mut buf[(dy * stride + dx) as usize], ((coverage * fg.0 as u32 / 255) << 16) | ((coverage * fg.1 as u32 / 255) << 8) | (coverage * fg.2 as u32 / 255) | (coverage << 24));
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

fn publish() {
    let Some((hwnd, scale, size, generation, visible, request)) = with(|s| (
        s.hwnd, s.scale, (px(360, s.scale), px(128, s.scale)), s.generation, s.visible, s.request.clone()
    )) else { return };
    let nodes = if visible {
        request.map(|request| {
            let body_key = format!("notification-body-{generation}");
            let close_key = format!("notification-close-{generation}");
            let mut body = accesskit::Node::new(accesskit::Role::Button);
            body.set_label(if request.title.is_empty() { request.text } else { format!("{}\n{}", request.title, request.text) });
            body.set_bounds(accesskit::Rect::new(0.0, 0.0, size.0 as f64, size.1 as f64));
            body.add_action(accesskit::Action::Click);
            let mut close = accesskit::Node::new(accesskit::Role::Button);
            close.set_label(crate::i18n::t(crate::i18n::K::NotificationDismiss));
            close.set_bounds(accesskit::Rect::new((size.0 - px(34, scale)) as f64, px(8, scale) as f64, px(26, scale) as f64, px(26, scale) as f64));
            close.add_action(accesskit::Action::Click);
            vec![(body_key, body), (close_key, close)]
        }).unwrap_or_default()
    } else { Vec::new() };
    crate::accessibility::publish(hwnd, "Winarchy notification", size, nodes, None);
}

fn dismiss_current(hwnd: HWND) {
    if let Some(request) = with(|s| s.visible.then(|| s.request.clone()).flatten()).flatten() {
        advance(hwnd, &request, Some(NIN_BALLOONTIMEOUT));
    }
}

fn click() {
    if let Some(request) = with(|s| s.visible.then(|| s.request.clone()).flatten()).flatten() {
        advance(with(|s| s.hwnd).unwrap_or_default(), &request, Some(NIN_BALLOONUSERCLICK));
    }
}

fn action_key_matches(key: &str, kind: &str, generation: u64) -> bool {
    key == format!("notification-{kind}-{generation}")
}

fn dispatch_action(hwnd: HWND, key: &str, action: accesskit::Action) {
    if action != accesskit::Action::Click { return; }
    let Some(generation) = with(|s| s.visible.then_some(s.generation)).flatten() else { return };
    if action_key_matches(key, "close", generation) { dismiss_current(hwnd); }
    else if action_key_matches(key, "body", generation) { click(); }
}

unsafe extern "system" fn proc(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if message == WM_GETOBJECT {
        if let Some(r) = crate::accessibility::get_object(hwnd, wp, lp) { return r; }
    }
    match message {
        WM_NCDESTROY => { crate::accessibility::destroy(hwnd); HWND_ATOMIC.store(0, Ordering::Relaxed); }
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        WM_TIMER if wp.0 == TICK => { tick(hwnd); return LRESULT(0); }
        WM_START => { begin(hwnd); return LRESULT(0); }
        WM_DISMISS => { hide(hwnd); let _ = KillTimer(hwnd, TICK); return LRESULT(0); }
        WM_THEME => {
            if apply_theme_update().unwrap_or(false) { show(hwnd); }
            return LRESULT(0);
        }
        WM_LBUTTONUP => {
            let point = POINT { x: (lp.0 as u16 as i16) as i32, y: ((lp.0 >> 16) as u16 as i16) as i32 };
            let close = with(|s| s.visible && point.x >= s.rect.w - px(34, s.scale) && point.x < s.rect.w - px(8, s.scale) && point.y >= px(8, s.scale) && point.y < px(34, s.scale)).unwrap_or(false);
            if close { dismiss_current(hwnd); } else { click(); }
            return LRESULT(0);
        }
        m if m == crate::accessibility::WM_ACTION => {
            for (key, action) in crate::accessibility::actions(hwnd, wp.0) { dispatch_action(hwnd, &key, action); }
            return LRESULT(0);
        }
        _ => {}
    }
    DefWindowProcW(hwnd, message, wp, lp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_availability_respects_quiet_time_and_realtime() {
        assert_eq!(availability_for_state(Some(QUNS_QUIET_TIME), NIIF_RESPECT_QUIET_TIME, false, false), Availability::Hide);
        assert_eq!(availability_for_state(Some(QUNS_QUIET_TIME), 0, false, false), Availability::Wait);
        assert_eq!(availability_for_state(Some(2), 0, true, false), Availability::Discard);
        assert_eq!(availability_for_state(Some(2), 0, true, true), Availability::Wait);
        assert_eq!(availability_for_state(Some(2), 0x40, false, false), Availability::Wait);
        assert_eq!(availability_for_state(Some(QUNS_ACCEPTS_NOTIFICATIONS), 0, true, false), Availability::Show);
        assert_eq!(availability_for_state(Some(QUNS_UNKNOWN), 0, true, false), Availability::Show);
        assert_eq!(availability_for_state(None, 0, false, false), Availability::Show);
    }

    #[test]
    fn theme_updates_apply_the_latest_theme_and_report_visibility() {
        let mut theme: Theme = serde_json::from_str(r##"{"background":"#000000","opacity":1.0,"foreground":"#ffffff","accent":"#111111","muted":"#222222","font_family":"A","font_size":12,"position":"top"}"##).unwrap();
        let previous = SURFACE.with(|s| s.replace(Some(Surface {
            hwnd: HWND::default(), owner: HWND::default(), theme: theme.clone(), scale: 1.0,
            request: None, generation: 0, visible: false, ever_shown: false,
            remaining: None, last_tick: None, rect: R::default(),
        })));
        theme.accent = "#333333".into();
        *THEME_UPDATE.lock().unwrap() = Some(theme.clone());

        assert_eq!(apply_theme_update(), Some(false));
        assert_eq!(with(|s| s.theme.accent.clone()).as_deref(), Some("#333333"));

        with(|s| s.visible = true);
        theme.accent = "#444444".into();
        *THEME_UPDATE.lock().unwrap() = Some(theme.clone());
        theme.accent = "#555555".into();
        *THEME_UPDATE.lock().unwrap() = Some(theme);

        assert_eq!(apply_theme_update(), Some(true));
        assert_eq!(with(|s| s.theme.accent.clone()).as_deref(), Some("#555555"));
        assert!(THEME_UPDATE.lock().unwrap().is_none());
        assert_eq!(apply_theme_update(), None);
        SURFACE.with(|s| s.replace(previous));
    }

    #[test]
    fn resolved_action_batch_cannot_target_the_next_notification() {
        let current_generation = 2;
        for (key, kind) in [("notification-body-1", "body"), ("notification-close-1", "close")] {
            assert!(!action_key_matches(key, kind, current_generation));
        }
        assert!(action_key_matches("notification-body-2", "body", current_generation));
        assert!(action_key_matches("notification-close-2", "close", current_generation));
    }

    #[test]
    fn balloon_notification_flags_are_not_the_deprecated_display_timeout() {
        let flags = NIIF_INFO | NIIF_NOSOUND | NIIF_RESPECT_QUIET_TIME;
        assert_eq!(flags & NIIF_ICON_MASK, NIIF_INFO);
        assert_ne!(flags & NIIF_NOSOUND, 0);
        assert_ne!(flags & NIIF_RESPECT_QUIET_TIME, 0);
    }
}
