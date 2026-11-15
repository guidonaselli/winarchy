// Dock de la shell: una ventana propia con las apps abiertas. La geometría es pura (`layout`) y lo demás es Win32.
use crate::apps::{self, Foreground, WinAttrs};
use crate::komorebi;
use crate::layout::{px, R};
use crate::theme::{rgb, Theme};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::os::windows::process::CommandExt;
use std::sync::Mutex;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::RemoteDesktop::{WTSRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION};
use windows::Win32::System::Threading::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const ICON_DIP: i32 = 24;
const CELL_DIP: i32 = 40;
const GAP_DIP: i32 = 4;
const PAD_DIP: i32 = 8;
pub const BAR_DIP: i32 = 48;
const MARGIN_DIP: i32 = 6;
const CORNER_DIP: i32 = 16;
const DOT_DIP: i32 = 4;
const MAX_ICONS: usize = 64;
// Altura que komorebi reserva: alto del dock + su margen + los 2 px de la franja de la taskbar nativa.
#[allow(dead_code)]
pub const RESERVE_DIP: i32 = BAR_DIP + MARGIN_DIP + 2;

type MonRect = (i32, i32, i32, i32);

#[derive(Debug, PartialEq)]
pub struct Layout {
    pub bar: R,
    pub cells: Vec<R>,
    pub overflow: usize,
}

#[derive(Debug, PartialEq)]
pub enum Hit {
    Item(usize),
    Overflow,
}

// El dock vive en el borde opuesto al de la isla: `bottom` es el borde del dock. `bar` va en coordenadas de pantalla, `cells` relativas a `bar`.
pub fn layout(n: usize, scale: f32, mon: MonRect, bottom: bool) -> Layout {
    let (cell, gap, pad) = (px(CELL_DIP, scale), px(GAP_DIP, scale), px(PAD_DIP, scale));
    let max_w = (mon.2 - mon.0) * 4 / 5;
    let cap = (((max_w - 2 * pad + gap) / (cell + gap)).max(1)) as usize;
    let (shown, overflow) = if n <= cap { (n, 0) } else { (cap - 1, n - (cap - 1)) };
    let slots = shown + usize::from(overflow > 0);
    let w = 2 * pad + slots as i32 * cell + (slots as i32 - 1).max(0) * gap;
    let h = px(BAR_DIP, scale);
    let x = mon.0 + (mon.2 - mon.0 - w) / 2;
    let y = if bottom { mon.3 - px(MARGIN_DIP, scale) - h } else { mon.1 + px(MARGIN_DIP, scale) };
    let cells = (0..slots).map(|i| R { x: pad + i as i32 * (cell + gap), y: (h - cell) / 2, w: cell, h: cell }).collect();
    Layout { bar: R { x, y, w, h }, cells, overflow }
}

pub fn hit_test(l: &Layout, x: i32, y: i32) -> Option<Hit> {
    let (rx, ry) = (x - l.bar.x, y - l.bar.y);
    let i = l.cells.iter().position(|c| rx >= c.x && rx < c.x + c.w && ry >= c.y && ry < c.y + c.h)?;
    Some(if l.overflow > 0 && i == l.cells.len() - 1 { Hit::Overflow } else { Hit::Item(i) })
}

// Ventana a enfocar al hacer clic: con el grupo ya activo, la siguiente; si no, la última activa del grupo.
pub fn pick_target(wins: &[isize], foreground: isize, last_active: Option<isize>) -> Option<isize> {
    if let Some(i) = wins.iter().position(|w| *w == foreground) {
        return (wins.len() > 1).then(|| wins[(i + 1) % wins.len()]).or(Some(wins[i]));
    }
    last_active.filter(|l| wins.contains(l)).or_else(|| wins.first().copied())
}

// Argumentos de `komorebic`: solo índices numéricos, nunca texto de la ventana.
pub fn focus_args(monitor: usize, workspace: usize) -> [String; 3] {
    ["focus-monitor-workspace".into(), monitor.to_string(), workspace.to_string()]
}

// Anclados primero y en su orden; un anclado cuyo exe ya corre se fusiona con ese grupo en vez de duplicarse.
#[derive(Clone)]
struct Pin {
    lnk: String,
    exe: String,
    aumid: Option<String>,
}

fn arrange(pins: &[Pin], mut groups: Vec<Item>) -> Vec<Item> {
    let mut out = Vec::new();
    for Pin { lnk, exe, aumid } in pins {
        let same = |g: &Item| {
            (!exe.is_empty() && g.exe.eq_ignore_ascii_case(exe)) || matches!((&g.aumid, aumid), (Some(a), Some(b)) if a.eq_ignore_ascii_case(b))
        };
        match groups.iter().position(same) {
            Some(i) => {
                let mut g = groups.remove(i);
                g.pin = Some(lnk.clone());
                out.push(g);
            }
            None => out.push(Item { key: format!("pin:{}", lnk.to_lowercase()), exe: exe.clone(), aumid: aumid.clone(), wins: Vec::new(), active: false, other_ws: false, pin: Some(lnk.clone()) }),
        }
    }
    out.extend(groups);
    out
}

const WM_ICON: u32 = WM_APP + 21;
const T_REFRESH: usize = 1;
const T_TICK: usize = 2;
const T_FOCUS: usize = 3;
const REFRESH_MS: u32 = 60;
const FOCUS_DELAY_MS: u32 = 220;

#[derive(Clone, PartialEq)]
struct Item {
    key: String,
    exe: String,
    aumid: Option<String>,
    wins: Vec<isize>,
    active: bool,
    other_ws: bool,
    pin: Option<String>,
}

struct Icon {
    w: i32,
    h: i32,
    px: Vec<u32>,
}

