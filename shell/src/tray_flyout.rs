// Flyout de la bandeja del sistema: reemplaza al overflow nativo con los íconos reales (los entrega `tray`).
// Se abre desde el botón de bandeja del dock; sin host activo el dock sigue abriendo la bandeja nativa.
use crate::dock::{approach, blend, draw_label, ease, fill_round_a, style, HOVER_MS, MARGIN_DIP};
use crate::i18n::{t, K};
use crate::layout::{px, R};
use crate::theme::{rgb, Theme};
use crate::tray::{self, Gesture};
use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_LBUTTON, VK_RBUTTON};
use windows::Win32::UI::WindowsAndMessaging::*;

const CELL_DIP: i32 = 40;
const PIN_DIP: i32 = 16;
const PIN_GLYPH: &str = "\u{E718}";
const PIN_FILL_GLYPH: &str = "\u{E840}";
const ICONS_FONT: &str = "Segoe MDL2 Assets";
const ICON_DIP: i32 = 22;
const PAD_DIP: i32 = 8;
const TIP_DIP: i32 = 26;
const COLS_MAX: usize = 6;
const SLIDE_DIP: i32 = 8;
const OPEN_MS: f32 = 180.0;
const ANIM_MS: u32 = 16;
const WATCH_MS: u32 = 40;
const T_ANIM: usize = 1;
const T_WATCH: usize = 2;
const MOUSE_LEAVE: u32 = 0x02A3;
const WM_SYNC: u32 = WM_APP + 41;

pub struct Grid {
    pub width: i32,
    pub height: i32,
    pub cells: Vec<R>,
    pub tip: R,
}

pub fn grid(n: usize, scale: f32) -> Grid {
    let (cell, pad, tip) = (px(CELL_DIP, scale), px(PAD_DIP, scale), px(TIP_DIP, scale));
    let cols = n.clamp(1, COLS_MAX);
    let rows = n.max(1).div_ceil(cols);
    let width = 2 * pad + cols as i32 * cell;
    let cells = (0..n).map(|i| R { x: pad + (i % cols) as i32 * cell, y: pad + (i / cols) as i32 * cell, w: cell, h: cell }).collect();
    let height = 2 * pad + rows as i32 * cell + tip;
    Grid { width, height, cells, tip: R { x: pad, y: height - pad - tip, w: width - 2 * pad, h: tip } }
}

// El marcador de fijado vive en la esquina superior derecha de cada celda.
pub fn pin_rect(cell: R, scale: f32) -> R {
    let side = px(PIN_DIP, scale);
    R { x: cell.x + cell.w - side - px(1, scale), y: cell.y + px(1, scale), w: side, h: side }
}

fn inside(r: R, x: i32, y: i32) -> bool {
    x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h
}

// Encima del botón de bandeja (o debajo si el dock está arriba), centrado en él y sin salirse del monitor.
pub fn place(anchor: R, mon: (i32, i32, i32, i32), bottom: bool, size: (i32, i32), scale: f32) -> (i32, i32) {
    let margin = px(MARGIN_DIP, scale);
    let x = (anchor.x + anchor.w / 2 - size.0 / 2).clamp(mon.0 + margin, (mon.2 - margin - size.0).max(mon.0 + margin));
    let y = if bottom { anchor.y - margin - size.1 } else { anchor.y + anchor.h + margin };
    // Con muchas filas el flyout podría no entrar entre el dock y el borde del monitor: se queda dentro.
    (x, y.clamp(mon.1 + margin, (mon.3 - margin - size.1).max(mon.1 + margin)))
}

// Reduce (o amplía) un ícono BGRA con alfa recta a `side`×`side`, ya premultiplicado.
pub(crate) fn scaled(src: &[u8], (w, h): (u32, u32), side: i32) -> Vec<u32> {
    let side = side.max(1) as usize;
    let texel = |x: i32, y: i32| -> [f32; 4] {
        let i = ((y.clamp(0, h as i32 - 1) as usize) * w as usize + x.clamp(0, w as i32 - 1) as usize) * 4;
        [src[i] as f32, src[i + 1] as f32, src[i + 2] as f32, src[i + 3] as f32]
    };
    let ratio = (w as f32 / side as f32, h as f32 / side as f32);
    let mut out = Vec::with_capacity(side * side);
    for y in 0..side {
        for x in 0..side {
            let (fx, fy) = ((x as f32 + 0.5) * ratio.0 - 0.5, (y as f32 + 0.5) * ratio.1 - 0.5);
            let (x0, y0) = (fx.floor() as i32, fy.floor() as i32);
            let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
            let mut acc = [0f32; 4];
            for (dx, dy, wgt) in [(0, 0, (1.0 - tx) * (1.0 - ty)), (1, 0, tx * (1.0 - ty)), (0, 1, (1.0 - tx) * ty), (1, 1, tx * ty)] {
                let t = texel(x0 + dx, y0 + dy);
                let a = t[3] / 255.0;
                for c in 0..3 {
                    acc[c] += t[c] * a * wgt;
                }
                acc[3] += t[3] * wgt;
            }
            let (b, g, r, a) = (acc[0].round() as u32, acc[1].round() as u32, acc[2].round() as u32, acc[3].round() as u32);
            out.push((a << 24) | (r << 16) | (g << 8) | b);
        }
    }
    out
}

