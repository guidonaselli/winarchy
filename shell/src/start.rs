// Menú Inicio propio: pineados, todas las apps (AppsFolder), búsqueda por teclado y energía.
// Sin Explorer no existe el Inicio de Windows; esta ventana lo reemplaza y se cierra al perder el foco.
// shortcut: sin recomendados ni (des)anclar desde la UI (los pines se editan en state/start-pins.json), sumar si hace falta.
use crate::dock::{approach, blend, draw_label, draw_text, ease, fill_round_a, style, HOVER_MS};
use crate::i18n::{t, K};
use crate::layout::{px, R};
use crate::start_model::{self as model, App, LIST_ROWS, PIN_COLS};
use crate::theme::{rgb, Theme};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Mutex;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const ICON_DIP: i32 = 32;
const SLIDE_DIP: i32 = 10;
const OPEN_MS: f32 = 160.0;
const ANIM_MS: u32 = 16;
const WATCH_MS: u32 = 100;
const GRACE_MS: u128 = 400;
const REFRESH_AFTER_SECS: u64 = 120;
const T_ANIM: usize = 1;
const T_WATCH: usize = 2;
const WM_SYNC: u32 = WM_APP + 43;
const POWER_GLYPH: &str = "\u{E7E8}";
const ICONS_FONT: &str = "Segoe MDL2 Assets";
pub const CLASS: PCWSTR = w!("WinarchyShellStart");

static APPS: Mutex<Vec<App>> = Mutex::new(Vec::new());
static ICONS: Mutex<Option<HashMap<String, crate::dock::Icon>>> = Mutex::new(None);
static PINS_PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
static LOADING: AtomicBool = AtomicBool::new(false);
static HWND_ATOMIC: AtomicIsize = AtomicIsize::new(0);
static LOADED_AT: Mutex<Option<std::time::Instant>> = Mutex::new(None);

#[derive(Clone, Copy, PartialEq, Debug)]
enum Hit {
    Search,
    All,
    Cell(usize),
    Row(usize),
    Power,
}

struct Menu {
    hwnd: HWND,
    theme: Theme,
    scale: f32,
    pos: (i32, i32),
    mon: (i32, i32, i32, i32),
    bottom: bool,
    open: bool,
    opened_at: std::time::Instant,
    vis: f32,
    all: bool,
    query: String,
    sel: Option<usize>,
    scroll: usize,
    hover: Option<Hit>,
    hover_t: f32,
    last: Option<std::time::Instant>,
    anim_on: bool,
    in_menu: bool,
    pins_path: std::path::PathBuf,
}

thread_local! {
    static MENU: RefCell<Option<Menu>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Menu) -> R) -> Option<R> {
    MENU.with(|c| c.try_borrow_mut().ok().and_then(|mut c| c.as_mut().map(f)))
}

// Índices (en APPS) de lo que muestra hoy el menú, en el orden en que se ven.
fn visible(m: &Menu, apps: &[App]) -> Vec<usize> {
    if !m.query.trim().is_empty() {
        model::filter(apps, &m.query)
    } else if m.all {
        (0..apps.len()).collect()
    } else {
        model::pinned_indices(apps, model::load_pins(&m.pins_path))
    }
}

fn list_mode(m: &Menu) -> bool {
    m.all || !m.query.trim().is_empty()
}