struct Dock {
    hwnd: HWND,
    tip: HWND,
    theme: Theme,
    items: Vec<Item>,
    order: Vec<String>,
    last_active: HashMap<String, isize>,
    icons: HashMap<(String, i32), Icon>,
    asked: HashSet<(String, i32)>,
    layout: Layout,
    hover: Option<usize>,
    tracking: bool,
    scale: f32,
    mon: MonRect,
    sig: u64,
    hidden: bool,
    pending_focus: Option<isize>,
    pins: Vec<Pin>,
    ident: HashMap<isize, (String, Option<String>)>,
    pins_stamp: Option<std::time::SystemTime>,
    shell_msg: u32,
    created_msg: u32,
}

thread_local! { static DOCK: RefCell<Option<Dock>> = const { RefCell::new(None) }; }

fn with<R>(f: impl FnOnce(&mut Dock) -> R) -> R {
    DOCK.with(|d| f(d.borrow_mut().as_mut().expect("dock")))
}

// Íconos listos desde el worker, con píxeles premultiplicados.
static READY: Mutex<Vec<(String, i32, Icon)>> = Mutex::new(Vec::new());
type Ask = (String, i32, Vec<String>);
static ASKS: Mutex<Option<std::sync::mpsc::Sender<Ask>>> = Mutex::new(None);

pub fn init(theme: &Theme) -> Option<HWND> {
    unsafe {
        let inst = GetModuleHandleW(PCWSTR::null()).ok()?;
        let class = WNDCLASSW { lpfnWndProc: Some(dock_proc), hInstance: inst.into(), hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(), lpszClassName: w!("WinarchyShellDock"), ..Default::default() };
        RegisterClassW(&class);
        let ex = WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_LAYERED;
        let hwnd = CreateWindowExW(ex, class.lpszClassName, w!(""), WS_POPUP, 0, 0, 1, 1, None, None, inst, None).ok()?;
        let r = crate::monitor_rects()[0];
        let mon = (r.left, r.top, r.right, r.bottom);
        let scale = crate::mon_scale(&r);
        let tip_class = WNDCLASSW { lpfnWndProc: Some(tip_proc), hInstance: inst.into(), lpszClassName: w!("WinarchyShellDockTip"), ..Default::default() };
        RegisterClassW(&tip_class);
        let tip = CreateWindowExW(ex | WS_EX_TRANSPARENT, tip_class.lpszClassName, w!(""), WS_POPUP, 0, 0, 1, 1, None, None, inst, None).ok()?;
        let d = Dock {
            hwnd,
            tip,
            theme: theme.clone(),
            items: Vec::new(),
            order: Vec::new(),
            last_active: HashMap::new(),
            icons: HashMap::new(),
            asked: HashSet::new(),
            layout: layout(0, scale, mon, theme.position != "bottom"),
            hover: None,
            tracking: false,
            scale,
            mon,
            sig: 0,
            hidden: false,
            pending_focus: None,
            pins: load_pins(),
            ident: HashMap::new(),
            pins_stamp: pins_stamp(),
            shell_msg: RegisterWindowMessageW(w!("SHELLHOOK")),
            created_msg: RegisterWindowMessageW(w!("TaskbarCreated")),
        };
        DOCK.with(|s| *s.borrow_mut() = Some(d));
        spawn_icon_worker(hwnd.0 as isize);
        komorebi::NOTIFY.store(hwnd.0 as isize, std::sync::atomic::Ordering::Relaxed);
        register(hwnd);
        refresh();
        SetTimer(hwnd, T_TICK, 1000, None);
        Some(hwnd)
    }
}

pub fn set_theme(theme: &Theme) {
    if DOCK.with(|d| d.borrow().is_none()) {
        return;
    }
    with(|d| {
        d.theme = theme.clone();
        d.sig = 0;
    });
    unsafe { refresh() }
}

// `.lnk` de la taskbar nativa. shortcut: sin el orden de `Taskband` (va alfabético) y sin anclados de Store (el `.lnk` no trae exe), cuando se necesite ese orden leer `Taskband\Favorites`.
fn pins_dir() -> Option<std::path::PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(std::path::Path::new(&appdata).join(r"Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar"))
}

fn pins_stamp() -> Option<std::time::SystemTime> {
    std::fs::metadata(pins_dir()?).and_then(|m| m.modified()).ok()
}

// `Taskband\Favorites` guarda los nombres de los `.lnk` anclados en UTF-16, en el orden de la taskbar.
// shortcut: se buscan cadenas `*.lnk` en el blob sin parsear su formato (puede colar un byte basura delante del nombre, por eso el match es por sufijo); si Windows lo cambia, el orden vuelve a alfabético.
fn lnk_order(blob: &[u8]) -> Vec<String> {
    let mut found: Vec<(usize, String)> = Vec::new();
    // las cadenas no están alineadas a 2 bytes: se recorren las dos paridades.
    for skew in 0..2 {
        let mut cur = String::new();
        let mut start = 0;
        let units = blob.get(skew..).unwrap_or(&[]).chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]));
        for (i, u) in units.chain(std::iter::once(0)).enumerate() {
            match char::from_u32(u as u32).filter(|c| !c.is_control()) {
                Some(c) => {
                    if cur.is_empty() {
                        start = skew + i * 2;
                    }
                    cur.push(c)
                }
                None => {
                    if cur.to_lowercase().ends_with(".lnk") {
                        found.push((start, cur.rsplit(['\\', '/']).next().unwrap_or("").to_lowercase()));
                    }
                    cur.clear();
                }
            }
        }
    }
    found.sort();
    found.into_iter().map(|(_, n)| n).collect()
}

