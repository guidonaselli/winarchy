// Miniatura en vivo de la ventana que enfocaría el clic en una celda del dock, encima de su tooltip.
// Es una ventana aparte y sin layered: DWM no compone miniaturas dentro de una ventana con alfa por píxel.
use crate::layout::px;
use crate::theme::{rgb, Theme};
use std::cell::RefCell;
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

const MAX_W_DIP: i32 = 240;
const MAX_H_DIP: i32 = 150;
const PAD_DIP: i32 = 6;
const GAP_DIP: i32 = 6;

struct Thumb {
    hwnd: HWND,
    handles: Vec<isize>,
    bg: COLORREF,
    edge: COLORREF,
}

thread_local! {
    static THUMB: RefCell<Option<Thumb>> = const { RefCell::new(None) };
}

// Ajusta el tamaño de la ventana origen a la caja máxima conservando la proporción; nunca agranda.
pub fn fit(src_w: i32, src_h: i32, max_w: i32, max_h: i32) -> (i32, i32) {
    if src_w <= 0 || src_h <= 0 {
        return (0, 0);
    }
    let k = (max_w as f32 / src_w as f32).min(max_h as f32 / src_h as f32).min(1.0);
    (((src_w as f32 * k).round() as i32).max(1), ((src_h as f32 * k).round() as i32).max(1))
}

fn colorref(c: (u8, u8, u8)) -> COLORREF {
    COLORREF(c.0 as u32 | (c.1 as u32) << 8 | (c.2 as u32) << 16)
}

unsafe extern "system" fn proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_ERASEBKGND => {
            let (bg, edge) = THUMB.with(|t| t.try_borrow().ok().and_then(|t| t.as_ref().map(|t| (t.bg, t.edge)))).unwrap_or((COLORREF(0), COLORREF(0)));
            let mut rc = RECT::default();
            let _ = GetClientRect(h, &mut rc);
            let hdc = HDC(wp.0 as *mut _);
            let fill = CreateSolidBrush(edge);
            FillRect(hdc, &rc, fill);
            let _ = DeleteObject(fill);
            let inner = RECT { left: rc.left + 1, top: rc.top + 1, right: rc.right - 1, bottom: rc.bottom - 1 };
            let fill = CreateSolidBrush(bg);
            FillRect(hdc, &inner, fill);
            let _ = DeleteObject(fill);
            LRESULT(1)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

unsafe fn window() -> Option<HWND> {
    if let Some(h) = THUMB.with(|t| t.borrow().as_ref().map(|t| t.hwnd)) {
        return Some(h);
    }
    let inst = GetModuleHandleW(None).ok()?;
    let class = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: inst.into(), lpszClassName: w!("WinarchyShellDockThumb"), ..Default::default() };
    RegisterClassW(&class);
    let ex = WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT;
    let hwnd = CreateWindowExW(ex, w!("WinarchyShellDockThumb"), w!(""), WS_POPUP, 0, 0, 1, 1, None, None, inst, None).ok()?;
    THUMB.with(|t| *t.borrow_mut() = Some(Thumb { hwnd, handles: Vec::new(), bg: COLORREF(0), edge: COLORREF(0) }));
    Some(hwnd)
}

pub fn hide() {
    THUMB.with(|t| {
        let Ok(mut t) = t.try_borrow_mut() else { return };
        if let Some(t) = t.as_mut() {
            unsafe {
                for handle in t.handles.drain(..) {
                    let _ = DwmUnregisterThumbnail(handle);
                }
                let _ = ShowWindow(t.hwnd, SW_HIDE);
            }
        }
    });
}

// `tip` es el rectángulo (x, y, w, h) del tooltip en pantalla: la miniatura se apila del lado contrario a la barra.
// Posiciones de una fila de miniaturas con `gap` entre ellas: (x, ancho, alto) por cada una que cabe en `max_total`.
// shortcut: las que no caben se omiten (sin desplazamiento), mejorar con scroll si un grupo supera el ancho del monitor.
pub fn row_layout(sizes: &[(i32, i32)], gap: i32, max_total: i32) -> Vec<(i32, i32, i32)> {
    let mut out = Vec::new();
    let mut x = 0;
    for (w, h) in sizes {
        if !out.is_empty() && x + w > max_total {
            break;
        }
        out.push((x, *w, *h));
        x += w + gap;
    }
    out
}

