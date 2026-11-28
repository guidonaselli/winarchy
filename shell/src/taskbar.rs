// Progreso e insignias de ITaskbarList3: sin Explorer llegan a nuestro Shell_TrayWnd porque anuncia `TaskbandHWND`.
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{IsWindow, PostMessageW, RegisterWindowMessageW, WM_USER};
use crate::dock::Icon;

const PROGRESS_VALUE: u32 = WM_USER + 64;
const PROGRESS_STATE: u32 = WM_USER + 65;
const OVERLAY_ICON: u32 = WM_USER + 79;
// el valor llega escalado a este máximo
const FULL: f32 = 65534.0;

// el orden es la prioridad al juntar las ventanas de un mismo ícono del dock
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    #[default]
    None,
    Indeterminate,
    Normal,
    Paused,
    Error,
}

#[derive(Clone, Default)]
pub struct Entry {
    pub kind: Kind,
    pub fraction: f32,
    pub overlay: Option<Icon>,
}

static ENTRIES: Mutex<Option<HashMap<isize, Entry>>> = Mutex::new(None);
static ANNOUNCED: Mutex<Option<HashSet<isize>>> = Mutex::new(None);

fn set_value(entry: &mut Entry, value: usize) {
    entry.fraction = (value as f32 / FULL).clamp(0.0, 1.0);
    if matches!(entry.kind, Kind::None | Kind::Indeterminate) {
        entry.kind = Kind::Normal;
    }
}

// TBPF_*
fn set_state(entry: &mut Entry, flags: usize) {
    entry.kind = match flags {
        1 => Kind::Indeterminate,
        2 => Kind::Normal,
        4 => Kind::Error,
        8 => Kind::Paused,
        _ => Kind::None,
    };
    if entry.kind == Kind::None {
        entry.fraction = 0.0;
    }
}

/// Atiende un mensaje de la barra de tareas; true si cambió lo que se dibuja.
pub fn handle(message: u32, wp: WPARAM, lp: LPARAM) -> Option<bool> {
    if !matches!(message, PROGRESS_VALUE | PROGRESS_STATE | OVERLAY_ICON) {
        return None;
    }
    let window = wp.0 as isize;
    // el ícono es del proceso emisor y deja de valer cuando su llamada vuelve: se copia ahora
    let overlay = (message == OVERLAY_ICON && lp.0 != 0).then(|| unsafe { crate::dock::icon_from_handle(lp.0 as usize) }).flatten();
    let mut entries = ENTRIES.lock().unwrap();
    let entry = entries.get_or_insert_with(HashMap::new).entry(window).or_default();
    let before = (entry.kind, entry.fraction);
    match message {
        PROGRESS_VALUE => set_value(entry, lp.0 as usize),
        PROGRESS_STATE => set_state(entry, lp.0 as usize),
        _ => {
            entry.overlay = overlay;
            return Some(true);
        }
    }
    Some(before != (entry.kind, entry.fraction))
}

/// Lo que muestra un ícono del dock: el estado más urgente de sus ventanas y la primera insignia.
pub fn group(windows: &[isize]) -> Option<Entry> {
    let entries = ENTRIES.lock().unwrap();
    let found = windows.iter().filter_map(|w| entries.as_ref()?.get(w)).collect::<Vec<_>>();
    let progress = found.iter().rev().max_by_key(|e| e.kind)?;
    let overlay = found.iter().find_map(|e| e.overlay.clone());
    (progress.kind != Kind::None || overlay.is_some()).then(|| Entry { kind: progress.kind, fraction: progress.fraction, overlay })
}

/// Avisa una vez a cada ventana que ya puede informar progreso, como hace Explorer al crear su botón.
pub fn announce(window: isize) {
    if ANNOUNCED.lock().unwrap().get_or_insert_with(HashSet::new).insert(window) {
        unsafe {
            let created = RegisterWindowMessageW(w!("TaskbarButtonCreated"));
            let _ = PostMessageW(HWND(window as *mut _), created, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn prune() {
    let alive = |window: &isize| unsafe { IsWindow(HWND(*window as *mut _)).as_bool() };
    if let Some(entries) = ENTRIES.lock().unwrap().as_mut() {
        entries.retain(|w, _| alive(w));
    }
    if let Some(announced) = ANNOUNCED.lock().unwrap().as_mut() {
        announced.retain(alive);
    }
}

/// Tramo relleno `(x, ancho)` sobre una pista de `track` píxeles; `phase` en 0..1 mueve el indeterminado.
pub fn segment(track: i32, kind: Kind, fraction: f32, phase: f32) -> (i32, i32) {
    match kind {
        Kind::None => (0, 0),
        Kind::Indeterminate => {
            let length = track / 3;
            let start = (phase * (track + length) as f32) as i32 - length;
            (start.max(0), (start + length).min(track) - start.max(0))
        }
        // en error o pausa sin valor Explorer pinta la pista entera
        Kind::Error | Kind::Paused if fraction == 0.0 => (0, track),
        _ => (0, (track as f32 * fraction).round() as i32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_messages_follow_the_taskbar_states() {
        let mut entry = Entry::default();
        set_value(&mut entry, 27524);
        assert_eq!(entry.kind, Kind::Normal);
        assert!((entry.fraction - 0.42).abs() < 0.001);
        set_state(&mut entry, 8);
        set_value(&mut entry, 65534);
        assert_eq!((entry.kind, entry.fraction), (Kind::Paused, 1.0));
        set_state(&mut entry, 0);
        assert_eq!((entry.kind, entry.fraction), (Kind::None, 0.0));
    }

    #[test]
    fn a_group_shows_its_most_urgent_window() {
        // identificadores que no son ventanas: el mapa es global y ningún otro test los usa
        let (normal, failed) = (-101isize, -102isize);
        assert!(group(&[normal, failed]).is_none());
        assert_eq!(handle(PROGRESS_VALUE, WPARAM(normal as usize), LPARAM(32767)), Some(true));
        assert_eq!(handle(PROGRESS_VALUE, WPARAM(normal as usize), LPARAM(32767)), Some(false));
        assert_eq!(handle(PROGRESS_STATE, WPARAM(failed as usize), LPARAM(4)), Some(true));
        assert!(group(&[normal, failed]).unwrap().kind == Kind::Error);
        prune();
        assert!(group(&[normal, failed]).is_none());
        assert_eq!(handle(WM_USER + 1, WPARAM(0), LPARAM(0)), None);
    }

    #[test]
    fn the_filled_segment_stays_inside_the_track() {
        assert_eq!(segment(90, Kind::Normal, 0.5, 0.0), (0, 45));
        assert_eq!(segment(90, Kind::Error, 0.0, 0.0), (0, 90));
        assert_eq!(segment(90, Kind::Indeterminate, 0.0, 0.5), (30, 30));
        for step in 0..=20 {
            let (x, width) = segment(90, Kind::Indeterminate, 0.0, step as f32 / 20.0);
            assert!(x >= 0 && width >= 0 && x + width <= 90, "{step}: {x} {width}");
        }
    }
}