// Se crea oculto al arrancar la shell; `root` es la raíz de Winarchy (de ahí sale `state/`).
pub fn init(root: &std::path::Path, theme: &Theme) {
    let _ = PINS_PATH.set(root.join("state").join("start-pins.json"));
    unsafe {
        let Ok(inst) = GetModuleHandleW(PCWSTR::null()) else { return };
        let class = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: inst.into(), hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(), lpszClassName: CLASS, style: CS_DBLCLKS, ..Default::default() };
        RegisterClassW(&class);
        let ex = WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_LAYERED;
        let Ok(hwnd) = CreateWindowExW(ex, CLASS, w!("Winarchy Start"), WS_POPUP, 0, 0, 1, 1, None, None, inst, None) else { return };
        crate::accessibility::create(hwnd, "Winarchy start", accesskit::Role::Menu);
        HWND_ATOMIC.store(hwnd.0 as isize, Ordering::Relaxed);
        let m = Menu {
            hwnd, theme: theme.clone(), scale: 1.0, pos: (0, 0), mon: (0, 0, 0, 0), bottom: true, open: false, opened_at: std::time::Instant::now(), vis: 0.0, all: false, query: String::new(),
            sel: None, scroll: 0, hover: None, hover_t: 0.0, last: None, anim_on: false, in_menu: false, pins_path: root.join("state").join("start-pins.json"),
        };
        MENU.with(|c| *c.borrow_mut() = Some(m));
    }
    refresh();
}

// Enumera AppsFolder (lo mismo que "Todas las apps" de Windows) y precarga los íconos en un hilo aparte.
fn refresh() {
    if LOADING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        unsafe {
            use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if let Some(mut apps) = enumerate() {
                model::sort_dedupe(&mut apps);
                *APPS.lock().unwrap() = apps.clone();
                *LOADED_AT.lock().unwrap() = Some(std::time::Instant::now());
                notify();
                let pins = with_pins(&apps);
                let order = pins.iter().copied().chain((0..apps.len()).filter(|i| !pins.contains(i)));
                for (n, i) in order.enumerate() {
                    let open = &apps[i].open;
                    if ICONS.lock().unwrap().as_ref().is_some_and(|c| c.contains_key(open)) {
                        continue;
                    }
                    if let Some(icon) = crate::dock::fetch_icon(open, 64) {
                        ICONS.lock().unwrap().get_or_insert_with(HashMap::new).insert(open.clone(), icon);
                    }
                    if n % 12 == 11 {
                        notify();
                    }
                }
                notify();
            }
            CoUninitialize();
        }
        LOADING.store(false, Ordering::SeqCst);
    });
}

fn with_pins(apps: &[App]) -> Vec<usize> {
    model::pinned_indices(apps, PINS_PATH.get().and_then(|p| model::load_pins(p)))
}

unsafe fn enumerate() -> Option<Vec<App>> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::*;
    let text = |item: &IShellItem, kind: SIGDN| -> Option<String> {
        let p = item.GetDisplayName(kind).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s
    };
    let folder: IShellItem = SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None).ok()?;
    let items: IEnumShellItems = folder.BindToHandler(None, &BHID_EnumItems).ok()?;
    let mut apps = Vec::new();
    loop {
        let mut slot: [Option<IShellItem>; 1] = [None];
        let mut got = 0u32;
        if items.Next(&mut slot, Some(&mut got)).is_err() || got == 0 {
            break;
        }
        let Some(item) = slot[0].take() else { break };
        let (Some(name), Some(parse)) = (text(&item, SIGDN_NORMALDISPLAY), text(&item, SIGDN_DESKTOPABSOLUTEPARSING)) else { continue };
        let lower = name.to_lowercase();
        if name.is_empty() || lower.contains("uninstall") || lower.contains("desinstalar") {
            continue;
        }
        apps.push(App { name, open: format!(r"shell:AppsFolder\{parse}") });
    }
    (!apps.is_empty()).then_some(apps)
}

