// Menú contextual del dock: una ventana propia y activa (recibe el teclado y se descarta al perder el foco),
// dibujada con el theme. La lógica de qué entradas hay y dónde cae cada fila es pura.
use crate::dock::{blend, fill_round_a, pick_target, premul};
use crate::i18n::{t, K};
use crate::layout::{px, R};
use crate::theme::{contrasting_color as close_glyph_color, rgb, Theme};
use std::cell::RefCell;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_LEFT, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SPACE, VK_UP};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_LOCAL_SERVER};
use windows::Win32::UI::Shell::{ApplicationActivationManager, IApplicationActivationManager, ShellExecuteW, AO_NONE};
use windows::Win32::UI::WindowsAndMessaging::*;

type MonRect = (i32, i32, i32, i32);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    Focus(isize),
    Group(usize),
    Recent(usize),
    Open,
    Pin,
    Unpin,
    Close,
    CloseAll,
}

// `picker`: solo la lista de ventanas (clic izquierdo en un grupo con varias). El menú contextual la antepone cuando hay más de una.
#[derive(Clone, Copy)]
pub struct Caps {
    pub launchable: bool,
    pub pinned: bool,
    pub pinnable: bool,
    pub picker: bool,
    pub closable: bool,
    pub recents: usize,
}

pub fn acts(wins: &[isize], c: Caps) -> Vec<Act> {
    let Caps { launchable, pinned, pinnable, picker, closable, recents } = c;
    let windows = wins.len();
    let mut v = Vec::new();
    if picker || windows > 1 {
        v.extend(wins.iter().map(|w| Act::Focus(*w)));
    }
    if picker {
        return v;
    }
    v.extend((0..recents).map(Act::Recent));
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
    v
}

fn label(a: Act, windows: usize) -> &'static str {
    t(match a {
        Act::Focus(_) | Act::Group(_) => K::MenuOpen,
        Act::Recent(_) => K::MenuOpen,
        Act::Open if windows > 0 => K::MenuNewWindow,
        Act::Open => K::MenuOpen,
        Act::Pin => K::MenuPin,
        Act::Unpin => K::MenuUnpin,
        Act::Close => K::MenuClose,
        Act::CloseAll => K::MenuCloseAll,
    })
}

pub fn close_actions(wins: &[isize], eligible: &[isize], last: Option<isize>) -> (Option<isize>, Vec<isize>) {
    let target = pick_target(wins, 0, last).filter(|id| eligible.contains(id));
    let all = if wins.len() > 1 && wins.iter().all(|id| eligible.contains(id)) { wins.to_vec() } else { Vec::new() };
    (target, all)
}

// Fila bajo `y` (relativo a la ventana), o None en el relleno.
pub fn row_at(y: i32, pad: i32, row: i32, n: usize) -> Option<usize> {
    let r = y - pad;
    (r >= 0 && r < row * n as i32).then_some((r / row) as usize)
}

fn row_at_point(x: i32, y: i32, pad: i32, row: i32, n: usize, width: i32, right_inset: i32) -> Option<usize> {
    (0..width - right_inset).contains(&x).then(|| row_at(y, pad, row, n)).flatten()
}

fn close_target(row: Option<usize>, x: i32, close_x: i32, wins: &[isize], closeable_wins: &[isize]) -> Option<isize> {
    if x < close_x { return None }
    close_selected(row, wins, closeable_wins)
}

fn close_selected(row: Option<usize>, wins: &[isize], closeable_wins: &[isize]) -> Option<isize> {
    let id = *wins.get(row?)?;
    closeable_wins.contains(&id).then_some(id)
}

pub fn visible_rows(n: usize, row: i32, pad: i32, max_height: i32) -> usize {
    if n == 0 { return 0 }
    n.min(((max_height - 2 * pad) / row).max(1) as usize)
}

pub fn wheel_scroll(remainder: i32, delta: i32, lines: usize, page: usize) -> (i32, i32) {
    let total = remainder + delta;
    let steps = total / WHEEL_DELTA as i32;
    let per_step = if lines == usize::MAX { page } else { lines }.min(i32::MAX as usize) as i32;
    (total % WHEEL_DELTA as i32, steps.saturating_mul(per_step))
}

pub fn keep_visible(first: usize, selected: usize, rows: usize, n: usize) -> usize {
    if n == 0 || rows == 0 { return 0 }
    let rows = rows.min(n);
    let first = first.min(n - rows);
    let selected = selected.min(n - 1);
    if selected < first { selected } else if selected >= first + rows { selected + 1 - rows } else { first }
}

// Esquina superior izquierda de un popup centrado sobre `cx`, del lado de la pantalla opuesto al borde del dock.
pub fn popup_pos(cx: i32, bar: R, w: i32, h: i32, bottom: bool, gap: i32, mon: MonRect) -> (i32, i32) {
    let x = (cx - w / 2).clamp(mon.0, (mon.2 - w).max(mon.0));
    let y = if bottom { bar.y - h - gap } else { bar.y + bar.h + gap };
    (x, y.clamp(mon.1, (mon.3 - h).max(mon.1)))
}

pub(crate) struct Ctx {
    pub owner: isize,
    pub groups: Vec<(crate::dock::GroupId, String, Option<(isize, u32)>)>,
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
    pub closeable_wins: Vec<isize>,
    pub recents: Vec<(String, String)>,
}

