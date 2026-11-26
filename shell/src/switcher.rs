// Conmutador de Alt+Tab: AHK decide el orden y la ventana elegida; esta ventana solo muestra cuál es, con una miniatura en vivo por ventana.
// No toma el foco ni captura el mouse (WS_EX_NOACTIVATE + TRANSPARENT), así el Alt sostenido sigue siendo de AHK.
// shortcut: muestra como máximo MAX_SHOWN ventanas (las últimas hasta la elegida), sin scroll; sumar si hace falta.
use crate::dock::{draw_label, fill_round_a};
use crate::layout::{px, R};
use crate::theme::{rgb, Theme};
use std::cell::RefCell;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

const MAX_SHOWN: usize = 10;
const COLS: usize = 5;
const CELL_W_DIP: i32 = 210;
const THUMB_H_DIP: i32 = 124;
const LABEL_H_DIP: i32 = 30;
const ICON_DIP: i32 = 20;
const INSET_DIP: i32 = 10;
const PAD_DIP: i32 = 12;
const MAX_TITLE: usize = 26;
const CLASS: PCWSTR = w!("WinarchySwitcher");

struct Switcher {
    hwnd: HWND,
    shown: Vec<isize>,
    icons: Vec<Option<crate::dock::Icon>>,
    thumbs: Vec<isize>,
    bmp: HBITMAP,
}

thread_local! {
    static SW: RefCell<Option<Switcher>> = const { RefCell::new(None) };
}

pub fn init() {
    unsafe {
        let Ok(inst) = GetModuleHandleW(PCWSTR::null()) else { return };
        let class = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: inst.into(), lpszClassName: CLASS, ..Default::default() };
        RegisterClassW(&class);
        // Sin layered: DWM no compone miniaturas dentro de una ventana con alfa por píxel.
        let ex = WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT;
        let Ok(hwnd) = CreateWindowExW(ex, CLASS, w!("Winarchy Switcher"), WS_POPUP, 0, 0, 1, 1, None, None, inst, None) else { return };
        SW.with(|c| *c.borrow_mut() = Some(Switcher { hwnd, shown: Vec::new(), icons: Vec::new(), thumbs: Vec::new(), bmp: HBITMAP::default() }));
    }
}

fn unregister(s: &mut Switcher) {
    for t in s.thumbs.drain(..) {
        unsafe {
            let _ = DwmUnregisterThumbnail(t);
        }
    }
}

pub fn hide() {
    SW.with(|c| {
        if let Some(s) = c.borrow_mut().as_mut() {
            unregister(s);
            s.shown.clear();
            unsafe {
                let _ = ShowWindow(s.hwnd, SW_HIDE);
            }
        }
    });
}

// Ventanas visibles: hasta MAX_SHOWN, terminando en la elegida cuando la lista es más larga.
fn slice(len: usize, index: usize) -> std::ops::Range<usize> {
    let start = (index + 1).saturating_sub(MAX_SHOWN);
    start..len.min(start + MAX_SHOWN)
}