fn notify() {
    let h = HWND_ATOMIC.load(Ordering::Relaxed);
    if h != 0 {
        unsafe {
            let _ = PostMessageW(HWND(h as *mut _), WM_SYNC, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn available() -> bool {
    HWND_ATOMIC.load(Ordering::Relaxed) != 0
}

pub fn is_open() -> bool {
    with(|m| m.open).unwrap_or(false)
}

// Abre el menú sobre el dock (centrado en el monitor) o lo cierra si ya estaba abierto.
pub fn toggle(anchor: R, mon: (i32, i32, i32, i32), bottom: bool, scale: f32, theme: &Theme) {
    if is_open() {
        return close();
    }
    let stale = LOADED_AT.lock().unwrap().is_none_or(|t| t.elapsed().as_secs() > REFRESH_AFTER_SECS);
    if stale {
        refresh();
    }
    let l = model::layout(scale);
    let (_, y) = crate::tray_flyout::place(anchor, mon, bottom, l.size, scale);
    let x = mon.0 + (mon.2 - mon.0 - l.size.0) / 2;
    let Some(h) = with(|m| {
        m.theme = theme.clone();
        m.scale = scale;
        m.mon = mon;
        m.bottom = bottom;
        m.pos = (x, y);
        m.open = true;
        m.opened_at = std::time::Instant::now();
        m.all = false;
        m.query.clear();
        m.scroll = 0;
        m.sel = None;
        m.hover = None;
        m.hwnd
    }) else {
        return;
    };
    unsafe {
        SetTimer(h, T_WATCH, WATCH_MS, None);
        start_anim(h);
    }
    paint();
    unsafe {
        let _ = SetForegroundWindow(h);
        let _ = SetFocus(h);
    }
}

pub fn close() {
    if let Some(h) = with(|m| {
        m.open = false;
        m.hwnd
    }) {
        unsafe {
            let _ = KillTimer(h, T_WATCH);
            start_anim(h);
        }
    }
}

unsafe fn start_anim(h: HWND) {
    if with(|m| std::mem::replace(&mut m.anim_on, true)) == Some(false) {
        SetTimer(h, T_ANIM, ANIM_MS, None);
    }
}

unsafe fn anim_tick(h: HWND) {
    let busy = with(|m| {
        let dt = m.last.map_or(ANIM_MS as f32, |l| l.elapsed().as_secs_f32() * 1000.0).min(64.0);
        m.last = Some(std::time::Instant::now());
        let target = if m.open { 1.0 } else { 0.0 };
        m.vis = approach(m.vis, target, dt, OPEN_MS);
        m.hover_t = approach(m.hover_t, if m.hover.is_some() { 1.0 } else { 0.0 }, dt, HOVER_MS);
        m.vis != target || (m.hover.is_some() && m.hover_t < 1.0) || (m.hover.is_none() && m.hover_t > 0.0)
    })
    .unwrap_or(false);
    paint();
    if !busy {
        let _ = KillTimer(h, T_ANIM);
        with(|m| {
            m.anim_on = false;
            m.last = None;
        });
    }
}

// Mientras está abierto, perder el primer plano lo cierra: el dock no activa ventanas y WM_ACTIVATE no alcanza.
unsafe fn watch(h: HWND) {
    let Some((open, since, in_menu)) = with(|m| (m.open, m.opened_at.elapsed().as_millis(), m.in_menu)) else { return };
    if open && !in_menu && since > GRACE_MS && GetForegroundWindow() != h {
        close();
    }
}

fn icon_pixels(open: &str, side: i32) -> Option<Vec<u32>> {
    let guard = ICONS.lock().unwrap();
    Some(scale_icon(guard.as_ref()?.get(open)?, side))
}

pub(crate) fn scale_icon(icon: &crate::dock::Icon, side: i32) -> Vec<u32> {
    let mut bgra = Vec::with_capacity(icon.px.len() * 4);
    for p in &icon.px {
        // `Icon` guarda alfa premultiplicado; `scaled` espera alfa recto.
        let a = p >> 24;
        let un = |c: u32| if a == 0 { 0 } else { ((c * 255 + a / 2) / a).min(255) as u8 };
        bgra.extend_from_slice(&[un(p & 255), un((p >> 8) & 255), un((p >> 16) & 255), a as u8]);
    }
    crate::tray_flyout::scaled(&bgra, (icon.w as u32, icon.h as u32), side)
}

pub(crate) fn blit(buf: &mut [u32], (w, h): (i32, i32), x: i32, y: i32, side: i32, pixels: &[u32]) {
    for (k, p) in pixels.iter().enumerate() {
        let (dx, dy) = (x + k as i32 % side, y + k as i32 / side);
        if dx >= 0 && dx < w && dy >= 0 && dy < h {
            blend(&mut buf[(dy * w + dx) as usize], *p);
        }
    }
}

fn user_name() -> String {
    std::env::var("USERNAME").unwrap_or_default()
}

fn paint() {
    let Some((hwnd, theme, scale, vis, pos, hover, hover_t, query, all, sel, scroll, is_list)) = with(|m| (m.hwnd, m.theme.clone(), m.scale, m.vis, m.pos, m.hover, m.hover_t, m.query.clone(), m.all, m.sel, m.scroll, list_mode(m))) else { return };
    unsafe {
        if vis <= 0.0 && !is_open() {
            let _ = ShowWindow(hwnd, SW_HIDE);
            return;
        }
        let l = model::layout(scale);
        let apps = APPS.lock().unwrap().clone();
        let items = with(|m| visible(m, &apps)).unwrap_or_default();
        let (w, h) = l.size;
        let (bg, fg, muted, accent) = (rgb(&theme.background), rgb(&theme.foreground), rgb(&theme.muted), rgb(&theme.accent));
        let st = style(&theme);
        let font = (theme.font_size).max(12);
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(screen);
        let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: 0, ..Default::default() }, ..Default::default() };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        if let Ok(dib) = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            let old = SelectObject(mem, dib);
            let buf = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
            buf.fill(0);
            fill_round_a(buf, w, h, R { x: 0, y: 0, w, h }, px(st.radius.max(12), scale), bg, st.opacity);
            let pad_text = px(12, scale);
            let shade = |buf: &mut [u32], r: R, alpha: f32| fill_round_a(buf, w, h, r, px(8, scale), fg, alpha);

            shade(buf, l.search, 0.08);
            let (text, color) = if query.is_empty() { (t(K::StartSearch).to_string(), muted) } else { (format!("{query}|"), fg) };
            draw_text(buf, w, h, R { x: l.search.x + pad_text, ..l.search }, &text, &theme.font_family, scale, color, font, DT_LEFT);

            let heading = if !query.is_empty() { "" } else if all { t(K::StartAllApps).trim_end_matches(['>', ' ']) } else { t(K::StartPinned) };
            draw_text(buf, w, h, R { x: l.header.x + pad_text, ..l.header }, heading, &theme.font_family, scale, fg, font, DT_LEFT);
            if query.is_empty() {
                if hover == Some(Hit::All) {
                    shade(buf, l.all_button, 0.10 * ease(hover_t));
                }
                draw_label(buf, w, h, l.all_button, if all { t(K::StartBack) } else { t(K::StartAllApps) }, &theme.font_family, scale, fg, font - 1);
            }

            let side = px(ICON_DIP, scale);
            let on = |hit: Hit, index: usize| hover == Some(hit) || sel == Some(index);
            if items.is_empty() && is_list {
                draw_label(buf, w, h, R { y: l.rows[2].y, h: l.rows[0].h, ..l.rows[0] }, t(K::StartNoResults), &theme.font_family, scale, muted, font);
            }
            if is_list {
                let row_side = px(24, scale);
                for (n, row) in l.rows.iter().enumerate() {
                    let index = scroll + n;
                    let Some(app) = items.get(index).and_then(|i| apps.get(*i)) else { break };
                    if on(Hit::Row(index), index) {
                        shade(buf, *row, if sel == Some(index) { 0.16 } else { 0.10 * ease(hover_t) });
                    }
                    if let Some(p) = icon_pixels(&app.open, row_side) {
                        blit(buf, (w, h), row.x + pad_text, row.y + (row.h - row_side) / 2, row_side, &p);
                    }
                    draw_text(buf, w, h, R { x: row.x + pad_text * 2 + row_side, w: row.w - pad_text * 3 - row_side, ..*row }, &app.name, &theme.font_family, scale, fg, font, DT_LEFT);
                }
            } else {
                for (n, cell) in l.cells.iter().enumerate() {
                    let Some(app) = items.get(n).and_then(|i| apps.get(*i)) else { break };
                    if on(Hit::Cell(n), n) {
                        shade(buf, *cell, if sel == Some(n) { 0.16 } else { 0.10 * ease(hover_t) });
                    }
                    if let Some(p) = icon_pixels(&app.open, side) {
                        blit(buf, (w, h), cell.x + (cell.w - side) / 2, cell.y + px(10, scale), side, &p);
                    }
                    let label = R { x: cell.x + px(2, scale), y: cell.y + px(10, scale) + side + px(4, scale), w: cell.w - px(4, scale), h: px(22, scale) };
                    draw_label(buf, w, h, label, &app.name, &theme.font_family, scale, fg, font - 2);
                }
            }

            fill_round_a(buf, w, h, R { x: 0, y: l.footer.y, w, h: px(1, scale) }, 0, fg, 0.10);
            let badge = R { x: l.footer.x + px(20, scale), y: l.footer.y + (l.footer.h - px(32, scale)) / 2, w: px(32, scale), h: px(32, scale) };
            fill_round_a(buf, w, h, badge, badge.w / 2, accent, 1.0);
            let name = user_name();
            let initial: String = name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
            draw_label(buf, w, h, badge, &initial, &theme.font_family, scale, crate::theme::contrasting_color(accent), font);
            draw_text(buf, w, h, R { x: badge.x + badge.w + px(10, scale), y: l.footer.y, w: px(260, scale), h: l.footer.h }, &name, &theme.font_family, scale, fg, font, DT_LEFT);
            if hover == Some(Hit::Power) {
                shade(buf, l.power, 0.12 * ease(hover_t));
            }
            draw_label(buf, w, h, l.power, POWER_GLYPH, ICONS_FONT, scale, fg, 16);

            let slide = ((1.0 - ease(vis)) * px(SLIDE_DIP, scale) as f32) as i32;
            let target = POINT { x: pos.0, y: pos.1 + slide };
            let sz = SIZE { cx: w, cy: h };
            let src = POINT::default();
            let bf = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: (vis.clamp(0.0, 1.0) * 255.0).round() as u8, AlphaFormat: 1 };
            let _ = UpdateLayeredWindow(hwnd, None, Some(&target), Some(&sz), mem, Some(&src), COLORREF(0), Some(&bf), ULW_ALPHA);
            SelectObject(mem, old);
            let _ = DeleteObject(dib);
        }
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        if !IsWindowVisible(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_SHOWNA);
        }
        publish(hwnd, &l, &apps, &items, is_list, scroll, &query, all);
    }
}

