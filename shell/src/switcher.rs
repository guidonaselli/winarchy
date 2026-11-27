// Conmutador de Alt+Tab: AHK decide el orden y la ventana elegida; esta ventana solo muestra cuál es, con una miniatura en vivo por ventana.
// No toma el foco ni captura el mouse (WS_EX_NOACTIVATE + TRANSPARENT), así el Alt sostenido sigue siendo de AHK.
use crate::dock::{draw_label, fill_round_a};
use crate::layout::{px, R};
use crate::theme::{rgb, Theme};
use std::cell::RefCell;
use std::sync::Mutex;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

const COLS: usize = 5;
const CELL_W_DIP: i32 = 210;
const THUMB_H_DIP: i32 = 124;
const LABEL_H_DIP: i32 = 30;
const ICON_DIP: i32 = 20;
const INSET_DIP: i32 = 10;
const PAD_DIP: i32 = 12;
const MAX_TITLE: usize = 26;
const CLASS: PCWSTR = w!("WinarchySwitcher");
const WM_ICONS: u32 = WM_APP + 1;

type Icons = Vec<Option<crate::dock::Icon>>;
static LOADED: Mutex<Option<(Vec<isize>, Icons)>> = Mutex::new(None);

struct Switcher {
    hwnd: HWND,
    shown: Vec<isize>,
    titles: Vec<String>,
    icons: Icons,
    thumbs: Vec<isize>,
    bmp: HBITMAP,
    bits: *mut std::ffi::c_void,
    bitmap_size: (i32, i32),
    mon: (i32, i32, i32, i32),
    scale: f32,
    theme: Option<Theme>,
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
        SW.with(|c| {
            *c.borrow_mut() = Some(Switcher {
                hwnd,
                shown: Vec::new(),
                titles: Vec::new(),
                icons: Vec::new(),
                thumbs: Vec::new(),
                bmp: HBITMAP::default(),
                bits: std::ptr::null_mut(),
                bitmap_size: (0, 0),
                mon: (0, 0, 0, 0),
                scale: 1.0,
                theme: None,
            })
        });
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
            unsafe { let _ = ShowWindow(s.hwnd, SW_HIDE); }
            unregister(s);
            s.shown.clear();
        }
    });
}


