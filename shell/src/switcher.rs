// Conmutador de Alt+Tab: AHK decide el orden y la ventana elegida; esta ventana solo muestra cuál es.
// No toma el foco ni captura el mouse (WS_EX_NOACTIVATE + TRANSPARENT), así el Alt sostenido sigue siendo de AHK.
// shortcut: muestra como máximo MAX_SHOWN ventanas (las últimas hasta la elegida), sin filas ni scroll; sumar si hace falta.
use crate::dock::{draw_label, fill_round_a, style};
use crate::layout::{px, R};
use crate::theme::{rgb, Theme};
use std::cell::RefCell;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

const MAX_SHOWN: usize = 12;
const CELL_DIP: i32 = 72;
const ICON_DIP: i32 = 44;
const PAD_DIP: i32 = 14;
const TITLE_DIP: i32 = 34;
const MAX_TITLE: usize = 64;
const CLASS: PCWSTR = w!("WinarchySwitcher");

struct Switcher {
    hwnd: HWND,
    shown: Vec<isize>,
    icons: Vec<Option<crate::dock::Icon>>,
}

thread_local! {
    static SW: RefCell<Option<Switcher>> = const { RefCell::new(None) };
}

pub fn init() {
    unsafe {
        let Ok(inst) = GetModuleHandleW(PCWSTR::null()) else { return };
        let class = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: inst.into(), lpszClassName: CLASS, ..Default::default() };
        RegisterClassW(&class);
        let ex = WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT;
        let Ok(hwnd) = CreateWindowExW(ex, CLASS, w!("Winarchy Switcher"), WS_POPUP, 0, 0, 1, 1, None, None, inst, None) else { return };
        SW.with(|c| *c.borrow_mut() = Some(Switcher { hwnd, shown: Vec::new(), icons: Vec::new() }));
    }
}

pub fn hide() {
    SW.with(|c| {
        if let Some(s) = c.borrow().as_ref() {
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
        if s.shown != ids {
            s.icons = ids.iter().map(|h| unsafe { crate::dock::fetch_icon(&format!("window:{h}"), 64) }).collect();
            s.shown = ids.clone();
        }
        unsafe { paint(s, sel, mon, scale, theme) };
    });
}

unsafe fn paint(s: &Switcher, sel: usize, mon: (i32, i32, i32, i32), scale: f32, theme: &Theme) {
    let (cell, icon, pad, title_h) = (px(CELL_DIP, scale), px(ICON_DIP, scale), px(PAD_DIP, scale), px(TITLE_DIP, scale));
    let n = s.shown.len() as i32;
    let (w, h) = (n * cell + 2 * pad, cell + title_h + 2 * pad);
    let (bg, fg, accent) = (rgb(&theme.background), rgb(&theme.foreground), rgb(&theme.accent));
    let st = style(theme);
    let screen = GetDC(None);
    let mem = CreateCompatibleDC(screen);
    let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: 0, ..Default::default() }, ..Default::default() };
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    if let Ok(dib) = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
        let old = SelectObject(mem, dib);
        let buf = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
        buf.fill(0);
        fill_round_a(buf, w, h, R { x: 0, y: 0, w, h }, px(st.radius.max(12), scale), bg, st.opacity);
        for (i, icon_px) in s.icons.iter().enumerate() {
            let c = R { x: pad + i as i32 * cell, y: pad, w: cell, h: cell };
            if i == sel {
                fill_round_a(buf, w, h, c, px(10, scale), accent, 0.28);
                fill_round_a(buf, w, h, c, px(10, scale), fg, 0.08);
            }
            if let Some(ic) = icon_px {
                crate::start::blit(buf, (w, h), c.x + (cell - icon) / 2, c.y + (cell - icon) / 2, icon, &crate::start::scale_icon(ic, icon));
            }
        }
        let title = crate::dock::clip_title(&title_of(s.shown[sel]), MAX_TITLE);
        draw_label(buf, w, h, R { x: pad, y: pad + cell, w: w - 2 * pad, h: title_h }, &title, &theme.font_family, scale, fg, theme.font_size.max(12));
        let target = POINT { x: mon.0 + (mon.2 - mon.0 - w) / 2, y: mon.1 + (mon.3 - mon.1 - h) / 2 };
        let sz = SIZE { cx: w, cy: h };
        let bf = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: 1 };
        let _ = UpdateLayeredWindow(s.hwnd, None, Some(&target), Some(&sz), mem, Some(&POINT::default()), COLORREF(0), Some(&bf), ULW_ALPHA);
        SelectObject(mem, old);
        let _ = DeleteObject(dib);
    }
    let _ = DeleteDC(mem);
    ReleaseDC(None, screen);
    let _ = ShowWindow(s.hwnd, SW_SHOWNA);
}

unsafe extern "system" fn proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    DefWindowProcW(h, m, wp, lp)
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
        assert_eq!(slice(30, 20), 9..21);
        assert!(slice(30, 29).contains(&29));
    }
}
