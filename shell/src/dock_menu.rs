// Menú contextual del dock: una ventana propia y activa (recibe el teclado y se descarta al perder el foco),
// dibujada con el theme. La lógica de qué entradas hay y dónde cae cada fila es pura.
use crate::dock::{blend, fill_round_a, pick_target, premul};
use crate::i18n::{t, K};
use crate::layout::{px, R};
use crate::theme::{rgb, Theme};
use std::cell::RefCell;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_DOWN, VK_ESCAPE, VK_RETURN, VK_UP};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::*;

type MonRect = (i32, i32, i32, i32);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    Focus(isize),
    Open,
    Pin,
    Unpin,
    Close,
    CloseAll,
    Tray,
}

// `picker`: solo la lista de ventanas (clic izquierdo en un grupo con varias). El menú contextual la antepone cuando hay más de una.
#[derive(Clone, Copy)]
pub struct Caps {
    pub launchable: bool,
    pub pinned: bool,
    pub pinnable: bool,
    pub picker: bool,
    pub closable: bool,
    pub tray: bool,
}

pub fn acts(wins: &[isize], c: Caps) -> Vec<Act> {
    let Caps { launchable, pinned, pinnable, picker, closable, tray } = c;
    let windows = wins.len();
    let mut v = Vec::new();
    if picker || windows > 1 {
        v.extend(wins.iter().map(|w| Act::Focus(*w)));
    }
    if picker {
        return v;
    }
    if launchable {
        v.push(Act::Open);
    }
    if pinned {
        v.push(Act::Unpin);
    } else if pinnable {
        v.push(Act::Pin);
    }
    // Una ventana elevada ignora WM_CLOSE de un proceso que no lo está (UIPI): ofrecer cerrar sería mentir.
    if closable && windows > 0 {
        v.push(Act::Close);
    }
    if closable && windows > 1 {
        v.push(Act::CloseAll);
    }
    if tray {
        v.push(Act::Tray);
    }
    v
}

fn label(a: Act, windows: usize) -> &'static str {
    t(match a {
        Act::Focus(_) => K::MenuOpen,
        Act::Open if windows > 0 => K::MenuNewWindow,
        Act::Open => K::MenuOpen,
        Act::Pin => K::MenuPin,
        Act::Unpin => K::MenuUnpin,
        Act::Close => K::MenuClose,
        Act::CloseAll => K::MenuCloseAll,
        Act::Tray => K::MenuTray,
    })
}

// Fila bajo `y` (relativo a la ventana), o None en el relleno.
pub fn row_at(y: i32, pad: i32, row: i32, n: usize) -> Option<usize> {
    let r = y - pad;
    (r >= 0 && r < row * n as i32).then_some((r / row) as usize)
}

// Esquina superior izquierda de un popup centrado sobre `cx`, del lado de la pantalla opuesto al borde del dock.
pub fn popup_pos(cx: i32, bar: R, w: i32, h: i32, bottom: bool, gap: i32, mon: MonRect) -> (i32, i32) {
    let x = (cx - w / 2).clamp(mon.0, (mon.2 - w).max(mon.0));
    let y = if bottom { bar.y - h - gap } else { bar.y + bar.h + gap };
    (x, y)
}

pub fn step(cur: Option<usize>, n: usize, down: bool) -> Option<usize> {
    if n == 0 {
        return None;
    }
    Some(match (cur, down) {
        (None, true) => 0,
        (None, false) => n - 1,
        (Some(i), true) => (i + 1) % n,
        (Some(i), false) => (i + n - 1) % n,
    })
}

pub struct Ctx {
    pub picker: bool,
    pub titles: Vec<String>,
    pub active: Option<isize>,
    pub theme: Theme,
    pub scale: f32,
    pub cx: i32,
    pub bar: R,
    pub bottom: bool,
    pub mon: MonRect,
    pub wins: Vec<isize>,
    pub last: Option<isize>,
    pub launch: Option<(String, String)>,
    pub key: String,
    pub pinned: bool,
    pub pinnable: bool,
    pub closable: bool,
    pub tray: bool,
}

struct Menu {
    hwnd: HWND,
    acts: Vec<Act>,
    labels: Vec<String>,
    wins: Vec<isize>,
    target: Option<isize>,
    launch: Option<(String, String)>,
    key: String,
    hover: Option<usize>,
    pos: POINT,
    size: (i32, i32),
    row: i32,
    pad: i32,
    theme: Theme,
    scale: f32,
    born: std::time::Instant,
    animate: bool,
    bottom: bool,
}