fn title_of(hwnd: isize) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetWindowTextW(HWND(hwnd as *mut _), &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

pub fn show(mon: (i32, i32, i32, i32), scale: f32, theme: &Theme) {
    let (ids, sel) = crate::pipe::SWITCHER.lock().unwrap().clone();
    SW.with(|c| {
        let mut c = c.borrow_mut();
        let Some(s) = c.as_mut() else { return };
        let fresh = s.shown != ids;
        let resized = s.scale != scale || (s.mon.2 - s.mon.0, s.mon.3 - s.mon.1) != (mon.2 - mon.0, mon.3 - mon.1);
        if fresh {
            unregister(s);
            s.titles = ids.iter().map(|hwnd| title_of(*hwnd)).collect();
            s.icons = ids.iter().map(|_| None).collect();
            load_icons(s.hwnd, ids.clone());
            s.shown = ids;
        } else if resized {
            unregister(s);
        }
        s.mon = mon;
        s.scale = scale;
        s.theme = Some(theme.clone());
        unsafe { paint(s, sel, fresh || resized) };
    });
}

// Pedir un ícono manda un mensaje a la ventana ajena: nunca en el hilo de interfaz.
fn load_icons(hwnd: HWND, ids: Vec<isize>) {
    let target = hwnd.0 as isize;
    std::thread::spawn(move || {
        // el ícono por AppUserModelID sale del namespace del shell, que pide COM
        let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let icons = ids.iter().map(|h| unsafe { crate::dock::window_icon(*h, 64) }).collect();
        if com.is_ok() {
            unsafe { CoUninitialize() };
        }
        *LOADED.lock().unwrap() = Some((ids, icons));
        unsafe {
            let _ = PostMessageW(HWND(target as *mut _), WM_ICONS, WPARAM(0), LPARAM(0));
        }
    });
}

fn icons_ready() {
    let Some((ids, icons)) = LOADED.lock().unwrap().take() else { return };
    let sel = crate::pipe::SWITCHER.lock().unwrap().1;
    SW.with(|c| {
        let Ok(mut c) = c.try_borrow_mut() else { return };
        if let Some(s) = c.as_mut().filter(|s| s.shown == ids) {
            s.icons = icons;
            unsafe { paint(s, sel, false) };
        }
    });
}

pub fn set_theme(theme: &Theme) {
    let current = SW.with(|c| {
        c.borrow().as_ref().and_then(|s| (!s.shown.is_empty()).then_some((s.mon, s.scale)))
    });
    if let Some((mon, scale)) = current {
        show(mon, scale, theme);
    }
}

struct Geo {
    cell_w: i32,
    cell_h: i32,
    cols: usize,
    pad: i32,
    inset: i32,
    thumb_h: i32,
    factor: f32,
    w: i32,
    h: i32,
}

fn geometry(n: usize, scale: f32, monitor: (i32, i32)) -> Geo {
    let count = n.max(1);
    let (base_w, pad, base_inset) = (px(CELL_W_DIP, scale), px(PAD_DIP, scale), px(INSET_DIP, scale));
    let base_thumb_h = px(THUMB_H_DIP, scale);
    let base_h = base_inset + base_thumb_h + px(LABEL_H_DIP, scale) + base_inset / 2;
    let max_w = (monitor.0 - 2 * pad).max(1) as f32;
    let max_h = (monitor.1 - 2 * pad).max(1) as f32;
    let preferred_cols = count.min(COLS);
    let (cols, factor) = (1..=count)
        .map(|cols| {
            let rows = count.div_ceil(cols);
            let factor = (max_w / (cols as f32 * base_w as f32))
                .min(max_h / (rows as f32 * base_h as f32))
                .min(1.0);
            (cols, factor)
        })
        .max_by(|(cols_a, factor_a), (cols_b, factor_b)| {
            factor_a.total_cmp(factor_b).then_with(|| {
                cols_b.abs_diff(preferred_cols).cmp(&cols_a.abs_diff(preferred_cols))
            })
        })
        .unwrap();
    let rows = count.div_ceil(cols) as i32;
    let cell_w = (base_w as f32 * factor).floor().max(1.0) as i32;
    let cell_h = (base_h as f32 * factor).floor().max(1.0) as i32;
    let inset = (base_inset as f32 * factor).round().max(1.0) as i32;
    let thumb_h = (base_thumb_h as f32 * factor).round().max(1.0) as i32;
    Geo {
        cell_w,
        cell_h,
        cols,
        pad,
        inset,
        thumb_h,
        factor,
        w: cols as i32 * cell_w + 2 * pad,
        h: rows * cell_h + 2 * pad,
    }
}

fn cell_rect(g: &Geo, i: usize) -> R {
    R { x: g.pad + (i % g.cols) as i32 * g.cell_w, y: g.pad + (i / g.cols) as i32 * g.cell_h, w: g.cell_w, h: g.cell_h }
}


unsafe fn render_base(buf: &mut [u32], size: (i32, i32), g: &Geo, s: &Switcher, theme: &Theme, scale: f32, sel: usize, colors: ((u8, u8, u8), (u8, u8, u8))) {
    let (w, h) = size;
    let (bg, fg) = colors;
    buf.fill(0);
    fill_round_a(buf, w, h, R { x: 0, y: 0, w, h }, 0, bg, 1.0);
    let hl = cell_rect(g, sel);
    let radius = px((10.0 * g.factor).round().max(1.0) as i32, scale);
    fill_round_a(buf, w, h, hl, radius, rgb(&theme.accent), 0.35);
    fill_round_a(buf, w, h, hl, radius, fg, 0.08);
    for (i, title) in s.titles.iter().enumerate() {
        let c = cell_rect(g, i);
        // DWM no tiene miniatura de una ventana oculta (otro workspace): va su ícono en grande
        if let Some(ic) = s.icons[i].as_ref().filter(|_| !IsWindowVisible(HWND(s.shown[i] as *mut _)).as_bool()) {
            let big = g.thumb_h / 2;
            crate::start::blit(buf, (w, h), c.x + (c.w - big) / 2, c.y + g.inset + (g.thumb_h - big) / 2, big, &crate::start::scale_icon(ic, big));
        }
        let label = R { x: c.x + g.inset, y: c.y + g.inset + g.thumb_h, w: c.w - 2 * g.inset, h: c.h - 2 * g.inset - g.thumb_h + g.inset / 2 };
        let icon = (px(ICON_DIP, scale) as f32 * g.factor).round().max(1.0) as i32;
        let lead = s.icons[i].as_ref().map_or(0, |_| icon + (px(6, scale) as f32 * g.factor).round() as i32);
        let title = crate::dock::clip_title(title, MAX_TITLE);
        let (size, font) = (scale * g.factor, theme.font_size.max(12));
        // ícono y título se centran juntos; los 2 px evitan que el antialias fuerce los puntos suspensivos
        let text_w = (crate::dock::text_width(&title, &theme.font_family, size, font) + 2).min(label.w - lead);
        let x = label.x + (label.w - lead - text_w) / 2;
        if let Some(ic) = &s.icons[i] {
            crate::start::blit(buf, (w, h), x, label.y + (label.h - icon) / 2, icon, &crate::start::scale_icon(ic, icon));
        }
        draw_label(buf, w, h, R { x: x + lead, y: label.y, w: text_w, h: label.h }, &title, &theme.font_family, size, fg, font);
    }
}

unsafe fn paint(s: &mut Switcher, sel: usize, rebuild: bool) {
    let Some(theme) = s.theme.as_ref() else { return };
    let (mon, scale) = (s.mon, s.scale);
    let g = geometry(s.shown.len(), scale, (mon.2 - mon.0, mon.3 - mon.1));
    let (w, h) = (g.w, g.h);
    let (bg, fg) = (rgb(&theme.background), rgb(&theme.foreground));
    let radius = px((crate::dock::style(theme).radius.max(12) as f32 * g.factor).round().max(1.0) as i32, scale);
    if s.bitmap_size != (w, h) || s.bmp.is_invalid() {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(screen);
        let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: 0, ..Default::default() }, ..Default::default() };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let dib = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0);
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        // sin bitmap se olvida la lista: el siguiente paso de Alt+Tab vuelve a intentar entero
        let Ok(dib) = dib else {
            s.shown.clear();
            return;
        };
        if bits.is_null() {
            let _ = DeleteObject(dib);
            s.shown.clear();
            return;
        }
        if !s.bmp.is_invalid() {
            let _ = DeleteObject(s.bmp);
        }
        s.bmp = dib;
        s.bits = bits;
        s.bitmap_size = (w, h);
    }
    let buf = std::slice::from_raw_parts_mut(s.bits as *mut u32, (w * h) as usize);
    render_base(buf, (w, h), &g, s, theme, scale, sel, (bg, fg));
    let (x, y) = (mon.0 + (mon.2 - mon.0 - w) / 2, mon.1 + (mon.3 - mon.1 - h) / 2);

    if rebuild {
        for i in 0..s.shown.len() {
            let c = cell_rect(&g, i);
            if !IsWindowVisible(HWND(s.shown[i] as *mut _)).as_bool() {
                continue;
            }
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
        let _ = SetWindowRgn(s.hwnd, CreateRoundRectRgn(0, 0, w + 1, h + 1, radius, radius), true);
        let _ = SetWindowPos(s.hwnd, HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
    } else {
        let _ = SetWindowPos(s.hwnd, HWND_TOPMOST, x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
    }
    let _ = InvalidateRect(s.hwnd, None, false);
}


unsafe extern "system" fn proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_ERASEBKGND => LRESULT(1),
        WM_ICONS => {
            icons_ready();
            LRESULT(0)
        }
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
    fn grid_wraps_into_rows() {
        let g = geometry(7, 1.0, (1920, 1080));
        assert_eq!(g.w, g.cols as i32 * g.cell_w + 2 * g.pad);
        assert_eq!(g.h, 2 * g.cell_h + 2 * g.pad);
        assert_eq!(cell_rect(&g, 5).y, g.pad + g.cell_h);
    }

    #[test]
    fn dense_high_dpi_grid_keeps_text_inside_its_label() {
        let scale = 2.0;
        let g = geometry(64, scale, (1366, 768));
        let label_h = g.cell_h - 2 * g.inset - g.thumb_h + g.inset / 2;
        assert!(px(12, scale * g.factor) <= label_h);
    }

    #[test]
    fn all_windows_fit_inside_the_display() {
        for (monitor, scale) in [((1920, 1080), 1.0), ((1366, 768), 1.25), ((3840, 2160), 2.0)] {
            for count in [1, 7, 30, 100] {
                let g = geometry(count, scale, monitor);
                assert!(g.w <= monitor.0 && g.h <= monitor.1);
                for index in 0..count {
                    let cell = cell_rect(&g, index);
                    assert!(cell.x >= g.pad && cell.y >= g.pad);
                    assert!(cell.x + cell.w <= g.w - g.pad);
                    assert!(cell.y + cell.h <= g.h - g.pad);
                }
            }
        }
    }
}