fn taskband_blob() -> Vec<u8> {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_BINARY};
    let mut buf = vec![0u8; 16384];
    let mut len = buf.len() as u32;
    let rc = unsafe { RegGetValueW(HKEY_CURRENT_USER, w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Taskband"), w!("Favorites"), RRF_RT_REG_BINARY, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut len)) };
    if rc.is_err() {
        return Vec::new();
    }
    buf.truncate(len as usize);
    buf
}

fn sort_pins(files: &mut [std::path::PathBuf], order: &[String]) {
    let rank = |p: &std::path::PathBuf| p.file_name().and_then(|n| order.iter().position(|o| o.ends_with(&n.to_string_lossy().to_lowercase()))).unwrap_or(usize::MAX);
    files.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.cmp(b)));
}

fn load_pins() -> Vec<Pin> {
    use windows::core::Interface;
    use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ};
    let Some(dir) = pins_dir() else { return Vec::new() };
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk"))).collect();
    sort_pins(&mut files, &lnk_order(&taskband_blob()));
    let mut pins = Vec::new();
    for f in files {
        let wide: Vec<u16> = f.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
        let (exe, aumid) = unsafe {
            let Ok(link) = CoCreateInstance::<_, IShellLinkW>(&ShellLink, None, CLSCTX_INPROC_SERVER) else { continue };
            let Ok(file) = link.cast::<IPersistFile>() else { continue };
            if file.Load(PCWSTR(wide.as_ptr()), STGM_READ).is_err() {
                continue;
            }
            let mut buf = [0u16; 520];
            let _ = link.GetPath(&mut buf, std::ptr::null_mut(), 0);
            let n = buf.iter().position(|c| *c == 0).unwrap_or(0);
            let aumid = link.cast::<windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore>().ok().and_then(|st| variant_string(&st));
            (String::from_utf16_lossy(&buf[..n]), aumid)
        };
        // File Explorer y los de Store no traen exe: se identifican por AUMID.
        if !exe.is_empty() || aumid.is_some() {
            pins.push(Pin { lnk: f.to_string_lossy().into_owned(), exe, aumid });
        }
    }
    pins
}

unsafe fn register(hwnd: HWND) {
    let _ = RegisterShellHookWindow(hwnd);
    let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
}

unsafe fn queue_refresh(hwnd: HWND) {
    SetTimer(hwnd, T_REFRESH, REFRESH_MS, None);
}

unsafe fn is_cloaked(h: HWND) -> bool {
    let mut c = 0u32;
    DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut c as *mut _ as *mut _, 4).is_ok() && c != 0
}

unsafe fn exe_of(h: HWND) -> String {
    let mut pid = 0u32;
    GetWindowThreadProcessId(h, Some(&mut pid));
    let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return String::new() };
    let mut buf = [0u16; 520];
    let mut n = buf.len() as u32;
    let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
    let _ = CloseHandle(p);
    if ok { String::from_utf16_lossy(&buf[..n as usize]) } else { String::new() }
}

unsafe fn aumid_of(h: HWND) -> Option<String> {
    use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
    let store: IPropertyStore = SHGetPropertyStoreForWindow(h).ok()?;
    variant_string(&store)
}

fn variant_string(store: &windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore) -> Option<String> {
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
    unsafe {
        let v = store.GetValue(&PKEY_AppUserModel_ID).ok()?;
        let s = PropVariantToStringAlloc(&v).ok()?;
        let r = s.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(s.0 as *const _));
        r.filter(|a| !a.is_empty())
    }
}

unsafe fn is_eligible(h: HWND, own_pid: u32, shell: HWND) -> bool {
    if h == shell || GetWindowTextLengthW(h) == 0 {
        return false;
    }
    let ex = GetWindowLongW(h, GWL_EXSTYLE) as u32;
    let mut pid = 0u32;
    GetWindowThreadProcessId(h, Some(&mut pid));
    apps::eligible(&WinAttrs {
        visible: IsWindowVisible(h).as_bool(),
        owned: GetWindow(h, GW_OWNER).is_ok_and(|o| !o.0.is_null()),
        appwindow: ex & WS_EX_APPWINDOW.0 != 0,
        toolwindow: ex & WS_EX_TOOLWINDOW.0 != 0,
        cloaked: is_cloaked(h),
        managed: komorebi::window_slot(h.0 as isize).is_some(),
        own_process: pid == own_pid,
    })
}

unsafe extern "system" fn collect(h: HWND, data: LPARAM) -> BOOL {
    (*(data.0 as *mut Vec<HWND>)).push(h);
    BOOL(1)
}

// Re-lee las ventanas y redibuja solo si lo visible cambió.
unsafe fn refresh() {
    let mut all: Vec<HWND> = Vec::new();
    let _ = EnumWindows(Some(collect), LPARAM(&mut all as *mut _ as isize));
    let (own, shell) = (GetCurrentProcessId(), GetShellWindow());
    let fg = GetForegroundWindow().0 as isize;
    let mut groups: Vec<Item> = Vec::new();
    let eligible: Vec<HWND> = all.into_iter().filter(|h| is_eligible(*h, own, shell)).collect();
    // exe y AUMID no cambian en la vida de la ventana: se piden una vez y no en cada refresco.
    let mut ident = with(|d| std::mem::take(&mut d.ident));
    ident.retain(|k, _| eligible.iter().any(|h| h.0 as isize == *k));
    for h in eligible {
        let (exe, aumid) = ident.entry(h.0 as isize).or_insert_with(|| (exe_of(h), aumid_of(h))).clone();
        let key = apps::group_key(aumid.as_deref(), &exe);
        let id = h.0 as isize;
        let cloaked = is_cloaked(h);
        match groups.iter_mut().find(|g| g.key == key) {
            Some(g) => {
                g.wins.push(id);
                g.active |= id == fg;
                g.other_ws &= cloaked;
            }
            None => groups.push(Item { key, exe, aumid, wins: vec![id], active: id == fg, other_ws: cloaked, pin: None }),
        }
    }
    with(|d| {
        d.ident = ident;
        d.order.retain(|k| groups.iter().any(|g| &g.key == k));
        for g in &groups {
            if !d.order.contains(&g.key) {
                d.order.push(g.key.clone());
            }
            if g.wins.contains(&fg) {
                d.last_active.insert(g.key.clone(), fg);
            }
        }
        let order = d.order.clone();
        groups.sort_by_key(|g| order.iter().position(|k| k == &g.key));
        d.items = arrange(&d.pins, groups);
    });
    render();
}