struct Menu {
    hwnd: HWND,
    acts: Vec<Act>,
    labels: Vec<String>,
    wins: Vec<isize>,
    target: Option<isize>,
    close_all: Vec<isize>,
    owners: Vec<(isize, u32)>,
    groups: Vec<(crate::dock::GroupId, String, Option<(isize, u32)>)>,
    anchor: crate::dock::PopupAnchor,
    launch: Option<(String, String)>,
    recents: Vec<(String, String)>,
    key: String,
    hover: Option<usize>,
    keyboard_selected: Option<usize>,
    keyboard_close: bool,
    close_hover: Option<usize>,
    close_x: i32,
    closeable_wins: Vec<isize>,
    pos: POINT,
    size: (i32, i32),
    row: i32,
    pad: i32,
    first: usize,
    visible: usize,
    wheel_remainder: i32,
    theme: Theme,
    scale: f32,
    born: std::time::Instant,
    animate: bool,
    bottom: bool,
    hl_y: f32,
    hl_a: f32,
    tick_at: std::time::Instant,
    tracking: bool,
}

const OPEN_MS: f32 = 150.0;
const T_OPEN: usize = 1;
const WM_ACCESS_UPDATE: u32 = WM_APP + 41;

thread_local! { static MENU: RefCell<Option<Menu>> = const { RefCell::new(None) }; }

pub(crate) fn can_reconcile() -> bool {
    MENU.with(|m| m.try_borrow_mut().is_ok())
}

pub(crate) fn identity() -> Option<(isize, isize)> {
    MENU.with(|m| m.try_borrow().ok()?.as_ref().map(|m| (m.hwnd.0 as isize, m.anchor.owner)))
}

pub fn is_open() -> bool {
    MENU.with(|m| m.try_borrow().map(|m| m.is_some()).unwrap_or(true))
}

const ACTIVATION_DEADLINE: std::time::Duration = std::time::Duration::from_secs(20);
// sin Explorer una app UWP no devuelve error: se cuelga, y el aviso no puede tardar 20 s
const SESSION_ACTIVATION_DEADLINE: std::time::Duration = std::time::Duration::from_secs(3);

// Err(None) es plazo vencido: sin Explorer ActivateApplication puede no volver nunca
fn activate_with_deadline(activate: impl FnOnce() -> Result<(), i32> + Send + 'static, deadline: std::time::Duration) -> Result<(), Option<i32>> {
    let (done, result) = std::sync::mpsc::channel();
    std::thread::spawn(move || { let _ = done.send(activate()); });
    match result.recv_timeout(deadline) {
        Ok(outcome) => outcome.map_err(Some),
        Err(_) => Err(None),
    }
}

fn report_unlaunched(target: &str, hresult: Option<i32>) {
    let Some(host) = crate::HOST.get() else { return };
    let why = hresult.map_or_else(|| "timeout".to_owned(), |h| format!("hr=0x{:08X}", h as u32));
    crate::boot_log(&host.root, &format!("launch failed: {target} {why}"));
    if host.session_shell {
        crate::notify(t(K::UwpUnavailable));
    }
}

// applets Win32 que sí abren sin Explorer; SettingsFallback de winarchy.ahk repite los suyos
const SETTINGS_FALLBACK: &[(&str, &str, &str)] = &[
    ("ms-settings:sound", "control.exe", "mmsys.cpl"),
    ("ms-settings:apps-volume", "SndVol.exe", ""),
    ("ms-settings:network", "control.exe", "ncpa.cpl"),
    ("ms-settings:network-wifi", "control.exe", "ncpa.cpl"),
    ("ms-settings:powersleep", "control.exe", "powercfg.cpl"),
    ("ms-settings:dateandtime", "control.exe", "timedate.cpl"),
];

fn settings_fallback(uri: &str) -> Option<(&'static str, &'static str)> {
    SETTINGS_FALLBACK.iter().find(|(known, ..)| *known == uri).map(|&(_, exe, args)| (exe, args))
}

pub fn launch(path: &str, args: &str) {
    // Configuración es UWP: sin Explorer no abre y ShellExecute no lo informa
    if path.starts_with("ms-settings:") && crate::HOST.get().is_some_and(|h| h.session_shell) {
        match settings_fallback(path) {
            Some((exe, args)) => launch(exe, args),
            None => report_unlaunched(path, Some(0x80040900u32 as i32)),
        }
        return;
    }
    // sin Explorer, ShellExecute de shell:AppsFolder falla con "Class not registered";
    // la activación va en un hilo propio porque bloquea y bombea mensajes en el de UI
    if let Some(aumid) = path.strip_prefix(r"shell:AppsFolder\") {
        let (aumid, fallback) = (aumid.to_owned(), (path.to_owned(), args.to_owned()));
        std::thread::spawn(move || {
            let wide_id: Vec<u16> = aumid.encode_utf16().chain(std::iter::once(0)).collect();
            let activate = move || unsafe {
                use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                let activated = CoCreateInstance::<_, IApplicationActivationManager>(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER)
                    .and_then(|m| m.ActivateApplication(PCWSTR(wide_id.as_ptr()), PCWSTR::null(), AO_NONE));
                CoUninitialize();
                activated.map(drop).map_err(|e| e.code().0)
            };
            let session = crate::HOST.get().is_some_and(|h| h.session_shell);
            if let Err(hresult) = activate_with_deadline(activate, if session { SESSION_ACTIVATION_DEADLINE } else { ACTIVATION_DEADLINE }) {
                report_unlaunched(&fallback.0, hresult);
                if !session {
                    shell_execute(&fallback.0, &fallback.1);
                }
            }
        });
        return;
    }
    // un destino de red caído puede bloquear ShellExecute minutos: nunca en el hilo de UI
    let (path, args) = (path.to_owned(), args.to_owned());
    std::thread::spawn(move || unsafe {
        use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        shell_execute(&path, &args);
        CoUninitialize();
    });
}