const OPEN_MS: f32 = 150.0;
const T_OPEN: usize = 1;

thread_local! { static MENU: RefCell<Option<Menu>> = const { RefCell::new(None) }; }

pub fn is_open() -> bool {
    MENU.with(|m| m.try_borrow().map(|m| m.is_some()).unwrap_or(true))
}

pub fn launch(path: &str, args: &str) {
    let wide = |t: &str| -> Vec<u16> { t.encode_utf16().chain(std::iter::once(0)).collect() };
    let (p, a) = (wide(path), wide(args));
    unsafe {
        let params = if args.is_empty() { PCWSTR::null() } else { PCWSTR(a.as_ptr()) };
        let _ = ShellExecuteW(None, w!("open"), PCWSTR(p.as_ptr()), params, PCWSTR::null(), SW_SHOWNORMAL);
    }
}

unsafe fn font_for(theme: &Theme, scale: f32) -> HFONT {
    let family: Vec<u16> = theme.font_family.encode_utf16().chain(std::iter::once(0)).collect();
    CreateFontW(-px((theme.font_size - 1).max(10), scale), 0, 0, 0, 500, 0, 0, 0, 1, 0, 0, ANTIALIASED_QUALITY.0 as u32, 0, PCWSTR(family.as_ptr()))
}

pub unsafe fn show(c: Ctx) {
    dismiss();
    let list = acts(&c.wins, Caps { launchable: c.launch.is_some(), pinned: c.pinned, pinnable: c.pinnable, picker: c.picker, closable: c.closable, tray: c.tray });
    if list.is_empty() {
        return;
    }
    let current = c.active.filter(|a| c.wins.contains(a)).or(c.last);
    let labels: Vec<String> = list
        .iter()
        .map(|a| match a {
            Act::Focus(id) => {
                let i = c.wins.iter().position(|w| w == id).unwrap_or(0);
                let title = c.titles.get(i).filter(|t| !t.is_empty()).cloned().unwrap_or_else(|| "…".into());
                format!("{}{title}", if current == Some(*id) { "• " } else { "   " })
            }
            other => label(*other, c.wins.len()).to_string(),
        })
        .collect();
    let (row, pad, padx) = (px(32, c.scale), px(6, c.scale), px(14, c.scale));
    let screen = GetDC(None);
    let mem = CreateCompatibleDC(screen);
    let font = font_for(&c.theme, c.scale);
    let old = SelectObject(mem, font);
    let mut widest = px(150, c.scale);
    for l in &labels {
        let wide: Vec<u16> = l.encode_utf16().collect();
        let mut e = SIZE::default();
        let _ = GetTextExtentPoint32W(mem, &wide, &mut e);
        widest = widest.max(e.cx + 2 * padx);
    }
    SelectObject(mem, old);
    let _ = DeleteObject(font);
    let _ = DeleteDC(mem);
    ReleaseDC(None, screen);

    let (w, h) = (widest, row * list.len() as i32 + 2 * pad);
    let (x, y) = popup_pos(c.cx, c.bar, w, h, c.bottom, px(8, c.scale), c.mon);
    let Ok(inst) = GetModuleHandleW(PCWSTR::null()) else { return };
    let class = WNDCLASSW { lpfnWndProc: Some(menu_proc), hInstance: inst.into(), hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(), lpszClassName: w!("WinarchyShellDockMenu"), ..Default::default() };
    RegisterClassW(&class);
    let Ok(hwnd) = CreateWindowExW(WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED, class.lpszClassName, w!(""), WS_POPUP, x, y, w, h, None, None, inst, None) else { return };
    let menu = Menu { hwnd, acts: list, labels, target: pick_target(&c.wins, 0, c.last), wins: c.wins, launch: c.launch, key: c.key, hover: None, pos: POINT { x, y }, size: (w, h), row, pad, animate: crate::dock::motion_enabled(&c.theme), theme: c.theme, scale: c.scale, born: std::time::Instant::now(), bottom: c.bottom };
    paint(&menu);
    let animate = menu.animate;
    MENU.with(|m| *m.borrow_mut() = Some(menu));
    if animate {
        SetTimer(hwnd, T_OPEN, 16, None);
    }
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
}

pub fn dismiss() {
    let m = MENU.with(|m| m.borrow_mut().take());
    if let Some(m) = m {
        unsafe {
            let _ = DestroyWindow(m.hwnd);
        }
    }
}