fn publish(hwnd: HWND, l: &model::Layout, apps: &[App], items: &[usize], is_list: bool, scroll: usize, query: &str, all: bool) {
    let rects = if is_list { &l.rows } else { &l.cells };
    let first = if is_list { scroll } else { 0 };
    let mut nodes: Vec<(String, accesskit::Node)> = rects
        .iter()
        .enumerate()
        .filter_map(|(n, r)| {
            let app = items.get(first + n).and_then(|i| apps.get(*i))?;
            let mut node = accesskit::Node::new(accesskit::Role::MenuItem);
            node.set_label(app.name.clone());
            node.set_bounds(accesskit::Rect::new(r.x as f64, r.y as f64, (r.x + r.w) as f64, (r.y + r.h) as f64));
            node.add_action(accesskit::Action::Click);
            Some((app.open.clone(), node))
        })
        .collect();
    let bounds = |r: &R| accesskit::Rect::new(r.x as f64, r.y as f64, (r.x + r.w) as f64, (r.y + r.h) as f64);
    let mut search = accesskit::Node::new(accesskit::Role::SearchInput);
    search.set_label(t(K::StartSearch).to_string());
    search.set_value(query.to_string());
    search.set_bounds(bounds(&l.search));
    nodes.push(("search".to_string(), search));
    if query.trim().is_empty() {
        let mut toggle = accesskit::Node::new(accesskit::Role::Button);
        toggle.set_label(if all { t(K::StartPinned) } else { t(K::StartAllApps) }.trim_end_matches(['>', ' ']).to_string());
        toggle.set_bounds(bounds(&l.all_button));
        toggle.add_action(accesskit::Action::Click);
        nodes.push(("all".to_string(), toggle));
    }
    let mut power = accesskit::Node::new(accesskit::Role::Button);
    power.set_label(t(K::PowerShutdown).to_string());
    power.set_bounds(accesskit::Rect::new(l.power.x as f64, l.power.y as f64, (l.power.x + l.power.w) as f64, (l.power.y + l.power.h) as f64));
    power.add_action(accesskit::Action::Click);
    nodes.push(("power".to_string(), power));
    crate::accessibility::publish(hwnd, "Winarchy start", l.size, nodes, None);
}

