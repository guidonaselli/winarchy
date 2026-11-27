// Menú Inicio propio: pineados, todas las apps (AppsFolder), búsqueda por teclado y energía.
// Sin Explorer no existe el Inicio de Windows; esta ventana lo reemplaza y se cierra al perder el foco.
// Pineados: apps en la grilla y carpetas, archivos o .url en la franja de lugares; se anclan soltándolos sobre el logo del dock y se quitan con el menú contextual.
// shortcut: sin recomendados ni recientes (Recent está vacío sin Explorer), sin renombrar ni reordenar, y agregar un lugar solo por arrastre; sumar si hace falta.
use crate::dock::{approach, blend, draw_label, draw_text, ease, fill_round_a, style, HOVER_MS};
use crate::i18n::{t, K};
use crate::layout::{px, R};
use crate::start_model::{self as model, App, LIST_ROWS};
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
    Chip(usize),
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
    search: model::Search,
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

struct View {
    cat: Vec<App>,
    items: Vec<usize>,
    grid_len: usize,
}

fn saved_list(path: &std::path::Path, apps: &[App]) -> Vec<String> {
    match model::read_pins(path) {
        model::Saved::Ok(v) => v,
        _ => model::default_pins(apps),
    }
}

// Lo que muestra hoy el menú: catálogo (apps y lugares anclados) e índices en el orden visible; los primeros `grid_len` van en la grilla y el resto en la franja.
fn view(m: &Menu) -> View {
    let apps = APPS.lock().unwrap().clone();
    let saved = saved_list(&m.pins_path, &apps);
    let cat = model::catalog(&apps, &saved);
    let (items, grid_len) = if !m.search.text.trim().is_empty() {
        let found = model::filter(&cat, &m.search.text);
        let n = found.len();
        (found, n)
    } else if m.all {
        ((0..apps.len()).collect(), apps.len())
    } else {
        let (mut grid, places) = model::pinned(&cat, &saved);
        let n = grid.len();
        grid.extend(places);
        (grid, n)
    };
    View { cat, items, grid_len }
}

fn list_mode(m: &Menu) -> bool {
    m.all || !m.search.text.trim().is_empty()
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
            hwnd, theme: theme.clone(), scale: 1.0, pos: (0, 0), mon: (0, 0, 0, 0), bottom: true, open: false, opened_at: std::time::Instant::now(), vis: 0.0, all: false, search: Default::default(),
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
                let saved = PINS_PATH.get().map_or_else(Vec::new, |p| saved_list(p, &apps));
                let cat = model::catalog(&apps, &saved);
                let (grid, places) = model::pinned(&cat, &saved);
                let order = places.iter().chain(&grid).copied().chain((0..apps.len()).filter(|i| !grid.contains(i)));
                for (n, i) in order.enumerate() {
                    cache_icon(&cat[i].open);
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

// Un recurso de red puede tardar minutos en responder: su ícono es el glifo, nunca una consulta al shell.
fn is_remote(open: &str) -> bool {
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;
    const DRIVE_REMOTE: u32 = 4;
    if open.starts_with(r"\\") || open.contains("://") {
        return true;
    }
    let b = open.as_bytes();
    if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        let root: Vec<u16> = format!("{}:\\", b[0] as char).encode_utf16().chain(std::iter::once(0)).collect();
        return unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) } == DRIVE_REMOTE;
    }
    false
}

fn cache_icon(open: &str) {
    if ICONS.lock().unwrap().as_ref().is_some_and(|c| c.contains_key(open)) || (!model::is_app(open) && is_remote(open)) {
        return;
    }
    if let Some(icon) = unsafe { crate::dock::fetch_icon(open, 64) } {
        ICONS.lock().unwrap().get_or_insert_with(HashMap::new).insert(open.to_string(), icon);
    }
}

// Íconos de los lugares recién anclados, en un hilo propio: consultar el shell no debe frenar al llamador.
fn fetch_place_icons() {
    std::thread::spawn(|| unsafe {
        use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if let Some(model::Saved::Ok(saved)) = PINS_PATH.get().map(|p| model::read_pins(p)) {
            saved.iter().filter(|o| !model::is_app(o)).for_each(|o| cache_icon(o));
        }
        notify();
        CoUninitialize();
    });
}

// Edita la lista guardada sin perder pines: no escribe sobre un JSON roto ni antes de conocer las apps.
fn edit_pins(f: impl FnOnce(Vec<String>) -> Vec<String>) -> bool {
    let Some(path) = PINS_PATH.get() else { return false };
    let apps = APPS.lock().unwrap().clone();
    let Some(base) = model::base_for_write(model::read_pins(path), &apps) else { return false };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let ok = model::save_pins(path, &f(base)).is_ok();
    notify();
    fetch_place_icons();
    ok
}