fn title_of(hwnd: isize) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetWindowTextW(HWND(hwnd as *mut _), &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

pub fn show(mon: (i32, i32, i32, i32), scale: f32, theme: &Theme) {
    let (wins, index) = crate::pipe::SWITCHER.lock().unwrap().clone();
    let range = slice(wins.len(), index);
    let ids: Vec<isize> = wins[range.clone()].to_vec();
    let sel = index - range.start;
    SW.with(|c| {
        let mut c = c.borrow_mut();
        let Some(s) = c.as_mut() else { return };
        let fresh = s.shown != ids;
        if fresh {
            unregister(s);
            s.icons = ids.iter().map(|h| unsafe { crate::dock::fetch_icon(&format!("window:{h}"), 64) }).collect();
            s.shown = ids;
        }
        unsafe { paint(s, sel, fresh, mon, scale, theme) };
    });
}

struct Geo {
    cell_w: i32,
    cell_h: i32,
    pad: i32,
    inset: i32,
    thumb_h: i32,
    w: i32,
    h: i32,
}

fn geometry(n: usize, scale: f32) -> Geo {
    let (cell_w, pad, inset) = (px(CELL_W_DIP, scale), px(PAD_DIP, scale), px(INSET_DIP, scale));
    let thumb_h = px(THUMB_H_DIP, scale);
    let cell_h = inset + thumb_h + px(LABEL_H_DIP, scale) + inset / 2;
    let cols = n.clamp(1, COLS) as i32;
    let rows = n.div_ceil(COLS).max(1) as i32;
    Geo { cell_w, cell_h, pad, inset, thumb_h, w: cols * cell_w + 2 * pad, h: rows * cell_h + 2 * pad }
}

fn cell_rect(g: &Geo, i: usize) -> R {
    R { x: g.pad + (i % COLS) as i32 * g.cell_w, y: g.pad + (i / COLS) as i32 * g.cell_h, w: g.cell_w, h: g.cell_h }
}

unsafe fn paint(s: &mut Switcher, sel: usize, fresh: bool, mon: (i32, i32, i32, i32), scale: f32, theme: &Theme) {
    let g = geometry(s.shown.len(), scale);
    let (w, h) = (g.w, g.h);
    let (bg, fg, accent) = (rgb(&theme.background), rgb(&theme.foreground), rgb(&theme.accent));
    let radius = px(crate::dock::style(theme).radius.max(12), scale);
    let screen = GetDC(None);
    let mem = CreateCompatibleDC(screen);
    let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: 0, ..Default::default() }, ..Default::default() };
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    if let Ok(dib) = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
        let buf = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
        buf.fill(0);
        fill_round_a(buf, w, h, R { x: 0, y: 0, w, h }, 0, bg, 1.0);
        for i in 0..s.shown.len() {
            let c = cell_rect(&g, i);
            if i == sel {
                fill_round_a(buf, w, h, c, px(10, scale), accent, 0.35);
                fill_round_a(buf, w, h, c, px(10, scale), fg, 0.08);
            }
            let label = R { x: c.x + g.inset, y: c.y + g.inset + g.thumb_h, w: c.w - 2 * g.inset, h: c.h - 2 * g.inset - g.thumb_h + g.inset / 2 };
            let icon = px(ICON_DIP, scale);
            let mut text_x = label.x;
            if let Some(ic) = &s.icons[i] {
                crate::start::blit(buf, (w, h), label.x, label.y + (label.h - icon) / 2, icon, &crate::start::scale_icon(ic, icon));
                text_x += icon + px(6, scale);
            }
            let title = crate::dock::clip_title(&title_of(s.shown[i]), MAX_TITLE);
            draw_label(buf, w, h, R { x: text_x, y: label.y, w: label.x + label.w - text_x, h: label.h }, &title, &theme.font_family, scale, fg, theme.font_size.max(12));
        }
        if !s.bmp.is_invalid() {
            let _ = DeleteObject(s.bmp);
        }
        s.bmp = dib;
    }
    let _ = DeleteDC(mem);
    ReleaseDC(None, screen);

    if fresh {
        for i in 0..s.shown.len() {
            let c = cell_rect(&g, i);
            let Ok(handle) = DwmRegisterThumbnail(s.hwnd, HWND(s.shown[i] as *mut _)) else { continue };
            let Ok(size) = DwmQueryThumbnailSourceSize(handle) else {
                let _ = DwmUnregisterThumbnail(handle);
                continue;
            };
            let (tw, th) = crate::dock_thumb::fit(size.cx, size.cy, c.w - 2 * g.inset, g.thumb_h);
            if tw == 0 {
                let _ = DwmUnregisterThumbnail(handle);
                continue;
            }
            let (left, top) = (c.x + (c.w - tw) / 2, c.y + g.inset + (g.thumb_h - th) / 2);
            let props = DWM_THUMBNAIL_PROPERTIES { dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE | DWM_TNP_OPACITY, rcDestination: RECT { left, top, right: left + tw, bottom: top + th }, opacity: 255, fVisible: true.into(), ..Default::default() };
            let _ = DwmUpdateThumbnailProperties(handle, &props);
            s.thumbs.push(handle);
        }
        let (x, y) = (mon.0 + (mon.2 - mon.0 - w) / 2, mon.1 + (mon.3 - mon.1 - h) / 2);
        let _ = SetWindowRgn(s.hwnd, CreateRoundRectRgn(0, 0, w + 1, h + 1, radius, radius), true);
        let _ = SetWindowPos(s.hwnd, HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
    }
    let _ = InvalidateRect(s.hwnd, None, false);
}

unsafe extern "system" fn proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(h, &mut ps);
            let bmp = SW.with(|c| c.try_borrow().ok().and_then(|c| c.as_ref().map(|s| s.bmp)));
            if let Some(bmp) = bmp.filter(|b| !b.is_invalid()) {
                let mem = CreateCompatibleDC(hdc);
                let old = SelectObject(mem, bmp);
                let mut rc = RECT::default();
                let _ = GetClientRect(h, &mut rc);
                let _ = BitBlt(hdc, 0, 0, rc.right, rc.bottom, mem, 0, 0, SRCCOPY);
                SelectObject(mem, old);
                let _ = DeleteDC(mem);
            }
            let _ = EndPaint(h, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_lists_show_everything() {
        assert_eq!(slice(5, 3), 0..5);
        assert_eq!(slice(MAX_SHOWN, MAX_SHOWN - 1), 0..MAX_SHOWN);
    }

    #[test]
    fn long_lists_end_at_the_selected_window() {
        assert_eq!(slice(30, 0), 0..MAX_SHOWN);
        assert_eq!(slice(30, 20), 11..21);
        assert!(slice(30, 29).contains(&29));
    }

    #[test]
    fn grid_wraps_into_rows() {
        let g = geometry(7, 1.0);
        assert_eq!(g.w, COLS as i32 * g.cell_w + 2 * g.pad);
        assert_eq!(g.h, 2 * g.cell_h + 2 * g.pad);
        assert_eq!(cell_rect(&g, 5).y, g.pad + g.cell_h);
    }
}
