// Menú contextual propio y modal: reemplaza al TrackPopupMenu del sistema para que todos los menús lleven el theme, el movimiento y el teclado del dock.
use crate::dock::{draw_text, ease, fill_round_a, motion_enabled};
use crate::layout::{px, R};
use crate::theme::{rgb, Theme};
use std::cell::RefCell;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_DOWN, VK_ESCAPE, VK_RETURN, VK_UP};
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Anchor {
    BottomLeft,
    BottomRight,
    TopLeft,
}

// Cabecera opcional sobre las filas: una línea de estado, o un deslizador (etiqueta, valor 0..1) cuyo cambio se informa por `on_slide`.
#[derive(Clone)]
pub enum Header {
    None,
    Text(String),
    Slider(String, f32),
}

const OPEN_MS: f32 = 140.0;
const T_OPEN: usize = 1;
const WM_MOUSELEAVE_MSG: u32 = 0x02A3;

struct St {
    hwnd: HWND,
    labels: Vec<String>,
    header: Header,
    on_slide: Option<fn(f32)>,
    dragging: bool,
    hover: Option<usize>,
    done: Option<Option<usize>>,
    size: (i32, i32),
    pos: (i32, i32),
    theme: Theme,
    scale: f32,
    born: std::time::Instant,
    animate: bool,
    tracking: bool,
}

thread_local! {
    static ST: RefCell<Option<St>> = const { RefCell::new(None) };
}

fn row_h(scale: f32) -> i32 {
    px(30, scale)
}

fn pad(scale: f32) -> i32 {
    px(6, scale)
}

fn header_h(h: &Header, scale: f32) -> i32 {
    match h {
        Header::None => 0,
        Header::Text(_) => px(30, scale),
        Header::Slider(..) => px(62, scale),
    }
}

// Valor 0..1 del deslizador para una x del cliente; el riel va de `padx` a `w - padx`.
pub fn slider_value(x: i32, w: i32, padx: i32) -> f32 {
    ((x - padx) as f32 / (w - 2 * padx).max(1) as f32).clamp(0.0, 1.0)
}

// Fila bajo `y` (relativo a la ventana), o None en el relleno.
pub fn row_at(y: i32, pad: i32, row: i32, n: usize) -> Option<usize> {
    let r = y - pad;
    (r >= 0 && r < row * n as i32).then_some((r / row) as usize)
}

// Esquina superior izquierda para que el menú quede pegado a `at` según el ancla y dentro del monitor `mon` (l, t, r, b).
pub fn place(at: (i32, i32), size: (i32, i32), anchor: Anchor, mon: (i32, i32, i32, i32)) -> (i32, i32) {
    let x = match anchor {
        Anchor::BottomLeft | Anchor::TopLeft => at.0,
        Anchor::BottomRight => at.0 - size.0,
    };
    let y = if anchor == Anchor::TopLeft { at.1 } else { at.1 - size.1 };
    (x.clamp(mon.0, (mon.2 - size.0).max(mon.0)), y.clamp(mon.1, (mon.3 - size.1).max(mon.1)))
}

unsafe fn measure(labels: &[String], theme: &Theme, scale: f32) -> i32 {
    let family: Vec<u16> = theme.font_family.encode_utf16().chain(std::iter::once(0)).collect();
    let screen = GetDC(None);
    let mem = CreateCompatibleDC(screen);
    let font = CreateFontW(-px(font_px(theme), scale), 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, ANTIALIASED_QUALITY.0 as u32, 0, PCWSTR(family.as_ptr()));
    let old = SelectObject(mem, font);
    let mut widest = px(150, scale);
    for l in labels {
        let wide: Vec<u16> = l.encode_utf16().collect();
        let mut e = SIZE::default();
        let _ = GetTextExtentPoint32W(mem, &wide, &mut e);
        widest = widest.max(e.cx + 2 * px(14, scale));
    }
    SelectObject(mem, old);
    let _ = DeleteObject(font);
    let _ = DeleteDC(mem);
    ReleaseDC(None, screen);
    widest
}

fn font_px(theme: &Theme) -> i32 {
    (theme.font_size - 1).max(11)
}

// Abre el menú en `at` y espera la elección: devuelve el índice de la fila o None si se descartó (Esc, clic afuera, pérdida de foco).
pub unsafe fn pick(labels: &[String], at: (i32, i32), anchor: Anchor, theme: &Theme, scale: f32) -> Option<usize> {
    pick_with(Header::None, None, labels, at, anchor, theme, scale)
}