struct Flyout {
    hwnd: HWND,
    theme: Theme,
    scale: f32,
    pos: (i32, i32),
    anchor: R,
    mon: (i32, i32, i32, i32),
    bottom: bool,
    open: bool,
    vis: f32,
    hover: Option<usize>,
    hover_pin: bool,
    hover_t: Vec<f32>,
    pressed: Option<usize>,
    tracking: bool,
    last: Option<std::time::Instant>,
    anim_on: bool,
    revision: u64,
    sig: u64,
}

static HWND_ATOMIC: AtomicIsize = AtomicIsize::new(0);
// Ventana en primer plano al abrir: si cambia (clic en otra app, Alt+Tab, tecla Win) el flyout se cierra.
static FG_AT_OPEN: AtomicIsize = AtomicIsize::new(0);
// Hasta este GetTickCount64 el foco lo puede tomar el menú contextual del ícono sin cerrar el flyout.
static FG_GRACE_UNTIL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
const MENU_GRACE_MS: u64 = 1500;
thread_local! { static FLY: RefCell<Option<Flyout>> = const { RefCell::new(None) }; }

fn with<R>(f: impl FnOnce(&mut Flyout) -> R) -> Option<R> {
    FLY.with(|c| c.try_borrow_mut().ok().and_then(|mut c| c.as_mut().map(f)))
}

// Abre (o cierra, si ya estaba abierto) el flyout sobre el botón `anchor`. Falso si no hay host de bandeja: el llamador usa la bandeja nativa.
pub fn toggle(anchor: R, mon: (i32, i32, i32, i32), bottom: bool, scale: f32, theme: &Theme) -> bool {
    if !tray::active() {
        return false;
    }
    if with(|f| f.open).unwrap_or(false) {
        close();
        return true;
    }
    if with(|_| ()).is_none() && !create(theme) {
        return false;
    }
    let started = with(|f| {
        f.theme = theme.clone();
        f.scale = scale;
        f.anchor = anchor;
        f.mon = mon;
        f.bottom = bottom;
        f.open = true;
        f.hover = None;
        f.pressed = None;
        f.sig = 0;
        f.hwnd
    });
    let Some(h) = started else { return false };
    unsafe {
        FG_AT_OPEN.store(GetForegroundWindow().0 as isize, Ordering::Relaxed);
        SetTimer(h, T_WATCH, WATCH_MS, None);
        start_anim(h);
    }
    paint();
    // una ventana activada antes puede ser topmost y quedar encima; solo al abrir, para no tapar el menú de la app durante el fundido de salida
    unsafe {
        let _ = SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
    true
}

pub fn close() {
    if let Some(h) = with(|f| {
        f.open = false;
        f.hwnd
    }) {
        unsafe {
            let _ = KillTimer(h, T_WATCH);
            start_anim(h);
        }
    }
}

fn create(theme: &Theme) -> bool {
    unsafe {
        let Ok(inst) = GetModuleHandleW(PCWSTR::null()) else { return false };
        let class = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: inst.into(), hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(), lpszClassName: w!("WinarchyShellTray"), ..Default::default() };
        RegisterClassW(&class);
        let ex = WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_LAYERED;
        let Ok(hwnd) = CreateWindowExW(ex, w!("WinarchyShellTray"), w!(""), WS_POPUP, 0, 0, 1, 1, None, None, inst, None) else { return false };
        crate::accessibility::create(hwnd, "Winarchy tray", accesskit::Role::Toolbar);
        let f = Flyout { hwnd, theme: theme.clone(), scale: 1.0, pos: (0, 0), anchor: R::default(), mon: (0, 0, 0, 0), bottom: true, open: false, vis: 0.0, hover: None, hover_pin: false, hover_t: Vec::new(), pressed: None, tracking: false, last: None, anim_on: false, revision: 0, sig: 0 };
        FLY.with(|s| *s.borrow_mut() = Some(f));
        HWND_ATOMIC.store(hwnd.0 as isize, Ordering::Relaxed);
        true
    }
}