fn signature(d: &Dock) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for i in &d.items {
        (&i.key, i.wins.len(), i.active, i.other_ws).hash(&mut h);
    }
    (d.hover, d.icons.len(), d.scale.to_bits(), d.mon, d.hidden).hash(&mut h);
    (&d.theme.background, &d.theme.accent, &d.theme.position).hash(&mut h);
    h.finish()
}

fn blend(dst: &mut u32, src: u32) {
    let sa = src >> 24;
    if sa == 255 {
        *dst = src;
        return;
    }
    let inv = 255 - sa;
    let ch = |s: u32, d: u32| (s + d * inv / 255).min(255);
    let d = *dst;
    *dst = ch(sa, d >> 24) << 24 | ch((src >> 16) & 255, (d >> 16) & 255) << 16 | ch((src >> 8) & 255, (d >> 8) & 255) << 8 | ch(src & 255, d & 255);
}

fn premul(col: (u8, u8, u8), alpha: f32) -> u32 {
    let p = |c: u8| (c as f32 * alpha) as u32;
    ((alpha * 255.0) as u32) << 24 | p(col.0) << 16 | p(col.1) << 8 | p(col.2)
}

fn fill_round_a(buf: &mut [u32], stride: i32, rows: i32, r: R, rad: i32, col: (u8, u8, u8), alpha: f32) {
    let rad = (rad as f32).min(r.w as f32 / 2.0).min(r.h as f32 / 2.0);
    for y in r.y.max(0)..(r.y + r.h).min(rows) {
        for x in r.x.max(0)..(r.x + r.w).min(stride) {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let cx = fx.clamp(r.x as f32 + rad, (r.x + r.w) as f32 - rad);
            let cy = fy.clamp(r.y as f32 + rad, (r.y + r.h) as f32 - rad);
            let cov = (rad - ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt() + 0.5).clamp(0.0, 1.0);
            if cov > 0.0 {
                blend(&mut buf[(y * stride + x) as usize], premul(col, alpha * cov));
            }
        }
    }
}

fn draw_icon(buf: &mut [u32], stride: i32, rows: i32, ic: &Icon, x0: i32, y0: i32, dim: f32) {
    for y in 0..ic.h {
        for x in 0..ic.w {
            let (dx, dy) = (x0 + x, y0 + y);
            if dx < 0 || dy < 0 || dx >= stride || dy >= rows {
                continue;
            }
            let s = ic.px[(y * ic.w + x) as usize];
            let m = |sh: u32| ((((s >> sh) & 255) as f32) * dim) as u32;
            blend(&mut buf[(dy * stride + dx) as usize], m(24) << 24 | m(16) << 16 | m(8) << 8 | m(0));
        }
    }
}

fn icon_sources(i: &Item) -> Vec<String> {
    let mut v = Vec::new();
    if let Some(p) = &i.pin {
        v.push(p.clone());
    }
    if let Some(a) = &i.aumid {
        v.push(format!(r"shell:AppsFolder\{a}"));
    }
    if !i.exe.is_empty() {
        v.push(i.exe.clone());
    }
    v
}