pub unsafe fn pick_with(header: Header, on_slide: Option<fn(f32)>, labels: &[String], at: (i32, i32), anchor: Anchor, theme: &Theme, scale: f32) -> Option<usize> {
    if labels.is_empty() && matches!(header, Header::None) {
        return None;
    }
    let (row, pad) = (row_h(scale), pad(scale));
    let width = if matches!(header, Header::Slider(..)) { measure(labels, theme, scale).max(px(240, scale)) } else { measure(labels, theme, scale) };
    let size = (width, header_h(&header, scale) + row * labels.len() as i32 + 2 * pad);
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    let hmon = MonitorFromPoint(POINT { x: at.0, y: at.1 }, MONITOR_DEFAULTTONEAREST);
    let _ = GetMonitorInfoW(hmon, &mut mi);
    let r = mi.rcMonitor;
    let pos = place(at, size, anchor, (r.left, r.top, r.right, r.bottom));
    let Ok(inst) = GetModuleHandleW(PCWSTR::null()) else { return None };
    let class = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: inst.into(), hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(), lpszClassName: w!("WinarchyShellMenu"), ..Default::default() };
    RegisterClassW(&class);
    let Ok(hwnd) = CreateWindowExW(WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED, class.lpszClassName, w!(""), WS_POPUP, pos.0, pos.1, size.0, size.1, None, None, inst, None) else { return None };
    let animate = motion_enabled(theme);
    ST.with(|s| *s.borrow_mut() = Some(St { hwnd, labels: labels.to_vec(), header, on_slide, dragging: false, hover: None, done: None, size, pos, theme: theme.clone(), scale, born: std::time::Instant::now(), animate, tracking: false }));
    paint();
    if animate {
        SetTimer(hwnd, T_OPEN, 16, None);
    }
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
    let _ = SetFocus(hwnd);
    let mut msg = MSG::default();
    while ST.with(|s| s.borrow().as_ref().is_some_and(|s| s.done.is_none())) {
        match GetMessageW(&mut msg, None, 0, 0).0 {
            0 => {
                // WM_QUIT pertenece al bucle de afuera: se devuelve y el menú se descarta.
                PostQuitMessage(msg.wParam.0 as i32);
                break;
            }
            -1 => break,
            _ => {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
    let chosen = ST.with(|s| s.borrow_mut().take()).and_then(|s| s.done.flatten());
    let _ = DestroyWindow(hwnd);
    chosen
}

fn finish(choice: Option<usize>) {
    ST.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut() {
            s.done.get_or_insert(choice);
        }
    });
}

fn set_hover(i: Option<usize>) {
    let changed = ST.with(|s| s.borrow_mut().as_mut().is_some_and(|s| std::mem::replace(&mut s.hover, i) != i));
    if changed {
        unsafe { paint() };
    }
}

fn progress(s: &St) -> f32 {
    if s.animate { (s.born.elapsed().as_secs_f32() * 1000.0 / OPEN_MS).min(1.0) } else { 1.0 }
}

unsafe fn compose(buf: &mut [u32], w: i32, h: i32, header: &Header, labels: &[String], hover: Option<usize>, theme: &Theme, scale: f32) {
    buf.fill(0);
    let (bg, fg, accent) = (rgb(&theme.background), rgb(&theme.foreground), rgb(&theme.accent));
    fill_round_a(buf, w, h, R { x: 0, y: 0, w, h }, px(12, scale), bg, 0.97);
    let (row, pad, padx) = (row_h(scale), pad(scale), px(14, scale));
    let top = pad + header_h(header, scale);
    let muted = rgb(&theme.muted);
    match header {
        Header::None => {}
        Header::Text(t) => draw_text(buf, w, h, R { x: padx, y: pad, w: w - 2 * padx, h: px(24, scale) }, t, &theme.font_family, scale, muted, font_px(theme), DT_LEFT),
        Header::Slider(label, v) => {
            let line = R { x: padx, y: pad, w: w - 2 * padx, h: px(28, scale) };
            draw_text(buf, w, h, line, label, &theme.font_family, scale, fg, font_px(theme), DT_LEFT);
            draw_text(buf, w, h, line, &format!("{}%", (v * 100.0).round() as i32), &theme.font_family, scale, accent, font_px(theme), DT_RIGHT);
            let (track_h, knob) = (px(6, scale), px(16, scale));
            let cy = pad + px(28, scale) + px(16, scale);
            let rail = R { x: padx, y: cy - track_h / 2, w: w - 2 * padx, h: track_h };
            fill_round_a(buf, w, h, rail, track_h / 2, fg, 0.18);
            let fill_w = ((rail.w as f32) * v).round() as i32;
            if fill_w > 0 {
                fill_round_a(buf, w, h, R { w: fill_w.max(track_h), ..rail }, track_h / 2, accent, 1.0);
            }
            fill_round_a(buf, w, h, R { x: padx + fill_w - knob / 2, y: cy - knob / 2, w: knob, h: knob }, knob / 2, fg, 1.0);
        }
    }
    if let Some(i) = hover {
        let inset = px(5, scale);
        fill_round_a(buf, w, h, R { x: inset, y: top + i as i32 * row, w: w - 2 * inset, h: row }, px(8, scale), accent, 0.30);
    }
    for (i, l) in labels.iter().enumerate() {
        draw_text(buf, w, h, R { x: padx, y: top + i as i32 * row, w: w - 2 * padx, h: row }, l, &theme.font_family, scale, fg, font_px(theme), DT_LEFT);
    }
}