// Una miniatura por ventana del grupo; la que enfocaría el clic va a opacidad completa y las demás atenuadas.
pub unsafe fn show(targets: &[isize], focus: isize, cx: i32, tip: (i32, i32, i32, i32), bottom: bool, mon: (i32, i32, i32, i32), scale: f32, theme: &Theme) {
    hide();
    let Some(hwnd) = window() else { return };
    let (pad, gap) = (px(PAD_DIP, scale), px(GAP_DIP, scale));
    let mut sources: Vec<(isize, isize, (i32, i32))> = Vec::new();
    for target in targets {
        let src = HWND(*target as *mut _);
        if !IsWindow(src).as_bool() {
            continue;
        }
        let Ok(handle) = DwmRegisterThumbnail(hwnd, src) else { continue };
        let Ok(size) = DwmQueryThumbnailSourceSize(handle) else {
            let _ = DwmUnregisterThumbnail(handle);
            continue;
        };
        let fitted = fit(size.cx, size.cy, px(MAX_W_DIP, scale), px(MAX_H_DIP, scale));
        if fitted.0 == 0 {
            let _ = DwmUnregisterThumbnail(handle);
            continue;
        }
        sources.push((*target, handle, fitted));
    }
    if sources.is_empty() {
        return;
    }
    let sizes: Vec<(i32, i32)> = sources.iter().map(|s| s.2).collect();
    let row = row_layout(&sizes, gap, (mon.2 - mon.0) - 2 * pad);
    for (_, handle, _) in sources.iter().skip(row.len()) {
        let _ = DwmUnregisterThumbnail(*handle);
    }
    sources.truncate(row.len());
    let total_w = row.last().map(|r| r.0 + r.1).unwrap_or(0);
    let row_h = row.iter().map(|r| r.2).max().unwrap_or(0);
    let (w, h) = (total_w + 2 * pad, row_h + 2 * pad);
    let x = (cx - w / 2).clamp(mon.0, (mon.2 - w).max(mon.0));
    let y = if bottom { tip.1 - h - gap } else { tip.1 + tip.3 + gap }.clamp(mon.1, (mon.3 - h).max(mon.1));
    for ((target, handle, _), (rx, rw, rh)) in sources.iter().zip(&row) {
        let top = pad + (row_h - rh) / 2;
        let opacity = if sources.len() == 1 || *target == focus { 255 } else { 140 };
        let props = DWM_THUMBNAIL_PROPERTIES { dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE | DWM_TNP_OPACITY, rcDestination: RECT { left: pad + rx, top, right: pad + rx + rw, bottom: top + rh }, opacity, fVisible: true.into(), ..Default::default() };
        let _ = DwmUpdateThumbnailProperties(*handle, &props);
    }
    THUMB.with(|t| {
        if let Some(t) = t.borrow_mut().as_mut() {
            t.handles = sources.iter().map(|s| s.1).collect();
            t.bg = colorref(rgb(&theme.background));
            t.edge = colorref(rgb(&theme.accent));
        }
    });
    let _ = SetWindowRgn(hwnd, CreateRoundRectRgn(0, 0, w + 1, h + 1, px(8, scale), px(8, scale)), true);
    let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
}

#[cfg(test)]
mod tests {
    use super::{fit, row_layout};

    #[test]
    fn the_row_stops_when_the_next_thumbnail_does_not_fit() {
        assert_eq!(row_layout(&[(100, 60), (100, 60), (100, 60)], 10, 250), vec![(0, 100, 60), (110, 100, 60)]);
        assert_eq!(row_layout(&[(300, 60)], 10, 250), vec![(0, 300, 60)]);
        assert!(row_layout(&[], 10, 250).is_empty());
    }

    #[test]
    fn thumbnails_keep_the_aspect_ratio_and_never_grow() {
        assert_eq!(fit(1920, 1080, 240, 150), (240, 135));
        assert_eq!(fit(800, 1200, 240, 150), (100, 150));
        assert_eq!(fit(100, 60, 240, 150), (100, 60));
        assert_eq!(fit(0, 10, 240, 150), (0, 0));
    }
}