fn shell_execute(path: &str, args: &str) {
    let wide = |t: &str| -> Vec<u16> { t.encode_utf16().chain(std::iter::once(0)).collect() };
    let (p, a) = (wide(path), wide(args));
    // como Explorer: un ancla .exe/.bat con rutas relativas corre desde su carpeta
    let dir = std::path::Path::new(path).parent().filter(|d| d.is_dir()).map(|d| wide(&d.to_string_lossy()));
    unsafe {
        let params = if args.is_empty() { PCWSTR::null() } else { PCWSTR(a.as_ptr()) };
        let directory = dir.as_ref().map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr()));
        let _ = ShellExecuteW(None, w!("open"), PCWSTR(p.as_ptr()), params, directory, SW_SHOWNORMAL);
    }
}

unsafe fn font_for(theme: &Theme, scale: f32) -> HFONT {
    let family: Vec<u16> = theme.font_family.encode_utf16().chain(std::iter::once(0)).collect();
    CreateFontW(-px((theme.font_size - 1).max(10), scale), 0, 0, 0, 500, 0, 0, 0, 1, 0, 0, ANTIALIASED_QUALITY.0 as u32, 0, PCWSTR(family.as_ptr()))
}

pub(crate) unsafe fn show(c: Ctx) {
    dismiss();
    let (target, close_all) = close_actions(&c.wins, &c.closeable_wins, c.last);
    let mut list = if c.groups.is_empty() { acts(&c.wins, Caps { launchable: c.launch.is_some(), pinned: c.pinned, pinnable: c.pinnable, picker: c.picker, closable: c.closable, recents: c.recents.len() }) } else { (0..c.groups.len()).map(Act::Group).collect() };
    list.retain(|action| match action { Act::Close => target.is_some(), Act::CloseAll => !close_all.is_empty(), _ => true });
    if list.is_empty() {
        return;
    }
    let current = c.active.filter(|a| c.wins.contains(a)).or(c.last);
    let labels: Vec<String> = list
        .iter()
        .map(|a| match a {
            Act::Group(i) => c.groups[*i].1.clone(),
            Act::Recent(i) => crate::dock::clip_title(&c.recents[*i].0, 40),
            Act::Focus(id) => {
                let i = c.wins.iter().position(|w| w == id).unwrap_or(0);
                let title = c.titles.get(i).filter(|t| !t.is_empty()).cloned().unwrap_or_else(|| "…".into());
                format!("{}{title}", if current == Some(*id) { "• " } else { "   " })
            }
            other => label(*other, c.wins.len()).to_string(),
        })
        .collect();
    let (row, pad, padx) = (px(32, c.scale), px(6, c.scale), px(14, c.scale));
    let close_button = if c.picker && !c.closeable_wins.is_empty() { px(26, c.scale) } else { 0 };
    let screen = GetDC(None);
    let mem = CreateCompatibleDC(screen);
    let font = font_for(&c.theme, c.scale);
    let old = SelectObject(mem, font);
    let mut widest = px(150, c.scale);
    for l in &labels {
        let wide: Vec<u16> = l.encode_utf16().collect();
        let mut e = SIZE::default();
        let _ = GetTextExtentPoint32W(mem, &wide, &mut e);
        widest = widest.max(e.cx + 2 * padx + close_button);
    }
    SelectObject(mem, old);
    let _ = DeleteObject(font);
    let _ = DeleteDC(mem);
    ReleaseDC(None, screen);

    let gap = px(8, c.scale);
    let available = (if c.bottom { c.bar.y - c.mon.1 - gap } else { c.mon.3 - c.bar.y - c.bar.h - gap }) - crate::layout::bar_h(c.scale) - gap;
    let visible = visible_rows(list.len(), row, pad, available);
    let (w, h) = (widest.min(c.mon.2 - c.mon.0), row * visible as i32 + 2 * pad);
    let close_x = w - padx - close_button;
    let (x, y) = popup_pos(c.cx, c.bar, w, h, c.bottom, gap, c.mon);
    let Ok(inst) = GetModuleHandleW(PCWSTR::null()) else { return };
    let class = WNDCLASSW { lpfnWndProc: Some(menu_proc), hInstance: inst.into(), hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(), lpszClassName: w!("WinarchyShellDockMenu"), ..Default::default() };
    RegisterClassW(&class);
    let Ok(hwnd) = CreateWindowExW(WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED, class.lpszClassName, w!(""), WS_POPUP, x, y, w, h, None, None, inst, None) else { return };
    crate::accessibility::create(hwnd, t(K::Applications), accesskit::Role::Menu);
    let owners = c.wins.iter().map(|id| { let mut pid = 0; GetWindowThreadProcessId(HWND(*id as *mut _), Some(&mut pid)); (*id, pid) }).collect();
    let closeable_wins = if c.picker { c.closeable_wins } else { Vec::new() };
    let anchor = crate::dock::PopupAnchor { owner: c.owner, cx: c.cx, bar: c.bar, bottom: c.bottom, mon: c.mon, scale: c.scale };
    let menu = Menu { hwnd, acts: list, labels, target, close_all, owners, groups: c.groups, anchor, wins: c.wins, closeable_wins, launch: c.launch, recents: c.recents, key: c.key, hover: None, keyboard_selected: None, keyboard_close: false, close_hover: None, close_x, pos: POINT { x, y }, size: (w, h), row, pad, first: 0, visible, wheel_remainder: 0, animate: crate::dock::motion_enabled(&c.theme), theme: c.theme, scale: c.scale, born: std::time::Instant::now(), bottom: c.bottom, hl_y: 0.0, hl_a: 0.0, tick_at: std::time::Instant::now(), tracking: false };
    paint(&menu);
    let animate = menu.animate;
    MENU.with(|m| *m.borrow_mut() = Some(menu));
    publish_accessibility(hwnd);
    if animate {
        SetTimer(hwnd, T_OPEN, 16, None);
    }
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
}