unsafe fn paint() {
    let Some((hwnd, labels, header, hover, (w, h), pos, theme, scale, e)) = ST.with(|s| s.borrow().as_ref().map(|s| (s.hwnd, s.labels.clone(), s.header.clone(), s.hover, s.size, s.pos, s.theme.clone(), s.scale, ease(progress(s))))) else { return };
    let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: 0, ..Default::default() }, ..Default::default() };
    let screen = GetDC(None);
    let mem = CreateCompatibleDC(screen);
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    if let Ok(dib) = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
        let old = SelectObject(mem, dib);
        let buf = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
        compose(buf, w, h, &header, &labels, hover, &theme, scale);
        let (sz, src) = (SIZE { cx: w, cy: h }, POINT::default());
        let bf = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: (e * 255.0).round() as u8, AlphaFormat: 1 };
        let off = ((1.0 - e) * px(8, scale) as f32) as i32;
        let p = POINT { x: pos.0, y: pos.1 + off };
        let _ = UpdateLayeredWindow(hwnd, None, Some(&p), Some(&sz), mem, Some(&src), COLORREF(0), Some(&bf), ULW_ALPHA);
        SelectObject(mem, old);
        let _ = DeleteObject(dib);
    }
    let _ = DeleteDC(mem);
    ReleaseDC(None, screen);
}

fn row_from_y(y: i32) -> Option<usize> {
    ST.with(|s| s.borrow().as_ref().and_then(|s| row_at(y - header_h(&s.header, s.scale), pad(s.scale), row_h(s.scale), s.labels.len())))
}

fn slide(x: i32) {
    let v = ST.with(|s| s.borrow().as_ref().map(|s| slider_value(x, s.size.0, px(14, s.scale))));
    if let Some(v) = v {
        apply_slide(v);
    }
}

fn apply_slide(v: f32) {
    let cb = ST.with(|s| {
        let mut s = s.borrow_mut();
        let s = s.as_mut()?;
        if let Header::Slider(label, _) = &s.header {
            s.header = Header::Slider(label.clone(), v);
        }
        s.on_slide
    });
    if let Some(f) = cb {
        f(v);
    }
    unsafe { paint() };
}

fn step(delta: i32) {
    let next = ST.with(|s| {
        let s = s.borrow();
        let s = s.as_ref()?;
        let n = s.labels.len() as i32;
        Some(match s.hover {
            Some(i) => (i as i32 + delta).rem_euclid(n) as usize,
            None if delta > 0 => 0,
            None => (n - 1) as usize,
        })
    });
    set_hover(next);
}