fn open_progress(m: &Menu) -> f32 {
    if m.animate { (m.born.elapsed().as_secs_f32() * 1000.0 / OPEN_MS).min(1.0) } else { 1.0 }
}

// Texto sobre negro y la luminancia pasa a alfa: GDI no escribe alfa propio en un DIB de 32 bits.
unsafe fn paint(m: &Menu) {
    let (w, h) = m.size;
    let s = m.scale;
    let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: 0, ..Default::default() }, ..Default::default() };
    let screen = GetDC(None);
    let mem = CreateCompatibleDC(screen);
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let Ok(dib) = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else {
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        return;
    };
    let old = SelectObject(mem, dib);
    let font = font_for(&m.theme, s);
    let old_font = SelectObject(mem, font);
    let buf = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
    buf.fill(0);
    SetBkMode(mem, TRANSPARENT);
    SetTextColor(mem, COLORREF(0x00FF_FFFF));
    let padx = px(14, s);
    for (i, l) in m.labels.iter().enumerate() {
        let mut wide: Vec<u16> = l.encode_utf16().collect();
        let mut rc = RECT { left: padx, top: m.pad + i as i32 * m.row, right: w - padx, bottom: m.pad + (i as i32 + 1) * m.row };
        DrawTextW(mem, &mut wide, &mut rc, DT_LEFT | DT_VCENTER | DT_NOPREFIX | DT_SINGLELINE);
    }
    let coverage: Vec<u32> = buf.iter().map(|p| ((*p >> 16) & 255).max((*p >> 8) & 255).max(*p & 255)).collect();
    buf.fill(0);
    let (bg, fg, accent) = (rgb(&m.theme.background), rgb(&m.theme.foreground), rgb(&m.theme.accent));
    fill_round_a(buf, w, h, R { x: 0, y: 0, w, h }, px(12, s), bg, 0.97);
    if let Some(i) = m.hover {
        let inset = px(5, s);
        fill_round_a(buf, w, h, R { x: inset, y: m.pad + i as i32 * m.row, w: w - 2 * inset, h: m.row }, px(8, s), accent, 0.30);
    }
    for (i, c) in coverage.iter().enumerate() {
        if *c > 0 {
            blend(&mut buf[i], premul(fg, *c as f32 / 255.0));
        }
    }
    let (sz, src) = (SIZE { cx: w, cy: h }, POINT::default());
    let e = crate::dock::ease(open_progress(m));
    let bf = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: (e * 255.0).round() as u8, AlphaFormat: 1 };
    // Entra deslizándose unos px desde el lado del dock.
    let off = ((1.0 - e) * px(10, s) as f32) as i32;
    let pos = POINT { x: m.pos.x, y: m.pos.y + if m.bottom { off } else { -off } };
    let _ = UpdateLayeredWindow(m.hwnd, None, Some(&pos), Some(&sz), mem, Some(&src), COLORREF(0), Some(&bf), ULW_ALPHA);
    SelectObject(mem, old_font);
    let _ = DeleteObject(font);
    SelectObject(mem, old);
    let _ = DeleteObject(dib);
    let _ = DeleteDC(mem);
    ReleaseDC(None, screen);
}

fn set_hover(i: Option<usize>) {
    let changed = MENU.with(|m| m.borrow_mut().as_mut().is_some_and(|m| std::mem::replace(&mut m.hover, i) != i));
    if changed {
        MENU.with(|m| m.borrow().as_ref().map(|m| unsafe { paint(m) }));
    }
}

fn activate(i: usize) {
    let m = MENU.with(|m| m.borrow_mut().take());
    let Some(m) = m else { return };
    unsafe {
        let _ = DestroyWindow(m.hwnd);
    }
    let close = |id: isize| unsafe {
        // PostMessage no espera: una app colgada no cuelga el dock.
        let _ = PostMessageW(HWND(id as *mut _), WM_CLOSE, WPARAM(0), LPARAM(0));
    };
    match m.acts.get(i) {
        Some(Act::Focus(id)) => unsafe {
            let h = HWND(*id as *mut _);
            if IsWindow(h).as_bool() {
                crate::dock::focus_window(h);
            }
        },
        Some(Act::Tray) => crate::dock::show_tray(),
        Some(Act::Open) => m.launch.iter().for_each(|(o, a)| launch(o, a)),
        Some(Act::Pin | Act::Unpin) => crate::dock::toggle_pin(&m.key),
        Some(Act::Close) => m.target.into_iter().for_each(close),
        Some(Act::CloseAll) => m.wins.iter().copied().for_each(close),
        None => {}
    }
}