// Ancla carpetas, archivos o accesos .url soltados sobre el logo del dock.
pub fn pin_paths(paths: Vec<String>) -> bool {
    edit_pins(|base| paths.iter().fold(base, |b, p| model::pin(b, p)))
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

pub fn set_theme(theme: &Theme) {
    if with(|m| {
        m.theme = theme.clone();
        m.open
    }) == Some(true) {
        paint();
    }
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
        m.search = Default::default();
        m.scroll = 0;
        m.sel = None;
        m.hover = None;
        m.hwnd
    }) else {
        return;
    };
    unsafe {
        SetTimer(h, T_WATCH, WATCH_MS, None);
        if !start_anim(h) {
            paint();
        }
    }
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

unsafe fn start_anim(h: HWND) -> bool {
    let animate = with(|m| crate::dock::motion_enabled(&m.theme)).unwrap_or(false);
    if !animate {
        let _ = KillTimer(h, T_ANIM);
        with(|m| {
            m.vis = if m.open { 1.0 } else { 0.0 };
            m.hover_t = if m.hover.is_some() { 1.0 } else { 0.0 };
            m.anim_on = false;
            m.last = None;
        });
        paint();
        return true;
    }
    if with(|m| std::mem::replace(&mut m.anim_on, true)) == Some(false) {
        SetTimer(h, T_ANIM, ANIM_MS, None);
    }
    false
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
    let Some((hwnd, theme, scale, vis, pos, hover, hover_t, query, all, sel, scroll, is_list)) = with(|m| (m.hwnd, m.theme.clone(), m.scale, m.vis, m.pos, m.hover, m.hover_t, m.search.text.clone(), m.all, m.sel, m.scroll, list_mode(m))) else { return };
    unsafe {
        if vis <= 0.0 && !is_open() {
            let _ = ShowWindow(hwnd, SW_HIDE);
            return;
        }
        let l = model::layout(scale);
        let Some(v) = with(|m| view(m)) else { return };
        let (apps, items) = (&v.cat, &v.items);
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
            let (text, color) = if query.is_empty() { (t(K::StartSearch).to_string(), muted) } else if with(|m| m.search.selected) == Some(true) { (query.clone(), rgb(&theme.accent)) } else { (format!("{query}|"), fg) };
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
                for (n, cell) in l.cells.iter().enumerate().take(v.grid_len) {
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
                fill_round_a(buf, w, h, l.divider, 0, fg, 0.10);
                if items.len() == v.grid_len {
                    let hint = R { w: l.divider.w, ..l.places[0] };
                    draw_text(buf, w, h, R { x: hint.x + pad_text, ..hint }, t(K::StartPlacesHint), &theme.font_family, scale, muted, font - 2, DT_LEFT);
                }
                let chip_side = px(20, scale);
                for (k, chip) in l.places.iter().enumerate() {
                    let Some(place) = items.get(v.grid_len + k).and_then(|i| apps.get(*i)) else { break };
                    if on(Hit::Chip(k), v.grid_len + k) {
                        shade(buf, *chip, if sel == Some(v.grid_len + k) { 0.16 } else { 0.10 * ease(hover_t) });
                    }
                    let icon_at = R { x: chip.x + pad_text, y: chip.y + (chip.h - chip_side) / 2, w: chip_side, h: chip_side };
                    match icon_pixels(&place.open, chip_side) {
                        Some(p) => blit(buf, (w, h), icon_at.x, icon_at.y, chip_side, &p),
                        None => draw_label(buf, w, h, icon_at, place_glyph(&place.open), ICONS_FONT, scale, muted, 16),
                    }
                    let text = R { x: icon_at.x + chip_side + px(8, scale), w: chip.x + chip.w - icon_at.x - chip_side - px(14, scale), ..*chip };
                    draw_text(buf, w, h, text, &crate::dock::clip_title(&place.name, 16), &theme.font_family, scale, fg, font - 1, DT_LEFT);
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
        publish(hwnd, &l, &v, is_list, scroll, &query, all);
    }
}

// Glifo de respaldo cuando el shell no entrega ícono (recursos de red, URLs): carpeta, archivo o enlace.
fn place_glyph(open: &str) -> &'static str {
    let leaf = open.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or(open);
    if open.contains("://") || open.to_ascii_lowercase().ends_with(".url") {
        "\u{E774}"
    } else if leaf.contains('.') {
        "\u{E8A5}"
    } else {
        "\u{E8B7}"
    }
}

fn publish(hwnd: HWND, l: &model::Layout, v: &View, is_list: bool, scroll: usize, query: &str, all: bool) {
    let slots: Vec<(R, usize)> = if is_list {
        l.rows.iter().enumerate().map(|(n, r)| (*r, scroll + n)).collect()
    } else {
        l.cells.iter().take(v.grid_len).enumerate().map(|(n, r)| (*r, n)).chain(l.places.iter().enumerate().map(|(k, r)| (*r, v.grid_len + k))).collect()
    };
    let mut nodes: Vec<(String, accesskit::Node)> = slots
        .iter()
        .filter_map(|(r, pos)| {
            let app = v.items.get(*pos).and_then(|i| v.cat.get(*i))?;
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
    if inside(&l.all_button) && with(|m| m.search.text.trim().is_empty()).unwrap_or(false) {
        return Some(Hit::All);
    }
    if list {
        l.rows.iter().position(inside).map(|n| Hit::Row(scroll + n))
    } else {
        l.cells.iter().position(inside).map(Hit::Cell).or_else(|| l.places.iter().position(inside).map(Hit::Chip))
    }
}

// Posición en `View::items` de lo que hay bajo un golpe del mouse; las fichas van después de la grilla.
fn position_of(hit: Hit) -> Option<usize> {
    match hit {
        Hit::Cell(n) | Hit::Row(n) => Some(n),
        Hit::Chip(k) => with(|m| view(m).grid_len + k),
        _ => None,
    }
}

fn open_at(position: usize) -> Option<String> {
    with(|m| {
        let v = view(m);
        v.items.get(position).and_then(|i| v.cat.get(*i)).map(|a| a.open.clone())
    })
    .flatten()
}

fn launch_at(position: usize) {
    if let Some(open) = open_at(position) {
        close();
        crate::dock_menu::launch(&open, "");
    }
}

// Menú contextual de un elemento: quita un pin del Inicio o ancla una app de la lista; usa el mismo guardia `in_menu` que el de energía.
unsafe fn item_menu(position: usize, at: (i32, i32)) {
    let Some(open) = open_at(position) else { return };
    let pinned = PINS_PATH.get().is_some_and(|p| matches!(model::read_pins(p), model::Saved::Ok(v) if v.iter().any(|o| o.eq_ignore_ascii_case(&open))));
    let places_only = !model::is_app(&open);
    if !pinned && places_only {
        return;
    }
    with(|m| m.in_menu = true);
    let (theme, scale) = with(|m| (m.theme.clone(), m.scale)).unwrap_or_default();
    let picked = crate::menu::pick(&[(if pinned { t(K::StartUnpin) } else { t(K::StartPin) }).to_string()], at, crate::menu::Anchor::TopLeft, &theme, scale).map_or(0, |i| i as i32 + 1);
    if let Some(hw) = with(|m| m.hwnd) {
        let _ = SetForegroundWindow(hw);
    }
    with(|m| {
        m.in_menu = false;
        m.opened_at = std::time::Instant::now();
    });
    if picked == 1 {
        edit_pins(|base| if pinned { model::unpin(base, &open) } else { model::pin(base, &open) });
        with(|m| m.sel = None);
        paint();
    }
}

// Esquina superior izquierda en pantalla de lo seleccionado, para abrir el menú desde el teclado.
fn selection_origin() -> Option<(usize, (i32, i32))> {
    with(|m| {
        let pos = m.sel?;
        let l = model::layout(m.scale);
        let grid_len = view(m).grid_len;
        let r = if list_mode(m) { *l.rows.get(pos.checked_sub(m.scroll)?)? } else if pos < grid_len { *l.cells.get(pos)? } else { *l.places.get(pos - grid_len)? };
        Some((pos, (m.pos.0 + r.x + r.w / 2, m.pos.1 + r.y + r.h / 2)))
    })
    .flatten()
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
    with(|m| {
        let v = view(m);
        m.sel = if list_mode(m) { model::step(m.sel, v.items.len(), dx, dy, 1) } else { model::step_sections(m.sel, v.grid_len, v.items.len() - v.grid_len, dx, dy) };
        if let Some(s) = m.sel {
            m.scroll = if list_mode(m) { model::scroll_to(m.scroll, s) } else { 0 };
        }
    });
    paint();
}

fn edit_query(edit: model::Edit) {
    with(|m| {
        m.search.apply(edit);
        m.scroll = 0;
        m.sel = (!m.search.text.trim().is_empty()).then_some(0);
    });
    paint();
}

unsafe fn power_menu() {
    let Some((scale, pos)) = with(|m| {
        m.in_menu = true;
        (m.scale, m.pos)
    }) else {
        return;
    };
    let l = model::layout(scale);
    let labels = [K::PowerLock, K::PowerSleep, K::PowerShutdown, K::PowerRestart, K::PowerSignOut].map(|k| t(k).to_string());
    let theme = with(|m| m.theme.clone()).unwrap_or_default();
    let picked = crate::menu::pick(&labels, (pos.0 + l.power.x + l.power.w, pos.1 + l.power.y), crate::menu::Anchor::BottomRight, &theme, scale).map_or(0, |i| i + 1);
    if let Some(hw) = with(|m| m.hwnd) {
        let _ = SetForegroundWindow(hw);
    }
    with(|m| {
        m.in_menu = false;
        m.opened_at = std::time::Instant::now();
    });
    let args: Option<(&str, &[&str])> = match picked {
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
            with(|s| {
                if list_mode(s) {
                    let max = view(s).items.len().saturating_sub(LIST_ROWS);
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
                Some(Hit::Power) => power_menu(),
                Some(hit @ (Hit::Cell(_) | Hit::Row(_) | Hit::Chip(_))) => position_of(hit).map_or((), launch_at),
                _ => {}
            }
            return LRESULT(0);
        }
        WM_RBUTTONUP => {
            let (x, y) = client(lp);
            if let Some(position) = hit_at(x, y).and_then(position_of) {
                let pos = with(|s| s.pos).unwrap_or_default();
                item_menu(position, (pos.0 + x, pos.1 + y));
            }
            return LRESULT(0);
        }
        WM_SYSKEYDOWN if VIRTUAL_KEY(wp.0 as u16) == VK_F10 && GetKeyState(VK_SHIFT.0 as i32) < 0 => {
            if let Some((position, at)) = selection_origin() {
                item_menu(position, at);
            }
            return LRESULT(0);
        }
        WM_KEYDOWN => {
            if matches!(VIRTUAL_KEY(wp.0 as u16), VK_HOME | VK_END | VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN) {
                with(|s| s.search.apply(model::Edit::Deselect));
            }
            match VIRTUAL_KEY(wp.0 as u16) {
                VK_HOME | VK_END => paint(),
                VK_ESCAPE => {
                    if with(|s| s.search.text.is_empty()).unwrap_or(true) {
                        close();
                    } else {
                        edit_query(model::Edit::Clear);
                    }
                }
                VK_RETURN => {
                    let sel = with(|s| s.sel).flatten();
                    let empty = with(|s| s.search.text.trim().is_empty()).unwrap_or(true);
                    if let Some(n) = sel.or(if empty { None } else { Some(0) }) {
                        launch_at(n);
                    }
                }
                VK_BACK => edit_query(if GetKeyState(VK_CONTROL.0 as i32) < 0 { model::Edit::BackWord } else { model::Edit::Backspace }),
                VK_UP => move_selection(0, -1),
                VK_DOWN => move_selection(0, 1),
                VK_LEFT => move_selection(-1, 0),
                VK_RIGHT => move_selection(1, 0),
                VK_TAB => set_all(!with(|s| s.all).unwrap_or(false)),
                VK_APPS => {
                    if let Some((position, at)) = selection_origin() {
                        item_menu(position, at);
                    }
                }
                _ => {}
            }
            return LRESULT(0);
        }
        WM_CHAR => {
            let query = || with(|s| s.search.text.clone()).unwrap_or_default();
            // con Ctrl, WM_CHAR entrega el código de control de la letra
            match wp.0 {
                0x01 => edit_query(model::Edit::SelectAll),
                0x03 => crate::clipboard::set(h, &query()),
                0x18 => {
                    crate::clipboard::set(h, &query());
                    edit_query(model::Edit::Clear);
                }
                0x16 => {
                    if let Some(text) = crate::clipboard::get(h) {
                        edit_query(model::Edit::Insert(&text));
                    }
                }
                0x1A => edit_query(model::Edit::Undo),
                code => {
                    if let Some(c) = char::from_u32(code as u32).filter(|c| !c.is_control()) {
                        edit_query(model::Edit::Insert(c.encode_utf8(&mut [0; 4])));
                    }
                }
            }
            return LRESULT(0);
        }
        m if m == crate::accessibility::WM_ACTION => {
            for (key, action) in crate::accessibility::actions(h, wp.0) {
                if action != accesskit::Action::Click {
                    continue;
                }
                if key == "power" {
                    power_menu();
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