fn render() {
    let (hwnd, theme, items, layout_, hover, scale, hidden) = with(|d| {
        d.layout = layout(d.items.len(), d.scale, d.mon, d.theme.position != "bottom");
        (d.hwnd, d.theme.clone(), d.items.clone(), Layout { bar: d.layout.bar, cells: d.layout.cells.clone(), overflow: d.layout.overflow }, d.hover, d.scale, d.hidden)
    });
    let size = px(ICON_DIP, scale);
    let to_ask: Vec<Ask> = with(|d| {
        let missing: Vec<&Item> = items.iter().filter(|i| !d.icons.contains_key(&(i.key.clone(), size))).collect();
        missing.into_iter().filter(|i| d.asked.insert((i.key.clone(), size))).map(|i| (i.key.clone(), size, icon_sources(i))).collect()
    });
    if let Some(tx) = ASKS.lock().unwrap().as_ref() {
        for a in to_ask {
            let _ = tx.send(a);
        }
    }
    let sig = with(|d| signature(d));
    if with(|d| d.sig == sig) {
        return;
    }
    with(|d| d.sig = sig);
    unsafe {
        if hidden || items.is_empty() {
            let _ = ShowWindow(with(|d| d.tip), SW_HIDE);
            let _ = ShowWindow(hwnd, SW_HIDE);
            return;
        }
        let (w, h) = (layout_.bar.w, layout_.bar.h);
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
        let buf = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
        buf.fill(0);
        let (bg, fg, accent) = (rgb(&theme.background), rgb(&theme.foreground), rgb(&theme.accent));
        fill_round_a(buf, w, h, R { x: 0, y: 0, w, h }, px(CORNER_DIP, scale), bg, theme.opacity.clamp(0.4, 1.0));
        DOCK.with(|d| {
            let d = d.borrow();
            let icons = &d.as_ref().expect("dock").icons;
            for (idx, cell) in layout_.cells.iter().enumerate() {
                if layout_.overflow > 0 && idx == layout_.cells.len() - 1 {
                    fill_round_a(buf, w, h, *cell, px(10, scale), fg, 0.12);
                    continue;
                }
                let Some(item) = items.get(idx) else { continue };
                if item.active {
                    fill_round_a(buf, w, h, *cell, px(10, scale), accent, 0.28);
                } else if hover == Some(idx) {
                    fill_round_a(buf, w, h, *cell, px(10, scale), fg, 0.10);
                }
                let dim = if item.other_ws { 0.5 } else { 1.0 };
                let lift = px(2, scale);
                match icons.get(&(item.key.clone(), size)) {
                    Some(ic) => draw_icon(buf, w, h, ic, cell.x + (cell.w - ic.w) / 2, cell.y + (cell.h - ic.h) / 2 - lift, dim),
                    None => fill_round_a(buf, w, h, R { x: cell.x + (cell.w - size) / 2, y: cell.y + (cell.h - size) / 2 - lift, w: size, h: size }, px(6, scale), fg, 0.25),
                }
                if item.wins.is_empty() {
                    continue;
                }
                let dot = px(DOT_DIP, scale);
                let dw = if item.active { dot * 3 } else { dot };
                fill_round_a(buf, w, h, R { x: cell.x + (cell.w - dw) / 2, y: cell.y + cell.h - dot - px(3, scale), w: dw, h: dot }, dot / 2, if item.active { accent } else { fg }, if item.active { 1.0 } else { 0.6 });
            }
        });
        let pos = POINT { x: layout_.bar.x, y: layout_.bar.y };
        let sz = SIZE { cx: w, cy: h };
        let src = POINT::default();
        let bf = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: 1 };
        let _ = UpdateLayeredWindow(hwnd, None, Some(&pos), Some(&sz), mem, Some(&src), COLORREF(0), Some(&bf), ULW_ALPHA);
        SelectObject(mem, old);
        let _ = DeleteObject(dib);
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
}

fn drain_ready() {
    let ready: Vec<_> = std::mem::take(&mut *READY.lock().unwrap());
    if ready.is_empty() {
        return;
    }
    with(|d| {
        for (k, size, ic) in ready {
            d.icons.insert((k, size), ic);
        }
        if d.icons.len() > MAX_ICONS {
            let live: HashSet<String> = d.items.iter().map(|i| i.key.clone()).collect();
            d.icons.retain(|(k, _), _| live.contains(k));
            let kept: HashSet<(String, i32)> = d.icons.keys().cloned().collect();
            d.asked.retain(|a| kept.contains(a));
        }
        d.sig = 0;
    });
    render();
}

fn spawn_icon_worker(target: isize) {
    let (tx, rx) = std::sync::mpsc::channel::<Ask>();
    *ASKS.lock().unwrap() = Some(tx);
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        while let Ok((key, size, sources)) = rx.recv() {
            // shortcut: reintenta 3 veces con 400 ms en el mismo hilo (bloquea la cola un máximo de ~0,8 s); un ícono que sigue fallando queda en placeholder hasta recargar el dock.
            let found = (0..3).find_map(|n| {
                if n > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(400));
                }
                sources.iter().find_map(|s| fetch_icon(s, size))
            });
            if let Some(ic) = found {
                READY.lock().unwrap().push((key, size, ic));
                let _ = PostMessageW(HWND(target as *mut _), WM_ICON, WPARAM(0), LPARAM(0));
            }
        }
        CoUninitialize();
    });
}

unsafe fn fetch_icon(source: &str, size: i32) -> Option<Icon> {
    let wide: Vec<u16> = source.encode_utf16().chain(std::iter::once(0)).collect();
    let item: IShellItemImageFactory = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None).ok()?;
    let bmp = item.GetImage(SIZE { cx: size, cy: size }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK).ok()?;
    let mut bm = BITMAP::default();
    if GetObjectW(bmp, std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _)) == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 {
        let _ = DeleteObject(bmp);
        return None;
    }
    let (w, h) = (bm.bmWidth, bm.bmHeight);
    let mut buf = vec![0u32; (w * h) as usize];
    let mut bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: 0, ..Default::default() }, ..Default::default() };
    let dc = GetDC(None);
    let got = GetDIBits(dc, bmp, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut bmi, DIB_RGB_COLORS);
    ReleaseDC(None, dc);
    let _ = DeleteObject(bmp);
    (got != 0).then_some(Icon { w, h, px: buf })
}

unsafe fn dock_foreground(mon: MonRect) -> Option<Foreground> {
    let fg = GetForegroundWindow();
    if fg.0.is_null() {
        return None;
    }
    let mut rc = RECT::default();
    let _ = GetWindowRect(fg, &mut rc);
    let style = GetWindowLongW(fg, GWL_STYLE) as u32;
    let mut cls = [0u16; 64];
    let n = GetClassNameW(fg, &mut cls).max(0) as usize;
    let class = String::from_utf16_lossy(&cls[..n]);
    let m = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    let _ = GetMonitorInfoW(m, &mut mi);
    Some(Foreground {
        rect: (rc.left, rc.top, rc.right, rc.bottom),
        caption: style & WS_CAPTION.0 == WS_CAPTION.0,
        thickframe: style & WS_THICKFRAME.0 != 0,
        zoomed: IsZoomed(fg).as_bool(),
        shell_like: fg == GetShellWindow() || matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd"),
        on_dock_monitor: (mi.rcMonitor.left, mi.rcMonitor.top) == (mon.0, mon.1),
    })
}