fn y_of(lp: LPARAM) -> i32 {
    ((lp.0 >> 16) & 0xffff) as i16 as i32
}

unsafe extern "system" fn menu_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let rows = |lp: LPARAM| MENU.with(|m| m.try_borrow().ok().and_then(|m| m.as_ref().and_then(|m| row_at(y_of(lp), m.pad, m.row, m.acts.len()))));
    match msg {
        WM_TIMER if wp.0 == T_OPEN => {
            let done = MENU.with(|m| m.try_borrow().ok().and_then(|m| m.as_ref().map(|m| unsafe { paint(m); open_progress(m) >= 1.0 })));
            if done != Some(false) {
                let _ = KillTimer(h, T_OPEN);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            set_hover(rows(lp));
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            match rows(lp) {
                Some(i) => activate(i),
                None => dismiss(),
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let n = MENU.with(|m| m.try_borrow().ok().and_then(|m| m.as_ref().map(|m| (m.acts.len(), m.hover))));
            match (wp.0 as u16, n) {
                (k, _) if k == VK_ESCAPE.0 => dismiss(),
                (k, Some((n, cur))) if k == VK_DOWN.0 || k == VK_UP.0 => set_hover(step(cur, n, k == VK_DOWN.0)),
                (k, Some((_, Some(i)))) if k == VK_RETURN.0 => activate(i),
                _ => {}
            }
            LRESULT(0)
        }
        // WA_INACTIVE: otra ventana tomó el foco.
        WM_ACTIVATE if wp.0 & 0xffff == 0 => {
            dismiss();
            LRESULT(0)
        }
        _ => DefWindowProcW(h, msg, wp, lp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(l: bool, pinned: bool, pinnable: bool, picker: bool) -> Caps {
        Caps { launchable: l, pinned, pinnable, picker, closable: true, tray: false }
    }

    #[test]
    fn entries_follow_the_state_of_the_cell() {
        assert_eq!(acts(&[], caps(true, true, true, false)), [Act::Open, Act::Unpin]);
        assert_eq!(acts(&[1], caps(true, false, true, false)), [Act::Open, Act::Pin, Act::Close]);
        assert_eq!(acts(&[1, 2, 3], caps(true, false, false, false)), [Act::Focus(1), Act::Focus(2), Act::Focus(3), Act::Open, Act::Close, Act::CloseAll]);
        assert_eq!(acts(&[1, 2], caps(false, false, false, false)), [Act::Focus(1), Act::Focus(2), Act::Close, Act::CloseAll]);
        assert!(acts(&[], caps(false, false, false, false)).is_empty());
    }

    #[test]
    fn the_picker_lists_only_the_windows() {
        assert_eq!(acts(&[7, 8], caps(true, true, true, true)), [Act::Focus(7), Act::Focus(8)]);
    }

    #[test]
    fn elevated_windows_cannot_be_closed_and_the_tray_entry_is_last() {
        let c = Caps { closable: false, tray: true, ..caps(true, false, false, false) };
        assert_eq!(acts(&[1, 2], c), [Act::Focus(1), Act::Focus(2), Act::Open, Act::Tray]);
    }

    #[test]
    fn rows_are_hit_inside_the_padding_only() {
        assert_eq!(row_at(5, 6, 32, 3), None);
        assert_eq!(row_at(6, 6, 32, 3), Some(0));
        assert_eq!(row_at(6 + 32, 6, 32, 3), Some(1));
        assert_eq!(row_at(6 + 96, 6, 32, 3), None);
    }

    #[test]
    fn popup_stays_on_the_monitor_and_flips_with_the_dock_edge() {
        let bar = R { x: 800, y: 1000, w: 300, h: 48 };
        let mon = (0, 0, 1920, 1080);
        assert_eq!(popup_pos(10, bar, 200, 100, true, 8, mon), (0, 892));
        assert_eq!(popup_pos(1915, bar, 200, 100, true, 8, mon).0, 1720);
        assert_eq!(popup_pos(960, R { y: 6, ..bar }, 200, 100, false, 8, mon).1, 6 + 48 + 8);
    }

    #[test]
    fn arrows_wrap_around() {
        assert_eq!(step(None, 3, true), Some(0));
        assert_eq!(step(None, 3, false), Some(2));
        assert_eq!(step(Some(2), 3, true), Some(0));
        assert_eq!(step(Some(0), 3, false), Some(2));
        assert_eq!(step(None, 0, true), None);
    }
}