pub fn dismiss() {
    let m = MENU.with(|m| m.borrow_mut().take());
    if let Some(m) = m {
        crate::accessibility::destroy(m.hwnd);
        unsafe {
            let _ = DestroyWindow(m.hwnd);
        }
    }
}

fn a11y_row_keys(action: Act, owner: Option<(isize, u32)>, group: Option<&crate::dock::GroupId>, close_eligible: bool) -> Vec<String> {
    match action {
        Act::Focus(hwnd) => match owner {
            Some((id, pid)) if id == hwnd && pid != 0 => {
                let mut keys = vec![format!("window:{hwnd}:{pid}")];
                if close_eligible { keys.push(format!("close:{hwnd}:{pid}")); }
                keys
            }
            _ => Vec::new(),
        },
        Act::Group(_) => match group {
            Some(crate::dock::GroupId::App(id)) => vec![format!("group:app:{id}")],
            Some(crate::dock::GroupId::Pin(id)) => vec![format!("group:pin:{id}")],
            None => Vec::new(),
        },
        Act::Recent(i) => vec![format!("action:recent:{i}")],
        Act::Open => vec!["action:open".into()],
        Act::Pin => vec!["action:pin".into()],
        Act::Unpin => vec!["action:unpin".into()],
        Act::Close => vec!["action:close".into()],
        Act::CloseAll => vec!["action:close_all".into()],
    }
}

fn row_keys(m: &Menu, i: usize) -> Vec<String> {
    let Some(action) = m.acts.get(i).copied() else { return Vec::new() };
    let owner = match action { Act::Focus(hwnd) => m.owners.iter().find(|(id, _)| *id == hwnd).copied(), _ => None };
    let group = match action { Act::Group(index) => m.groups.get(index).map(|(id, _, _)| id), _ => None };
    let eligible = matches!(action, Act::Focus(hwnd) if m.closeable_wins.contains(&hwnd));
    a11y_row_keys(action, owner, group, eligible)
}

fn accessible_target(m: &Menu, key: &str) -> Option<(usize, bool)> {
    m.acts.iter().enumerate().find_map(|(i, _)| {
        row_keys(m, i).iter().position(|candidate| candidate == key).map(|part| (i, part == 1))
    })
}

fn publish_accessibility(hwnd: HWND) {
    let data = MENU.with(|menu| {
        let menu = menu.try_borrow().ok()?;
        let m = menu.as_ref().filter(|m| m.hwnd == hwnd)?;
        let mut nodes = Vec::new();
        let mut focus = None;
        for i in 0..m.acts.len() {
            let keys = row_keys(m, i);
            if keys.is_empty() { continue }
            let top = m.pad + (i as i32 - m.first as i32) * m.row;
            let onscreen = i >= m.first && i < m.first + m.visible;
            let title = m.labels[i].trim_start_matches('•').trim_start().to_string();
            let mut row = accesskit::Node::new(accesskit::Role::MenuItem);
            row.set_label(title.clone());
            let right = if keys.len() > 1 { m.close_x } else { m.size.0 };
            row.set_bounds(if onscreen { accesskit::Rect::new(0.0, top as f64, right as f64, (top + m.row) as f64) } else { accesskit::Rect::new(0.0, 0.0, 0.0, 0.0) });
            row.add_action(accesskit::Action::Click);
            row.add_action(accesskit::Action::Focus);
            row.add_action(accesskit::Action::ScrollIntoView);
            if m.keyboard_selected == Some(i) { focus = Some(keys[usize::from(m.keyboard_close && keys.len() > 1)].clone()); }
            nodes.push((keys[0].clone(), row));
            if let Some(key) = keys.get(1) {
                let mut close = accesskit::Node::new(accesskit::Role::Button);
                close.set_label(format!("{} · {}", t(K::MenuClose), title));
                close.set_bounds(if onscreen { accesskit::Rect::new(m.close_x as f64, top as f64, (m.size.0 - px(14, m.scale)) as f64, (top + m.row) as f64) } else { accesskit::Rect::new(0.0, 0.0, 0.0, 0.0) });
                close.add_action(accesskit::Action::Click);
                close.add_action(accesskit::Action::Focus);
                close.add_action(accesskit::Action::ScrollIntoView);
                nodes.push((key.clone(), close));
            }
        }
        Some((m.size, nodes, focus))
    });
    if let Some((size, nodes, focus)) = data {
        crate::accessibility::publish(hwnd, t(K::Applications), size, nodes, focus.as_deref());
    }
}