pub fn notify() {
    let h = HWND_ATOMIC.load(Ordering::Relaxed);
    if h != 0 {
        unsafe {
            let _ = PostMessageW(HWND(h as *mut _), WM_SYNC, WPARAM(0), LPARAM(0));
        }
    }
}

unsafe fn start_anim(h: HWND) {
    if with(|f| std::mem::replace(&mut f.anim_on, true)) == Some(false) {
        SetTimer(h, T_ANIM, ANIM_MS, None);
    }
}

unsafe fn anim_tick(h: HWND) {
    let busy = with(|f| {
        let dt = f.last.map_or(ANIM_MS as f32, |l| l.elapsed().as_secs_f32() * 1000.0).min(64.0);
        f.last = Some(std::time::Instant::now());
        let target = if f.open { 1.0 } else { 0.0 };
        f.vis = approach(f.vis, target, dt, OPEN_MS);
        let mut busy = f.vis != target;
        let hover = f.hover;
        f.hover_t.resize(tray::with_visible(|v| v.len()), 0.0);
        for (i, t) in f.hover_t.iter_mut().enumerate() {
            let goal = if hover == Some(i) { 1.0 } else { 0.0 };
            *t = approach(*t, goal, dt, HOVER_MS);
            busy |= *t != goal;
        }
        busy
    })
    .unwrap_or(false);
    paint();
    if !busy {
        let _ = KillTimer(h, T_ANIM);
        with(|f| {
            f.anim_on = false;
            f.last = None;
        });
    }
}

fn paint() {
    let Some((hwnd, theme, scale, vis, bottom, anchor, mon, hover, hover_pin, hover_t, revision)) = with(|f| (f.hwnd, f.theme.clone(), f.scale, f.vis, f.bottom, f.anchor, f.mon, f.hover, f.hover_pin, f.hover_t.clone(), tray::revision())) else { return };
    let sig = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (vis.to_bits(), hover, hover_pin, hover_t.iter().map(|t| t.to_bits()).collect::<Vec<_>>(), revision, (anchor.x, anchor.y, scale.to_bits()), &theme.background, &theme.foreground).hash(&mut h);
        h.finish()
    };
    if with(|f| std::mem::replace(&mut f.sig, sig) == sig).unwrap_or(true) {
        return;
    }
    with(|f| f.revision = revision);
    unsafe {
        if vis <= 0.0 && !with(|f| f.open).unwrap_or(false) {
            let _ = ShowWindow(hwnd, SW_HIDE);
            return;
        }
        let full = tray::with_pinned(|v| v.len()) >= tray::PINNED_MAX;
        tray::with_visible(|icons| {
            if icons.is_empty() {
                return;
            }
            let g = grid(icons.len(), scale);
            let (x, y) = place(anchor, mon, bottom, (g.width, g.height), scale);
            let slide = ((1.0 - ease(vis)) * px(SLIDE_DIP, scale) as f32) as i32;
            let (w, h) = (g.width, g.height);
            let (bg, fg, muted) = (rgb(&theme.background), rgb(&theme.foreground), rgb(&theme.muted));
            let st = style(&theme);
            let screen = GetDC(None);
            let mem = CreateCompatibleDC(screen);
            let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: 0, ..Default::default() }, ..Default::default() };
            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            if let Ok(dib) = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
                let old = SelectObject(mem, dib);
                let buf = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
                buf.fill(0);
                fill_round_a(buf, w, h, R { x: 0, y: 0, w, h }, px(st.radius.max(10), scale), bg, st.opacity);
                let side = px(ICON_DIP, scale);
                for (i, (icon, cell)) in icons.iter().zip(&g.cells).enumerate() {
                    let t = hover_t.get(i).copied().unwrap_or(0.0);
                    if t > 0.0 {
                        fill_round_a(buf, w, h, *cell, px(10, scale), fg, 0.14 * ease(t));
                    }
                    let lift = (t * px(1, scale) as f32) as i32;
                    let (ox, oy) = (cell.x + (cell.w - side) / 2, cell.y + (cell.h - side) / 2 - lift);
                    for (k, p) in scaled(&icon.bgra, icon.size, side).into_iter().enumerate() {
                        let (px_x, px_y) = (ox + (k as i32 % side), oy + (k as i32 / side));
                        if px_x >= 0 && px_x < w && px_y >= 0 && px_y < h {
                            blend(&mut buf[(px_y * w + px_x) as usize], p);
                        }
                    }
                    let pinned = tray::is_pinned(icon);
                    if pinned || hover == Some(i) {
                        let badge = pin_rect(*cell, scale);
                        let on = hover == Some(i) && hover_pin;
                        if on || pinned {
                            fill_round_a(buf, w, h, badge, badge.w / 2, rgb(&theme.accent), if pinned { 1.0 } else { 0.35 });
                        }
                        let ink = if pinned { crate::theme::contrasting_color(rgb(&theme.accent)) } else { fg };
                        draw_label(buf, w, h, badge, if pinned { PIN_FILL_GLYPH } else { PIN_GLYPH }, ICONS_FONT, scale, ink, 10);
                    }
                }
                let note = hover.and_then(|i| icons.get(i)).map(|i| {
                    if !hover_pin {
                        i.tip.lines().next().unwrap_or_default().to_string()
                    } else if tray::is_pinned(i) {
                        t(K::UnpinFromBar).to_string()
                    } else if full {
                        t(K::BarFull).to_string()
                    } else {
                        t(K::PinToBar).to_string()
                    }
                });
                if let Some(text) = note.filter(|n| !n.is_empty()) {
                    draw_label(buf, w, h, g.tip, &text, &theme.font_family, scale, muted, (theme.font_size - 1).max(10));
                }
                let pos = POINT { x, y: y + if bottom { slide } else { -slide } };
                let sz = SIZE { cx: w, cy: h };
                let src = POINT::default();
                let bf = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: (vis.clamp(0.0, 1.0) * 255.0).round() as u8, AlphaFormat: 1 };
                let _ = UpdateLayeredWindow(hwnd, None, Some(&pos), Some(&sz), mem, Some(&src), COLORREF(0), Some(&bf), ULW_ALPHA);
                with(|f| f.pos = (x, y));
                SelectObject(mem, old);
                let _ = DeleteObject(dib);
            }
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        });
    }
    publish();
}