fn hit_at(x: i32, y: i32) -> Option<Hit> {
    let (scale, list, scroll) = with(|m| (m.scale, list_mode(m), m.scroll))?;
    let l = model::layout(scale);
    let inside = |r: &R| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h;
    if inside(&l.power) {
        return Some(Hit::Power);
    }
    if inside(&l.search) {
        return Some(Hit::Search);
    }
    if inside(&l.all_button) && with(|m| m.query.trim().is_empty()).unwrap_or(false) {
        return Some(Hit::All);
    }
    if list {
        l.rows.iter().position(inside).map(|n| Hit::Row(scroll + n))
    } else {
        l.cells.iter().position(inside).map(Hit::Cell)
    }
}

fn launch_at(position: usize) {
    let apps = APPS.lock().unwrap().clone();
    let open = with(|m| visible(m, &apps).get(position).and_then(|i| apps.get(*i)).map(|a| a.open.clone())).flatten();
    if let Some(open) = open {
        close();
        crate::dock_menu::launch(&open, "");
    }
}

fn set_all(all: bool) {
    with(|m| {
        m.all = all;
        m.scroll = 0;
        m.sel = None;
    });
    paint();
}

fn move_selection(dx: i32, dy: i32) {
    let apps = APPS.lock().unwrap().clone();
    with(|m| {
        let len = visible(m, &apps).len();
        let cols = if list_mode(m) { 1 } else { PIN_COLS };
        m.sel = model::step(m.sel, len, dx, dy, cols);
        if let Some(s) = m.sel {
            m.scroll = if list_mode(m) { model::scroll_to(m.scroll, s) } else { 0 };
        }
    });
    paint();
}