fn keyboard_row(current: Option<usize>, n: usize, first: usize, visible: usize, key: u16) -> Option<usize> {
    if n == 0 { return None }
    let first = first.min(n - 1);
    let last = first.saturating_add(visible.saturating_sub(1)).min(n - 1);
    let current = current.map(|i| i.min(n - 1));
    match key {
        k if k == VK_HOME.0 => Some(0),
        k if k == VK_END.0 => Some(n - 1),
        k if k == VK_DOWN.0 => current.map(|i| (i + 1) % n).or(Some(first)),
        k if k == VK_UP.0 => current.map(|i| if i == 0 { n - 1 } else { i - 1 }).or(Some(last)),
        k if k == VK_NEXT.0 => current.map(|i| i.saturating_add(visible.max(1)).min(n - 1)).or(Some(last)),
        k if k == VK_PRIOR.0 => current.map(|i| i.saturating_sub(visible.max(1))).or(Some(first)),
        _ => current,
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
    for (i, l) in m.labels.iter().enumerate().skip(m.first).take(m.visible) {
        let mut wide: Vec<u16> = l.encode_utf16().collect();
        let can_close = matches!(m.acts.get(i), Some(Act::Focus(id)) if m.closeable_wins.contains(id));
        let local = (i - m.first) as i32;
        let mut rc = RECT { left: padx, top: m.pad + local * m.row, right: if can_close { m.close_x - px(4, s) } else { w - padx }, bottom: m.pad + (local + 1) * m.row };
        if !wide.is_empty() {
            DrawTextW(mem, &mut wide, &mut rc, DT_LEFT | DT_VCENTER | DT_NOPREFIX | DT_SINGLELINE | DT_END_ELLIPSIS);
        }
        if can_close {
            let mut close: Vec<u16> = "×".encode_utf16().collect();
            let mut rc = RECT { left: m.close_x, top: rc.top, right: w - padx, bottom: rc.bottom };
            DrawTextW(mem, &mut close, &mut rc, DT_CENTER | DT_VCENTER | DT_NOPREFIX | DT_SINGLELINE);
        }
    }
    let coverage: Vec<u32> = buf.iter().map(|p| ((*p >> 16) & 255).max((*p >> 8) & 255).max(*p & 255)).collect();
    buf.fill(0);
    let (bg, fg, accent) = (rgb(&m.theme.background), rgb(&m.theme.foreground), rgb(&m.theme.accent));
    fill_round_a(buf, w, h, R { x: 0, y: 0, w, h }, px(12, s), bg, 0.97);
    if m.visible < m.acts.len() {
        let track = h - 2 * m.pad;
        let thumb = ((track as usize * m.visible / m.acts.len()) as i32).max(px(8, s));
        let travel = track - thumb;
        let y = m.pad + (travel as usize * m.first / (m.acts.len() - m.visible)) as i32;
        fill_round_a(buf, w, h, R { x: w - px(4, s), y, w: px(2, s).max(1), h: thumb }, px(1, s), fg, 0.65);
    }
    if m.hl_a > 0.0 && m.hl_y >= m.first as f32 && m.hl_y < (m.first + m.visible) as f32 {
        let inset = px(5, s);
        fill_round_a(buf, w, h, R { x: inset, y: m.pad + ((m.hl_y - m.first as f32) * m.row as f32).round() as i32, w: w - 2 * inset, h: m.row }, px(8, s), accent, 0.30 * m.hl_a);
    }
    let close_active = m.close_hover.or_else(|| m.keyboard_selected.filter(|_| m.keyboard_close)).filter(|i| *i >= m.first && *i < m.first + m.visible);
    if let Some(i) = close_active {
        fill_round_a(buf, w, h, R { x: m.close_x, y: m.pad + (i - m.first) as i32 * m.row + px(4, s), w: w - padx - m.close_x, h: m.row - px(8, s) }, px(5, s), accent, 1.0);
    }
    let close_fg = close_glyph_color(accent);
    for (i, c) in coverage.iter().enumerate() {
        if *c > 0 {
            let x = (i % w as usize) as i32;
            let y = (i / w as usize) as i32;
            let selected_close = close_active.is_some_and(|row| x >= m.close_x && x < w - padx && y >= m.pad + (row - m.first) as i32 * m.row && y < m.pad + (row - m.first + 1) as i32 * m.row);
            blend(&mut buf[i], premul(if selected_close { close_fg } else { fg }, *c as f32 / 255.0));
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
    let _ = PostMessageW(m.hwnd, WM_ACCESS_UPDATE, WPARAM(0), LPARAM(0));
}

// El resaltado se desliza entre filas y se funde al salir; sin animación salta directo.
fn set_hover(i: Option<usize>) {
    set_hover_at(i, None);
}

fn set_hover_at(i: Option<usize>, x: Option<i32>) {
    let hwnd = MENU.with(|m| {
        let mut m = m.borrow_mut();
        let m = m.as_mut()?;
        let close_hover = i.filter(|r| {
            x.is_some_and(|x| x >= m.close_x) && matches!(m.acts.get(*r), Some(Act::Focus(id)) if m.closeable_wins.contains(id))
        });
        let first = i.map(|r| keep_visible(m.first, r, m.visible, m.acts.len())).unwrap_or(m.first);
        let scrolled = first != m.first;
        m.first = first;
        if scrolled { if let Some(r) = i { m.hl_y = r as f32; } }
        let changed = m.hover != i || m.close_hover != close_hover || scrolled;
        if !changed {
            return None;
        }
        if m.hover != i {
            if let Some(r) = i {
                if !m.animate || m.hl_a < 0.05 {
                    m.hl_y = r as f32;
                }
            }
            if !m.animate {
                m.hl_a = if i.is_some() { 1.0 } else { 0.0 };
            }
        }
        m.hover = i;
        m.close_hover = close_hover;
        m.tick_at = std::time::Instant::now();
        Some((m.hwnd, m.animate))
    });
    let Some((hwnd, animate)) = hwnd else { return };
    if animate {
        unsafe { SetTimer(hwnd, T_OPEN, 16, None) };
    } else {
        MENU.with(|m| m.borrow().as_ref().map(|m| unsafe { paint(m) }));
    }
}

// Un cuadro del resaltado; devuelve si todavía falta recorrido.
fn step_highlight(m: &mut Menu) -> bool {
    let now = std::time::Instant::now();
    let dt = (now.duration_since(m.tick_at).as_secs_f32() * 1000.0).min(50.0);
    m.tick_at = now;
    let ty = m.hover.map(|r| r as f32).unwrap_or(m.hl_y);
    m.hl_y += (ty - m.hl_y) * (1.0 - (-dt / 40.0).exp());
    if (ty - m.hl_y).abs() < 0.01 {
        m.hl_y = ty;
    }
    let ta = if m.hover.is_some() { 1.0 } else { 0.0 };
    m.hl_a = crate::dock::approach(m.hl_a, ta, dt, 110.0);
    m.hl_y != ty || m.hl_a != ta
}

fn activate(i: usize) {
    let m = MENU.with(|m| m.borrow_mut().take());
    let Some(m) = m else { return };
    crate::accessibility::destroy(m.hwnd);
    unsafe {
        let _ = DestroyWindow(m.hwnd);
    }
    let close = |id: isize| close_window(id, &m.owners);
    crate::dock::on_dock(m.anchor.owner, || match m.acts.get(i) {
        Some(Act::Group(index)) => if let Some((id, _, witness)) = m.groups.get(*index) { crate::dock::select_overflow_group(m.anchor.owner, id, *witness, m.anchor); },
        Some(Act::Focus(id)) => unsafe {
            let h = HWND(*id as *mut _);
            let mut pid = 0;
            GetWindowThreadProcessId(h, Some(&mut pid));
            if pid != 0 && m.owners.contains(&(*id, pid)) && IsWindow(h).as_bool() {
                crate::dock::focus_window(h);
            }
        },
        Some(Act::Recent(index)) => if let Some((_, path)) = m.recents.get(*index) { launch(path, ""); },
        Some(Act::Open) => m.launch.iter().for_each(|(o, a)| launch(o, a)),
        Some(Act::Pin | Act::Unpin) => crate::dock::toggle_pin(&m.key),
        Some(Act::Close) => m.target.into_iter().for_each(close),
        Some(Act::CloseAll) => m.close_all.iter().copied().for_each(close),
        None => {}
    });
    if !is_open() { crate::dock::end_keyboard(false); }
}

fn close_window(id: isize, owners: &[(isize, u32)]) {
    unsafe {
        let h = HWND(id as *mut _);
        let mut pid = 0;
        GetWindowThreadProcessId(h, Some(&mut pid));
        if pid != 0 && owners.contains(&(id, pid)) && IsWindow(h).as_bool() && !crate::dock::is_elevated(h) {
            let _ = PostMessageW(h, WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

fn activate_close(id: isize) {
    let m = MENU.with(|m| m.borrow_mut().take());
    let Some(m) = m else { return };
    crate::accessibility::destroy(m.hwnd);
    unsafe {
        let _ = DestroyWindow(m.hwnd);
        close_window(id, &m.owners);
    }
    crate::dock::return_keyboard();
}

fn y_of(lp: LPARAM) -> i32 {
    ((lp.0 >> 16) & 0xffff) as i16 as i32
}

fn x_of(lp: LPARAM) -> i32 {
    (lp.0 & 0xffff) as i16 as i32
}

unsafe extern "system" fn menu_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_GETOBJECT { if let Some(result) = crate::accessibility::get_object(h, wp, lp) { return result; } }
    if msg == WM_NCDESTROY { crate::accessibility::destroy(h); }
    let rows = |lp: LPARAM| MENU.with(|m| m.try_borrow().ok().and_then(|m| m.as_ref().and_then(|m| row_at_point(x_of(lp), y_of(lp), m.pad, m.row, m.visible, m.size.0, px(14, m.scale)).map(|r| r + m.first))));
    match msg {
        WM_ACCESS_UPDATE => { publish_accessibility(h); LRESULT(0) }
        crate::accessibility::WM_ACTION => {
            for (key, action) in crate::accessibility::actions(h, wp.0) {
                let target = MENU.with(|m| m.try_borrow().ok().and_then(|m| m.as_ref().and_then(|m| accessible_target(m, &key))));
                let Some((index, close)) = target else { continue };
                match action {
                    accesskit::Action::Click if close => {
                        let id = MENU.with(|m| m.borrow().as_ref().and_then(|m| close_selected(Some(index), &m.wins, &m.closeable_wins)));
                        if let Some(id) = id { activate_close(id); }
                        break;
                    }
                    accesskit::Action::Click => { activate(index); break; }
                    accesskit::Action::Focus => {
                        MENU.with(|m| { if let Some(m) = m.borrow_mut().as_mut() { m.keyboard_selected = Some(index); m.keyboard_close = close; } });
                        set_hover(Some(index));
                        MENU.with(|m| { if let Some(m) = m.borrow().as_ref() { paint(m); } });
                        let _ = PostMessageW(h, WM_ACCESS_UPDATE, WPARAM(0), LPARAM(0));
                        let _ = SetForegroundWindow(h);
                    }
                    accesskit::Action::ScrollIntoView => {
                        MENU.with(|m| {
                            if let Some(m) = m.borrow_mut().as_mut() {
                                m.first = keep_visible(m.first, index, m.visible, m.acts.len());
                                paint(m);
                            }
                        });
                    }
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            crate::accessibility::focus(h, msg == WM_SETFOCUS);
            DefWindowProcW(h, msg, wp, lp)
        }
        WM_TIMER if wp.0 == T_OPEN => {
            let done = MENU.with(|m| {
                let mut m = m.try_borrow_mut().ok()?;
                let m = m.as_mut()?;
                let moving = step_highlight(m);
                unsafe { paint(m) };
                Some(!moving && open_progress(m) >= 1.0)
            });
            if done != Some(false) {
                let _ = KillTimer(h, T_OPEN);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let need_track = MENU.with(|m| m.borrow_mut().as_mut().is_some_and(|m| { let need = !m.tracking; m.tracking = true; need }));
            if need_track {
                let _ = TrackMouseEvent(&mut TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: h, dwHoverTime: 0 });
            }
            MENU.with(|m| { if let Some(m) = m.borrow_mut().as_mut() { m.keyboard_selected = None; m.keyboard_close = false; } });
            set_hover_at(rows(lp), Some(x_of(lp)));
            let _ = PostMessageW(h, WM_ACCESS_UPDATE, WPARAM(0), LPARAM(0));
            LRESULT(0)
        }
        0x02A3 => {
            MENU.with(|m| { if let Some(m) = m.borrow_mut().as_mut() { m.tracking = false; } });
            if MENU.with(|m| m.borrow().as_ref().is_none_or(|m| m.keyboard_selected.is_none())) { set_hover(None); }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp.0 >> 16) & 0xffff) as i16;
            if delta == 0 { return LRESULT(0) }
            let mut setting = 3u32;
            let _ = SystemParametersInfoW(SPI_GETWHEELSCROLLLINES, 0, Some(&mut setting as *mut _ as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
            let lines = if setting == u32::MAX { usize::MAX } else { setting as usize };
            MENU.with(|m| {
                if let Some(m) = m.borrow_mut().as_mut() {
                    let (remainder, amount) = wheel_scroll(m.wheel_remainder, delta as i32, lines, m.visible);
                    m.wheel_remainder = remainder;
                    if amount != 0 {
                        let max = m.acts.len().saturating_sub(m.visible);
                        m.first = if amount > 0 { m.first.saturating_sub(amount as usize) } else { m.first.saturating_add(amount.unsigned_abs() as usize).min(max) };
                        m.hover = None;
                        m.keyboard_selected = None;
                        m.keyboard_close = false;
                        m.close_hover = None;
                        m.hl_a = 0.0;
                        unsafe { paint(m) };
                    }
                }
            });
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if MENU.with(|m| m.borrow().as_ref().is_some_and(|m| x_of(lp) >= m.size.0 - px(14, m.scale))) { return LRESULT(0); }
            match rows(lp) {
                Some(i) => {
                    let target = MENU.with(|m| m.borrow().as_ref().and_then(|m| close_target(Some(i), x_of(lp), m.close_x, &m.wins, &m.closeable_wins)));
                    if let Some(id) = target { activate_close(id) } else { activate(i) }
                }
                None => dismiss(),
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let n = MENU.with(|m| m.try_borrow().ok().and_then(|m| m.as_ref().map(|m| (m.acts.len(), m.keyboard_selected.or(m.hover), m.first, m.visible, m.keyboard_close))));
            match (wp.0 as u16, n) {
                (k, _) if k == VK_ESCAPE.0 => { dismiss(); crate::dock::return_keyboard(); },
                (k, Some((n, cur, first, visible, _))) if [VK_DOWN.0, VK_UP.0, VK_HOME.0, VK_END.0, VK_NEXT.0, VK_PRIOR.0].contains(&k) => {
                    let selected = keyboard_row(cur, n, first, visible, k);
                    MENU.with(|m| { if let Some(m) = m.borrow_mut().as_mut() { m.keyboard_selected = selected; m.keyboard_close = false; } });
                    set_hover(selected);
                    let _ = PostMessageW(h, WM_ACCESS_UPDATE, WPARAM(0), LPARAM(0));
                }
                (k, Some((_, Some(i), _, _, _))) if k == VK_DELETE.0 => {
                    let target = MENU.with(|m| m.borrow().as_ref().and_then(|m| close_selected(Some(i), &m.wins, &m.closeable_wins)));
                    if let Some(id) = target { activate_close(id) }
                }
                (k, Some((_, Some(i), _, _, _))) if k == VK_LEFT.0 || k == VK_RIGHT.0 => {
                    MENU.with(|m| {
                        if let Some(m) = m.borrow_mut().as_mut() {
                            m.keyboard_selected = Some(i);
                            m.keyboard_close = k == VK_RIGHT.0 && close_selected(Some(i), &m.wins, &m.closeable_wins).is_some();
                            paint(m);
                        }
                    });
                }
                (k, Some((_, Some(i), _, _, close))) if k == VK_RETURN.0 || k == VK_SPACE.0 => {
                    if close {
                        let target = MENU.with(|m| m.borrow().as_ref().and_then(|m| close_selected(Some(i), &m.wins, &m.closeable_wins)));
                        if let Some(id) = target { activate_close(id) }
                    } else { activate(i); }
                }
                _ => {}
            }
            LRESULT(0)
        }
        // WA_INACTIVE: otra ventana tomó el foco.
        WM_ACTIVATE if wp.0 & 0xffff == 0 && is_open() => {
            dismiss();
            crate::dock::end_keyboard(false);
            LRESULT(0)
        }
        _ => DefWindowProcW(h, msg, wp, lp),
    }
}

#[cfg(test)]
#[path = "dock_menu_wheel_tests.rs"]
mod wheel_tests;

#[cfg(test)]
#[path = "dock_menu_keyboard_tests.rs"]
mod keyboard_tests;

#[cfg(test)]
#[path = "dock_menu_a11y_tests.rs"]
mod a11y_tests;

#[cfg(test)]
#[path = "dock_menu_reconcile_tests.rs"]
mod reconcile_tests;

#[cfg(test)]
#[path = "dock_menu_uia_probe_tests.rs"]
mod uia_probe_tests;

#[cfg(test)]
#[path = "dock_menu_contrast_tests.rs"]
mod contrast_tests;

#[cfg(test)]
#[path = "dock_menu_scroll_tests.rs"]
mod scroll_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_activation_that_never_returns_or_fails_is_reported() {
        let wait = std::time::Duration::from_millis(200);
        assert_eq!(activate_with_deadline(|| Ok(()), wait), Ok(()));
        assert_eq!(activate_with_deadline(|| Err(-5), wait), Err(Some(-5)));
        assert_eq!(activate_with_deadline(|| { std::thread::sleep(std::time::Duration::from_secs(30)); Ok(()) }, wait), Err(None));
    }

    fn caps(l: bool, pinned: bool, pinnable: bool, picker: bool) -> Caps {
        Caps { launchable: l, pinned, pinnable, picker, closable: true, recents: 0 }
    }

    #[test]
    fn recents_go_before_the_app_actions_and_never_in_the_picker() {
        let with = |picker| Caps { recents: 2, ..caps(true, false, true, picker) };
        assert_eq!(acts(&[1], with(false)), [Act::Recent(0), Act::Recent(1), Act::Open, Act::Pin, Act::Close]);
        assert_eq!(acts(&[1, 2], with(true)), [Act::Focus(1), Act::Focus(2)]);
    }

    #[test]
    fn close_actions_offer_the_last_window_and_all_normal_windows() {
        assert_eq!(close_actions(&[7, 8], &[8, 7], Some(8)), (Some(8), vec![7, 8]));
    }

    #[test]
    fn close_actions_offer_only_close_for_a_single_normal_window() {
        assert_eq!(close_actions(&[7], &[7], None), (Some(7), vec![]));
    }

    #[test]
    fn close_actions_do_not_replace_an_elevated_target_with_another_window() {
        assert_eq!(close_actions(&[7, 8], &[7], Some(8)), (None, vec![]));
    }

    #[test]
    fn close_actions_allow_a_normal_target_but_not_close_all_in_a_mixed_group() {
        assert_eq!(close_actions(&[7, 8], &[8], Some(8)), (Some(8), vec![]));
    }

    #[test]
    fn close_actions_fall_back_to_the_first_window_when_last_is_stale() {
        assert_eq!(close_actions(&[7, 8], &[7, 8], Some(99)), (Some(7), vec![7, 8]));
    }

    #[test]
    fn close_actions_do_not_skip_an_elevated_first_window_when_last_is_stale() {
        assert_eq!(close_actions(&[7, 8], &[8], Some(99)), (None, vec![]));
    }

    #[test]
    fn close_actions_are_empty_without_eligible_windows() {
        assert_eq!(close_actions(&[7, 8], &[], Some(8)), (None, vec![]));
    }

    #[test]
    fn close_actions_are_empty_without_windows() {
        assert_eq!(close_actions(&[], &[], Some(99)), (None, vec![]));
    }

    #[test]
    fn closing_requires_a_window_row() {
        assert_eq!(close_target(None, 200, 180, &[7, 8], &[7, 8]), None);
        assert_eq!(close_target(Some(2), 200, 180, &[7, 8], &[7, 8]), None);
        assert_eq!(close_target(Some(0), 200, 180, &[], &[]), None);
    }

    #[test]
    fn clicking_before_the_close_zone_does_not_close_a_window() {
        assert_eq!(close_target(Some(0), 179, 180, &[7, 8], &[7, 8]), None);
    }

    #[test]
    fn an_ineligible_window_cannot_be_closed() {
        assert_eq!(close_target(Some(0), 200, 180, &[7, 8], &[8]), None);
        assert_eq!(close_target(Some(1), 200, 180, &[7, 8], &[]), None);
    }

    #[test]
    fn closing_targets_the_hovered_window_including_the_zone_boundary() {
        assert_eq!(close_target(Some(1), 180, 180, &[7, 8], &[8, 7]), Some(8));
        assert_eq!(close_target(Some(0), 200, 180, &[7, 8], &[8, 7]), Some(7));
    }

    #[test]
    fn delete_closes_only_the_selected_eligible_window() {
        assert_eq!(close_selected(Some(1), &[7, 8], &[8]), Some(8));
        assert_eq!(close_selected(Some(0), &[7, 8], &[8]), None);
        assert_eq!(close_selected(None, &[7, 8], &[7, 8]), None);
        assert_eq!(close_selected(Some(2), &[7, 8], &[7, 8]), None);
        assert_eq!(close_selected(Some(0), &[], &[]), None);
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
    fn elevated_windows_cannot_be_closed_and_app_actions_stay_in_the_app_menu() {
        let c = Caps { closable: false, ..caps(true, false, false, false) };
        assert_eq!(acts(&[1, 2], c), [Act::Focus(1), Act::Focus(2), Act::Open]);
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
        assert_eq!(keyboard_row(None, 3, 0, 3, VK_DOWN.0), Some(0));
        assert_eq!(keyboard_row(None, 3, 0, 3, VK_UP.0), Some(2));
        assert_eq!(keyboard_row(Some(2), 3, 0, 3, VK_DOWN.0), Some(0));
        assert_eq!(keyboard_row(Some(0), 3, 0, 3, VK_UP.0), Some(2));
        assert_eq!(keyboard_row(None, 0, 0, 0, VK_DOWN.0), None);
    }
}

#[cfg(test)]
mod settings_tests {
    use super::settings_fallback;

    #[test]
    fn settings_links_map_to_applets_that_open_without_explorer() {
        assert_eq!(settings_fallback("ms-settings:sound"), Some(("control.exe", "mmsys.cpl")));
        assert_eq!(settings_fallback("ms-settings:apps-volume"), Some(("SndVol.exe", "")));
        // desk.cpl y bthprops.cpl no abren nada sin Explorer
        assert_eq!(settings_fallback("ms-settings:display"), None);
        assert_eq!(settings_fallback("ms-settings:bluetooth"), None);
    }
}