unsafe extern "system" fn proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let y = (lp.0 >> 16) as i16 as i32;
    match m {
        WM_MOUSEMOVE if ST.with(|s| s.borrow().as_ref().is_some_and(|s| s.dragging)) => {
            slide((lp.0 & 0xFFFF) as i16 as i32);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let need = ST.with(|s| s.borrow_mut().as_mut().is_some_and(|s| !std::mem::replace(&mut s.tracking, true)));
            if need {
                let _ = TrackMouseEvent(&mut TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: h, dwHoverTime: 0 });
            }
            set_hover(row_from_y(y));
            LRESULT(0)
        }
        WM_MOUSELEAVE_MSG => {
            ST.with(|s| s.borrow_mut().as_mut().map(|s| s.tracking = false));
            set_hover(None);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let on_slider = ST.with(|s| s.borrow().as_ref().is_some_and(|s| matches!(s.header, Header::Slider(..)) && y >= pad(s.scale) + px(28, s.scale) && y < header_h(&s.header, s.scale) + pad(s.scale)));
            if on_slider {
                ST.with(|s| s.borrow_mut().as_mut().map(|s| s.dragging = true));
                SetCapture(h);
                slide((lp.0 & 0xFFFF) as i16 as i32);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP | WM_RBUTTONUP => {
            if ST.with(|s| s.borrow_mut().as_mut().is_some_and(|s| std::mem::replace(&mut s.dragging, false))) {
                let _ = ReleaseCapture();
            } else {
                finish(row_from_y(y));
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = (wp.0 >> 16) as i16 as i32;
            let cur = ST.with(|s| s.borrow().as_ref().and_then(|s| if let Header::Slider(_, v) = s.header { Some(v) } else { None }));
            if let Some(v) = cur {
                apply_slide((v + delta.signum() as f32 * 0.02).clamp(0.0, 1.0));
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            match wp.0 as u16 {
                k if k == VK_ESCAPE.0 => finish(None),
                k if k == VK_DOWN.0 => step(1),
                k if k == VK_UP.0 => step(-1),
                k if k == VK_RETURN.0 => finish(ST.with(|s| s.borrow().as_ref().and_then(|s| s.hover))),
                _ => {}
            }
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_OPEN => {
            paint();
            if ST.with(|s| s.borrow().as_ref().is_none_or(|s| progress(s) >= 1.0)) {
                let _ = KillTimer(h, T_OPEN);
            }
            LRESULT(0)
        }
        WM_ACTIVATE if wp.0 & 0xFFFF == 0 => {
            finish(None);
            LRESULT(0)
        }
        WM_KILLFOCUS => {
            finish(None);
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_map_from_the_pointer_and_skip_the_padding() {
        assert_eq!(row_at(5, 6, 30, 3), None);
        assert_eq!(row_at(6, 6, 30, 3), Some(0));
        assert_eq!(row_at(6 + 59, 6, 30, 3), Some(1));
        assert_eq!(row_at(6 + 90, 6, 30, 3), None);
    }

    // Vista previa a disco del panel de volumen con el theme real: `cargo test render_preview -- --ignored`.
    #[test]
    #[ignore]
    fn render_preview() {
        let theme = Theme::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../config/shell/theme.json")).unwrap_or_default();
        let labels: Vec<String> = ["Sound settings", "Volume mixer", "Mute", "Display settings"].map(String::from).to_vec();
        let header = Header::Slider("Volume".into(), 0.42);
        let scale = 2.0;
        let (w, h) = (px(240, scale), header_h(&header, scale) + row_h(scale) * labels.len() as i32 + 2 * pad(scale));
        let mut buf = vec![0u32; (w * h) as usize];
        unsafe { compose(&mut buf, w, h, &header, &labels, Some(1), &theme, scale) };
        let mut bmp = Vec::new();
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&((54 + w * h * 4) as u32).to_le_bytes());
        bmp.extend_from_slice(&[0; 4]);
        bmp.extend_from_slice(&54u32.to_le_bytes());
        bmp.extend_from_slice(&40u32.to_le_bytes());
        bmp.extend_from_slice(&w.to_le_bytes());
        bmp.extend_from_slice(&(-h).to_le_bytes());
        bmp.extend_from_slice(&1u16.to_le_bytes());
        bmp.extend_from_slice(&32u16.to_le_bytes());
        bmp.extend_from_slice(&[0; 24]);
        for p in &buf {
            let a = (p >> 24) & 255;
            let over = |c: u32| ((c & 255) + (40 * (255 - a)) / 255).min(255);
            bmp.extend_from_slice(&[over(*p) as u8, over(*p >> 8) as u8, over(*p >> 16) as u8, 255]);
        }
        std::fs::write(std::env::temp_dir().join("winarchy-menu.bmp"), bmp).unwrap();
    }

    #[test]
    fn the_slider_maps_the_pointer_to_zero_one_inside_the_rail() {
        assert_eq!(slider_value(0, 200, 14), 0.0);
        assert_eq!(slider_value(100, 200, 14), 0.5);
        assert_eq!(slider_value(900, 200, 14), 1.0);
    }

    #[test]
    fn the_menu_sticks_to_the_anchor_and_stays_on_the_monitor() {
        let mon = (0, 0, 1000, 800);
        assert_eq!(place((100, 400), (200, 120), Anchor::BottomLeft, mon), (100, 280));
        assert_eq!(place((100, 400), (200, 120), Anchor::BottomRight, mon), (0, 280));
        assert_eq!(place((990, 50), (200, 120), Anchor::BottomLeft, mon), (800, 0));
    }
}