fn edit_query(f: impl FnOnce(&mut String)) {
    with(|m| {
        f(&mut m.query);
        m.scroll = 0;
        m.sel = (!m.query.trim().is_empty()).then_some(0);
    });
    paint();
}

unsafe fn power_menu(h: HWND) {
    let Some((scale, pos)) = with(|m| {
        m.in_menu = true;
        (m.scale, m.pos)
    }) else {
        return;
    };
    let l = model::layout(scale);
    let Ok(menu) = CreatePopupMenu() else { return };
    let items = [(1usize, t(K::PowerLock)), (2, t(K::PowerSleep)), (3, t(K::PowerShutdown)), (4, t(K::PowerRestart)), (5, t(K::PowerSignOut))];
    for (id, label) in items {
        let wide: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = AppendMenuW(menu, MF_STRING, id, PCWSTR(wide.as_ptr()));
    }
    let picked = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_BOTTOMALIGN | TPM_RIGHTALIGN, pos.0 + l.power.x + l.power.w, pos.1 + l.power.y, 0, h, None);
    let _ = DestroyMenu(menu);
    with(|m| {
        m.in_menu = false;
        m.opened_at = std::time::Instant::now();
    });
    let args: Option<(&str, &[&str])> = match picked.0 {
        1 => Some(("rundll32.exe", &["user32.dll,LockWorkStation"])),
        2 => Some(("rundll32.exe", &["powrprof.dll,SetSuspendState", "0,1,0"])),
        3 => Some(("shutdown.exe", &["/s", "/t", "0"])),
        4 => Some(("shutdown.exe", &["/r", "/t", "0"])),
        5 => Some(("shutdown.exe", &["/l"])),
        _ => None,
    };
    if let Some((exe, a)) = args {
        use std::os::windows::process::CommandExt;
        close();
        let _ = std::process::Command::new(exe).args(a).creation_flags(0x0800_0000).spawn();
    }
}