fn publish() {
    let Some((hwnd, scale)) = with(|f| (f.hwnd, f.scale)) else { return };
    let (nodes, size) = tray::with_visible(|icons| {
        let g = grid(icons.len(), scale);
        let nodes = icons
            .iter()
            .zip(&g.cells)
            .map(|(icon, r)| {
                let mut n = accesskit::Node::new(accesskit::Role::Button);
                n.set_label(if icon.tip.is_empty() { format!("{:#x}", icon.hwnd) } else { icon.tip.clone() });
                n.set_bounds(accesskit::Rect::new(r.x as f64, r.y as f64, (r.x + r.w) as f64, (r.y + r.h) as f64));
                n.add_action(accesskit::Action::Click);
                (format!("{}:{}", icon.hwnd, icon.uid), n)
            })
            .collect::<Vec<_>>();
        (nodes, (g.width, g.height))
    });
    crate::accessibility::publish(hwnd, "Winarchy tray", size, nodes, None);
}

fn cell_at(x: i32, y: i32) -> Option<usize> {
    with(|f| {
        let g = grid(tray::with_visible(|v| v.len()), f.scale);
        g.cells.iter().position(|c| x >= c.x && x < c.x + c.w && y >= c.y && y < c.y + c.h)
    })
    .flatten()
}

fn icon_at(index: usize) -> Option<(u32, u32, [u8; 16])> {
    tray::with_visible(|v| v.get(index).map(|i| (i.hwnd, i.uid, i.guid)))
}

fn on_pin(index: usize, x: i32, y: i32) -> bool {
    with(|f| tray::with_visible(|v| grid(v.len(), f.scale).cells.get(index).is_some_and(|c| inside(pin_rect(*c, f.scale), x, y)))).unwrap_or(false)
}