// La taskbar nativa en auto-hide, al aparecer, queda sobre el dock: mientras se vea (o su overflow) no se reafirma el orden.
unsafe fn native_taskbar_visible(mon: MonRect) -> bool {
    if let Ok(t) = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) {
        let mut rc = RECT::default();
        if GetWindowRect(t, &mut rc).is_ok() && IsWindowVisible(t).as_bool() && rc.top < mon.3 - 4 && rc.bottom > mon.3 - 4 {
            return true;
        }
    }
    FindWindowW(w!("TopLevelWindowForOverflowXamlIsland"), PCWSTR::null()).is_ok_and(|o| IsWindowVisible(o).as_bool())
}

unsafe fn tick() {
    let (hwnd, mon, hidden) = with(|d| (d.hwnd, d.mon, d.hidden));
    let busy = matches!(SHQueryUserNotificationState(), Ok(s) if s == QUNS_BUSY || s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_PRESENTATION_MODE);
    let hide = apps::should_hide(dock_foreground(mon).as_ref(), mon, busy);
    let r0 = crate::monitor_rects()[0];
    let m0 = (r0.left, r0.top, r0.right, r0.bottom);
    let scale = crate::mon_scale(&r0);
    let changed = with(|d| {
        let ch = d.mon != m0 || (d.scale - scale).abs() > 0.01;
        if ch {
            d.mon = m0;
            d.scale = scale;
            d.icons.clear();
            d.asked.clear();
        }
        d.hidden = hide;
        ch
    });
    let stamp = pins_stamp();
    let pins_changed = with(|d| d.pins_stamp != stamp);
    if pins_changed {
        let pins = load_pins();
        with(|d| {
            d.pins = pins;
            d.pins_stamp = stamp;
        });
        refresh();
    }
    if hide != hidden || changed {
        render();
    }
    if !hide && !native_taskbar_visible(m0) {
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

// Windows niega SetForegroundWindow a quien no es el foreground: se comparte la cola de entrada con él un instante.
unsafe fn raise(hwnd: HWND) {
    if SetForegroundWindow(hwnd).as_bool() {
        return;
    }
    let fg_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
    let me = GetCurrentThreadId();
    let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, true).as_bool();
    let _ = BringWindowToTop(hwnd);
    let _ = SetForegroundWindow(hwnd);
    if attached {
        let _ = AttachThreadInput(me, fg_thread, false);
    }
}

unsafe fn focus_window(hwnd: HWND) {
    let dock = with(|d| d.hwnd);
    if !IsWindow(hwnd).as_bool() {
        queue_refresh(dock);
        return;
    }
    let id = hwnd.0 as isize;
    if IsIconic(hwnd).as_bool() {
        let _ = ShowWindow(hwnd, SW_RESTORE);
    }
    match komorebi::window_slot(id) {
        Some((m, ws)) if is_cloaked(hwnd) => {
            let _ = std::process::Command::new("komorebic.exe").args(focus_args(m, ws)).creation_flags(0x0800_0000).spawn();
            with(|d| d.pending_focus = Some(id));
            SetTimer(dock, T_FOCUS, FOCUS_DELAY_MS, None);
        }
        _ => raise(hwnd),
    }
}

unsafe fn click(idx: usize) {
    let target = with(|d| d.items.get(idx).map(|i| (i.wins.clone(), d.last_active.get(&i.key).copied(), i.pin.clone())));
    let Some((wins, last, pin)) = target else { return };
    if wins.is_empty() {
        if let Some(p) = pin {
            let wide: Vec<u16> = p.encode_utf16().chain(std::iter::once(0)).collect();
            let _ = ShellExecuteW(None, w!("open"), PCWSTR(wide.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
        }
        return;
    }
    let fg = GetForegroundWindow().0 as isize;
    if let Some(t) = pick_target(&wins, fg, last) {
        focus_window(HWND(t as *mut _));
    }
}

fn xy(lp: LPARAM) -> (i32, i32) {
    ((lp.0 & 0xffff) as i16 as i32, ((lp.0 >> 16) & 0xffff) as i16 as i32)
}

unsafe extern "system" fn tip_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if m == WM_MOUSEACTIVATE {
        return LRESULT(MA_NOACTIVATE as isize);
    }
    DefWindowProcW(h, m, wp, lp)
}

pub fn clip_title(t: &str, max: usize) -> String {
    if t.chars().count() <= max {
        return t.to_string();
    }
    let mut out: String = t.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

// Texto del tooltip: el título de la ventana a enfocar, o el nombre del anclado si no hay ventanas.
unsafe fn tip_text(idx: usize) -> Option<String> {
    let (wins, last, pin) = with(|d| d.items.get(idx).map(|i| (i.wins.clone(), d.last_active.get(&i.key).copied(), i.pin.clone())))?;
    let target = pick_target(&wins, 0, last);
    if let Some(t) = target {
        let h = HWND(t as *mut _);
        let mut buf = [0u16; 256];
        let n = GetWindowTextW(h, &mut buf).max(0) as usize;
        let title = String::from_utf16_lossy(&buf[..n]);
        if !title.is_empty() {
            return Some(clip_title(&title, 48));
        }
    }
    pin.and_then(|p| std::path::Path::new(&p).file_stem().map(|s| s.to_string_lossy().into_owned()))
}

unsafe fn hide_tip() {
    let tip = with(|d| d.tip);
    let _ = ShowWindow(tip, SW_HIDE);
}

// Tooltip propio (ventana layered chica): el texto se dibuja con GDI sobre negro y su luminancia pasa a alfa.
unsafe fn show_tip(idx: usize) {
    let Some(text) = tip_text(idx) else { return hide_tip() };
    let (tip, theme, scale, bar, cell, bottom) = with(|d| (d.tip, d.theme.clone(), d.scale, d.layout.bar, d.layout.cells.get(idx).copied(), d.theme.position != "bottom"));
    let Some(cell) = cell else { return hide_tip() };
    let wide: Vec<u16> = text.encode_utf16().collect();
    let family: Vec<u16> = theme.font_family.encode_utf16().chain(std::iter::once(0)).collect();
    let screen = GetDC(None);
    let mem = CreateCompatibleDC(screen);
    let font = CreateFontW(-px((theme.font_size - 1).max(10), scale), 0, 0, 0, 500, 0, 0, 0, 1, 0, 0, ANTIALIASED_QUALITY.0 as u32, 0, PCWSTR(family.as_ptr()));
    let old_font = SelectObject(mem, font);
    let mut ext = SIZE::default();
    let _ = GetTextExtentPoint32W(mem, &wide, &mut ext);
    let pad = px(8, scale);
    let (w, h) = (ext.cx + 2 * pad, ext.cy + pad);
    let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: 0, ..Default::default() }, ..Default::default() };
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let Ok(dib) = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else {
        SelectObject(mem, old_font);
        let _ = DeleteObject(font);
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        return;
    };
    let old = SelectObject(mem, dib);
    let buf = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
    buf.fill(0);
    SetBkMode(mem, TRANSPARENT);
    SetTextColor(mem, COLORREF(0x00FF_FFFF));
    let mut rc = RECT { left: pad, top: pad / 2, right: w - pad, bottom: h };
    let mut tmp = wide.clone();
    DrawTextW(mem, &mut tmp, &mut rc, DT_LEFT | DT_NOPREFIX | DT_SINGLELINE);
    let (bg, fg) = (rgb(&theme.background), rgb(&theme.foreground));
    let coverage: Vec<u32> = buf.iter().map(|p| ((*p >> 16) & 255).max((*p >> 8) & 255).max(*p & 255)).collect();
    buf.fill(0);
    fill_round_a(buf, w, h, R { x: 0, y: 0, w, h }, px(8, scale), bg, 0.96);
    for (i, c) in coverage.iter().enumerate() {
        if *c > 0 {
            blend(&mut buf[i], premul(fg, *c as f32 / 255.0));
        }
    }
    let mon = with(|d| d.mon);
    let cx = bar.x + cell.x + cell.w / 2;
    let x = (cx - w / 2).clamp(mon.0, (mon.2 - w).max(mon.0));
    let gap = px(8, scale);
    let y = if bottom { bar.y - h - gap } else { bar.y + bar.h + gap };
    let (pos, sz, src) = (POINT { x, y }, SIZE { cx: w, cy: h }, POINT::default());
    let bf = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: 1 };
    let _ = UpdateLayeredWindow(tip, None, Some(&pos), Some(&sz), mem, Some(&src), COLORREF(0), Some(&bf), ULW_ALPHA);
    SelectObject(mem, old);
    SelectObject(mem, old_font);
    let _ = DeleteObject(dib);
    let _ = DeleteObject(font);
    let _ = DeleteDC(mem);
    ReleaseDC(None, screen);
    let _ = ShowWindow(tip, SW_SHOWNOACTIVATE);
}