fn client(lp: LPARAM) -> (i32, i32) {
    ((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32)
}

unsafe extern "system" fn proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if m == WM_GETOBJECT {
        if let Some(r) = crate::accessibility::get_object(h, wp, lp) {
            return r;
        }
    }
    match m {
        WM_NCDESTROY => crate::accessibility::destroy(h),
        WM_TIMER if wp.0 == T_ANIM => {
            anim_tick(h);
            return LRESULT(0);
        }
        WM_TIMER if wp.0 == T_WATCH => {
            watch(h);
            return LRESULT(0);
        }
        WM_SYNC => {
            if is_open() {
                paint();
            }
            return LRESULT(0);
        }
        WM_MOUSEMOVE => {
            let (x, y) = client(lp);
            let hit = hit_at(x, y);
            if with(|s| std::mem::replace(&mut s.hover, hit)) != Some(hit) {
                start_anim(h);
            }
            return LRESULT(0);
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp.0 >> 16) & 0xFFFF) as i16 as i32;
            let apps = APPS.lock().unwrap().clone();
            with(|s| {
                if list_mode(s) {
                    let max = visible(s, &apps).len().saturating_sub(LIST_ROWS);
                    s.scroll = (s.scroll as i32 - delta.signum() * 3).clamp(0, max as i32) as usize;
                }
            });
            paint();
            return LRESULT(0);
        }
        WM_LBUTTONUP => {
            let (x, y) = client(lp);
            match hit_at(x, y) {
                Some(Hit::All) => set_all(!with(|s| s.all).unwrap_or(false)),
                Some(Hit::Power) => power_menu(h),
                Some(Hit::Cell(n)) | Some(Hit::Row(n)) => launch_at(n),
                _ => {}
            }
            return LRESULT(0);
        }
        WM_KEYDOWN => {
            match VIRTUAL_KEY(wp.0 as u16) {
                VK_ESCAPE => {
                    if with(|s| s.query.is_empty()).unwrap_or(true) {
                        close();
                    } else {
                        edit_query(String::clear);
                    }
                }
                VK_RETURN => {
                    let sel = with(|s| s.sel).flatten();
                    let empty = with(|s| s.query.trim().is_empty()).unwrap_or(true);
                    if let Some(n) = sel.or(if empty { None } else { Some(0) }) {
                        launch_at(n);
                    }
                }
                VK_BACK => edit_query(|q| {
                    q.pop();
                }),
                VK_UP => move_selection(0, -1),
                VK_DOWN => move_selection(0, 1),
                VK_LEFT => move_selection(-1, 0),
                VK_RIGHT => move_selection(1, 0),
                VK_TAB => set_all(!with(|s| s.all).unwrap_or(false)),
                _ => {}
            }
            return LRESULT(0);
        }
        WM_CHAR => {
            if let Some(c) = char::from_u32(wp.0 as u32).filter(|c| !c.is_control()) {
                edit_query(|q| q.push(c));
            }
            return LRESULT(0);
        }
        m if m == crate::accessibility::WM_ACTION => {
            for (key, action) in crate::accessibility::actions(h, wp.0) {
                if action != accesskit::Action::Click {
                    continue;
                }
                if key == "power" {
                    power_menu(h);
                } else if key == "all" {
                    set_all(!with(|s| s.all).unwrap_or(false));
                } else if key == "search" {
                    continue;
                } else {
                    close();
                    crate::dock_menu::launch(&key, "");
                }
            }
            return LRESULT(0);
        }
        _ => {}
    }
    DefWindowProcW(h, m, wp, lp)
}