fn client(lp: LPARAM) -> (i32, i32) {
    ((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32)
}

fn send(index: usize, gesture: Gesture) {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    if let Some((hwnd, uid, guid)) = icon_at(index) {
        if gesture != Gesture::Left {
            FG_GRACE_UNTIL.store(unsafe { windows::Win32::System::SystemInformation::GetTickCount64() } + MENU_GRACE_MS, Ordering::Relaxed);
        }
        tray::click_identity(hwnd, uid, guid, gesture, (p.x, p.y));
    }
}

// Un clic fuera del flyout (lo que haya debajo del cursor, menús contextuales incluidos) lo cierra; sin hooks de mouse.
unsafe fn watch(h: HWND) {
    let fg = GetForegroundWindow();
    let baseline = FG_AT_OPEN.load(Ordering::Relaxed);
    if baseline == 0 {
        // abierto en una transición sin ventana activa: el primer primer plano real es la referencia
        FG_AT_OPEN.store(fg.0 as isize, Ordering::Relaxed);
    } else if fg.0 as isize != 0 && fg.0 as isize != baseline && GetAncestor(fg, GA_ROOT) != h && windows::Win32::System::SystemInformation::GetTickCount64() >= FG_GRACE_UNTIL.load(Ordering::Relaxed) {
        close();
        return;
    }
    let down = GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0 || GetAsyncKeyState(VK_RBUTTON.0 as i32) < 0;
    let mut p = POINT::default();
    if !down || GetCursorPos(&mut p).is_err() {
        return;
    }
    let a = with(|f| f.anchor).unwrap_or_default();
    let on_anchor = p.x >= a.x && p.x < a.x + a.w && p.y >= a.y && p.y < a.y + a.h;
    // Sobre el botón que lo abrió el cierre lo decide `toggle`; si no, soltar el clic lo reabriría.
    if !on_anchor && GetAncestor(WindowFromPoint(p), GA_ROOT) != h {
        close();
    }
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
        WM_TIMER if wp.0 == T_ANIM => {
            anim_tick(h);
            return LRESULT(0);
        }
        WM_TIMER if wp.0 == T_WATCH => {
            watch(h);
            return LRESULT(0);
        }
        WM_SYNC => {
            if with(|f| f.open).unwrap_or(false) && tray::with_visible(|v| v.is_empty()) {
                close();
            }
            start_anim(h);
            return LRESULT(0);
        }
        WM_MOUSEMOVE => {
            let (x, y) = client(lp);
            let cell = cell_at(x, y);
            let on_pin = cell.is_some_and(|i| on_pin(i, x, y));
            if with(|f| std::mem::replace(&mut f.hover, cell)) != Some(cell) || with(|f| std::mem::replace(&mut f.hover_pin, on_pin)) != Some(on_pin) {
                start_anim(h);
            }
            if with(|f| std::mem::replace(&mut f.tracking, true)) == Some(false) {
                let _ = TrackMouseEvent(&mut TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: h, dwHoverTime: 0 });
            }
            return LRESULT(0);
        }
        MOUSE_LEAVE => {
            with(|f| {
                f.hover = None;
                f.hover_pin = false;
                f.tracking = false;
                f.pressed = None;
            });
            start_anim(h);
            return LRESULT(0);
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
            let (x, y) = client(lp);
            let cell = cell_at(x, y);
            with(|f| f.pressed = cell);
            return LRESULT(0);
        }
        WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => {
            let (x, y) = client(lp);
            let down = with(|f| f.pressed.take()).flatten();
            if let (Some(d), Some(u)) = (down, cell_at(x, y)) {
                if d == u && m == WM_LBUTTONUP && on_pin(u, x, y) {
                    if let Some(icon) = tray::with_visible(|v| v.get(u).map(|i| (*i).clone())) {
                        tray::toggle_pin(&icon);
                    }
                    start_anim(h);
                } else if d == u {
                    send(u, match m {
                        WM_LBUTTONUP => Gesture::Left,
                        WM_RBUTTONUP => Gesture::Right,
                        _ => Gesture::Middle,
                    });
                    if m == WM_LBUTTONUP {
                        close();
                    }
                }
            }
            return LRESULT(0);
        }
        m if m == crate::accessibility::WM_ACTION => {
            for (key, action) in crate::accessibility::actions(h, wp.0) {
                if action == accesskit::Action::Click {
                    let index = tray::with_visible(|v| v.iter().position(|i| format!("{}:{}", i.hwnd, i.uid) == key));
                    if let Some(i) = index {
                        send(i, Gesture::Left);
                        close();
                    }
                }
            }
            return LRESULT(0);
        }
        _ => {}
    }
    DefWindowProcW(h, m, wp, lp)
}

#[cfg(test)]
#[path = "tray_flyout_tests.rs"]
mod tests;