unsafe extern "system" fn dock_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let (shell_msg, created_msg) = DOCK.with(|d| d.try_borrow().ok().and_then(|d| d.as_ref().map(|d| (d.shell_msg, d.created_msg))).unwrap_or((0, 0)));
    if m != 0 && m == shell_msg {
        queue_refresh(h);
        return LRESULT(0);
    }
    if m != 0 && m == created_msg {
        register(h);
        queue_refresh(h);
        return LRESULT(0);
    }
    match m {
        WM_TIMER if wp.0 == T_REFRESH => {
            let _ = KillTimer(h, T_REFRESH);
            refresh();
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_TICK => {
            tick();
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_FOCUS => {
            let _ = KillTimer(h, T_FOCUS);
            if let Some(id) = with(|d| d.pending_focus.take()) {
                let t = HWND(id as *mut _);
                if IsWindow(t).as_bool() {
                    raise(t);
                }
            }
            LRESULT(0)
        }
        WM_ICON => {
            drain_ready();
            LRESULT(0)
        }
        x if x == komorebi::WM_WINDOW_MAP => {
            queue_refresh(h);
            LRESULT(0)
        }
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            with(|d| d.sig = 0);
            tick();
            LRESULT(0)
        }
        // WM_WTSSESSION_CHANGE o suspensión: se re-siembra y se reafirma el orden.
        0x02B1 | WM_POWERBROADCAST => {
            with(|d| d.sig = 0);
            queue_refresh(h);
            let _ = SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            LRESULT(1)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_MOUSEMOVE => {
            let (x, y) = xy(lp);
            let (changed, need_track) = with(|d| {
                let idx = match hit_test(&d.layout, x + d.layout.bar.x, y + d.layout.bar.y) {
                    Some(Hit::Item(i)) => Some(i),
                    _ => None,
                };
                let changed = idx != d.hover;
                d.hover = idx;
                let need = !d.tracking;
                d.tracking = true;
                (changed, need)
            });
            if need_track {
                let _ = TrackMouseEvent(&mut TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: h, dwHoverTime: 0 });
            }
            if changed {
                render();
                match with(|d| d.hover) {
                    Some(i) => show_tip(i),
                    None => hide_tip(),
                }
            }
            LRESULT(0)
        }
        0x02A3 => {
            with(|d| {
                d.hover = None;
                d.tracking = false;
            });
            render();
            hide_tip();
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = xy(lp);
            match with(|d| hit_test(&d.layout, x + d.layout.bar.x, y + d.layout.bar.y).map(|h| (h, d.layout.cells.len()))) {
                Some((Hit::Item(i), _)) => click(i),
                // shortcut: "+N" salta al primer grupo oculto, sin menú con la lista completa; hacerlo cuando haya menú propio.
                Some((Hit::Overflow, cells)) => click(cells - 1),
                None => {}
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MON: MonRect = (0, 0, 1920, 1080);

    #[test]
    fn dock_is_centered_on_the_requested_edge() {
        let b = layout(3, 1.0, MON, true);
        assert_eq!(b.bar.y + b.bar.h, 1080 - MARGIN_DIP);
        assert_eq!(b.bar.x * 2 + b.bar.w, 1920);
        assert_eq!(layout(3, 1.0, MON, false).bar.y, MARGIN_DIP);
        assert_eq!(b.cells.len(), 3);
    }

    #[test]
    fn many_windows_collapse_into_a_plus_n_cell() {
        let l = layout(200, 1.0, MON, true);
        assert!(l.overflow > 1 && l.bar.w <= 1920 * 4 / 5);
        assert_eq!(l.cells.len() + l.overflow - 1, 200);
        assert_eq!(hit_test(&l, l.bar.x + l.cells.last().unwrap().x + 1, l.bar.y + l.cells[0].y + 1), Some(Hit::Overflow));
    }

    #[test]
    fn hit_test_finds_the_cell_and_ignores_the_gaps() {
        let l = layout(3, 1.0, MON, true);
        let c = l.cells[1];
        assert_eq!(hit_test(&l, l.bar.x + c.x + 5, l.bar.y + c.y + 5), Some(Hit::Item(1)));
        assert_eq!(hit_test(&l, l.bar.x + c.x - 1, l.bar.y + c.y + 5), None);
        assert_eq!(hit_test(&l, 0, 0), None);
    }

    #[test]
    fn zero_items_do_not_panic() {
        assert!(layout(0, 1.5, MON, true).cells.is_empty());
    }

    #[test]
    fn click_cycles_a_group_that_is_already_active() {
        assert_eq!(pick_target(&[1, 2, 3], 2, None), Some(3));
        assert_eq!(pick_target(&[1, 2, 3], 3, None), Some(1));
        assert_eq!(pick_target(&[7], 7, None), Some(7));
    }

    #[test]
    fn click_prefers_the_last_active_window_and_ignores_stale_ones() {
        assert_eq!(pick_target(&[1, 2], 99, Some(2)), Some(2));
        assert_eq!(pick_target(&[1, 2], 99, Some(5)), Some(1));
        assert_eq!(pick_target(&[], 99, None), None);
    }

    fn item(key: &str, exe: &str) -> Item {
        Item { key: key.into(), exe: exe.into(), aumid: None, wins: vec![1], active: false, other_ws: false, pin: None }
    }

    #[test]
    fn a_running_pin_merges_with_its_group_and_leads_the_list() {
        let pins = vec![Pin { lnk: "a.lnk".into(), exe: r"C:\X\Tool.exe".into(), aumid: None }];
        let out = arrange(&pins, vec![item("exe:other", r"C:\o.exe"), item("exe:tool", r"c:\x\tool.EXE")]);
        assert_eq!((out[0].key.as_str(), out[0].pin.as_deref(), out.len()), ("exe:tool", Some("a.lnk"), 2));
    }

    #[test]
    fn a_pin_without_exe_merges_by_aumid() {
        let pins = vec![Pin { lnk: "e.lnk".into(), exe: String::new(), aumid: Some("Microsoft.Windows.Explorer".into()) }];
        let mut g = item("aumid:microsoft.windows.explorer", r"C:\Windows\explorer.exe");
        g.aumid = Some("microsoft.windows.explorer".into());
        let out = arrange(&pins, vec![item("exe:o", "o.exe"), g]);
        assert_eq!((out.len(), out[0].pin.as_deref(), out[0].wins.len()), (2, Some("e.lnk"), 1));
    }

    #[test]
    fn a_pin_without_windows_stays_as_a_launcher() {
        let pins = vec![Pin { lnk: "b.lnk".into(), exe: r"C:\gone.exe".into(), aumid: None }];
        let out = arrange(&pins, vec![item("exe:o", "o.exe")]);
        assert!(out[0].wins.is_empty() && out[0].pin.as_deref() == Some("b.lnk") && out[1].key == "exe:o");
    }

    #[test]
    fn pins_follow_the_taskband_order_and_unknowns_go_last_alphabetically() {
        let utf16 = |t: &str| t.encode_utf16().flat_map(|u| u.to_le_bytes()).collect::<Vec<u8>>();
        let mut blob = vec![1u8, 0, 0xFF];
        for n in ["Helium.lnk", "File Explorer.lnk"] {
            blob.extend(utf16(n));
            blob.extend([0, 0, 7, 0]);
        }
        let order = lnk_order(&blob);
        assert_eq!(order.len(), 2);
        assert!(order[0].ends_with("helium.lnk") && order[1].ends_with("file explorer.lnk"));
        let mut files: Vec<std::path::PathBuf> = ["Zed.lnk", "File Explorer.lnk", "Abc.lnk", "Helium.lnk"].iter().map(Into::into).collect();
        sort_pins(&mut files, &order);
        let names: Vec<_> = files.iter().map(|f| f.to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["Helium.lnk", "File Explorer.lnk", "Abc.lnk", "Zed.lnk"]);
    }

    #[test]
    fn long_titles_are_clipped_with_an_ellipsis() {
        assert_eq!(clip_title("abc", 5), "abc");
        assert_eq!(clip_title("abcdefgh", 5), "abcd…");
    }

    #[test]
    fn komorebic_arguments_are_numeric_only() {
        assert_eq!(focus_args(1, 3), ["focus-monitor-workspace", "1", "3"]);
    }
}
