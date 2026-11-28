//! The STA's loop: WinEvents wake it, a timer runs only while a presentation is pending.

use crate::{
    contract::{Failure, EXIT_WATCHDOG},
    desktop, orphans,
    present::{Presenter, Window},
    symbols::Symbols,
    wrapper,
};
use std::path::Path;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{HANDLE, HWND, LPARAM, WAIT_OBJECT_0, WPARAM},
    System::{
        ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::{GetCurrentProcess, TerminateProcess, INFINITE},
    },
    UI::{
        Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK},
        WindowsAndMessaging::{
            DispatchMessageW, IsWindow, KillTimer, MsgWaitForMultipleObjects, PeekMessageW, SendMessageTimeoutW, SetTimer, TranslateMessage, EVENT_OBJECT_CREATE,
            EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_SHOW, MSG, OBJID_WINDOW, PM_REMOVE, QS_ALLINPUT, SMTO_ABORTIFHUNG, WINEVENT_OUTOFCONTEXT, WM_NULL,
            WM_QUIT, WM_TIMER,
        },
    },
};

const TIMER: usize = 1;
const INTERVAL_MS: u32 = 250;
// a frame can get its AppUserModelID after its last create or show event
const SETTLE: Duration = Duration::from_secs(3);
// budget: twice the 62 MB measured for the reference host; recycled only past twice that
const RECYCLE_BYTES: usize = 2 * 2 * 62 * 1024 * 1024;

// only after serving an app and with none left: a restart then loses nothing
fn recycle(served: bool, frames_open: bool, working_set: usize) -> bool {
    served && !frames_open && working_set > RECYCLE_BYTES
}

fn working_set() -> usize {
    let mut counters = PROCESS_MEMORY_COUNTERS::default();
    // SAFETY: The size passed is that of the structure written.
    let read = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, size_of::<PROCESS_MEMORY_COUNTERS>() as u32) };
    if read.as_bool() { counters.WorkingSetSize } else { 0 }
}

// a failing relayout is retried, never forever: each miss can cost a memory scan
const BOUNDS_TRIES: u8 = 3;

#[derive(Default)]
struct Told {
    size: Option<(i32, i32)>,
    failures: u8,
}

impl Told {
    fn due(&self, size: (i32, i32)) -> bool {
        self.failures < BOUNDS_TRIES && self.size != Some(size)
    }

    fn record(&mut self, size: (i32, i32), ok: bool) {
        if ok {
            *self = Self { size: Some(size), failures: 0 };
        } else {
            self.failures += 1;
        }
    }
}

thread_local! {
    // a destroyed window has no class name left: closing is recognised by handle
    static PRESENTED: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    static APPEARED: Cell<bool> = const { Cell::new(false) };
    static MOVED: Cell<bool> = const { Cell::new(false) };
}

unsafe extern "system" fn appeared(_: HWINEVENTHOOK, _: u32, hwnd: HWND, object: i32, child: i32, _: u32, _: u32) {
    if object != OBJID_WINDOW.0 || child != 0 {
        return;
    }
    let class = desktop::class_name(hwnd);
    if class == desktop::FRAME_CLASS || class == desktop::CORE_CLASS || PRESENTED.with_borrow(|frames| frames.contains(&(hwnd.0 as usize))) {
        APPEARED.set(true);
    }
}

unsafe extern "system" fn moved(_: HWINEVENTHOOK, _: u32, hwnd: HWND, object: i32, child: i32, _: u32, _: u32) {
    if object == OBJID_WINDOW.0 && child == 0 && desktop::class_name(hwnd) == desktop::FRAME_CLASS {
        MOVED.set(true);
    }
}

struct Hook(HWINEVENTHOOK);

impl Hook {
    fn new(first: u32, last: u32, pid: u32, callback: unsafe extern "system" fn(HWINEVENTHOOK, u32, HWND, i32, i32, u32, u32)) -> Option<Self> {
        // SAFETY: Out-of-context hooks call back on this thread while it pumps messages.
        let hook = unsafe { SetWinEventHook(first, last, None, Some(callback), pid, 0, WINEVENT_OUTOFCONTEXT) };
        (!hook.is_invalid()).then(|| Self(hook))
    }
}

impl Drop for Hook {
    fn drop(&mut self) {
        // SAFETY: Unhooked by the thread that installed it.
        let _ = unsafe { UnhookWinEvent(self.0) };
    }
}

// SendMessageTimeoutW does not always set the last error: any failure against a window that still exists is a hang
fn hung(window: usize, timeout_ms: u32) -> bool {
    let window = HWND(window as *mut _);
    // SAFETY: A timed message without payload; Windows validates the handle.
    unsafe { SendMessageTimeoutW(window, WM_NULL, WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG, timeout_ms, None).0 == 0 && IsWindow(Some(window)).as_bool() }
}

/// Ends the process if this thread stops answering; the shell relaunches it.
fn watch_ourselves(window: usize) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(3));
        if hung(window, 6000) {
            // SAFETY: TerminateProcess avoids DLL detach on a stuck thread.
            let _ = unsafe { TerminateProcess(GetCurrentProcess(), u32::from(EXIT_WATCHDOG)) };
        }
    });
}

/// Returns when the parent shell exits.
pub fn run(window: HWND, parent: HANDLE, symbols: &Symbols, preexisting: HashSet<Window>, root: &Path, log: &dyn Fn(&str)) -> Result<(), Failure> {
    let mut presenter = Presenter::new(preexisting);
    let _appeared = Hook::new(EVENT_OBJECT_CREATE, EVENT_OBJECT_SHOW, 0, appeared).ok_or_else(|| Failure::Runtime("cannot watch new windows".into()))?;
    // location changes fire on every cursor move system-wide, so they are hooked per frame process
    let mut location: HashMap<u32, Hook> = HashMap::new();
    let mut ticking = false;
    let mut appeared_at = Instant::now();
    let mut served = false;
    let mut open = String::new();
    let mut placed: HashSet<Window> = HashSet::new();
    let mut told: HashMap<Window, Told> = HashMap::new();
    watch_ourselves(window.0 as usize);
    APPEARED.set(true);
    loop {
        let mut due = false;
        let mut message = MSG::default();
        // SAFETY: Standard pump of this thread's queue; WinEvent callbacks run inside PeekMessageW.
        while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            if message.message == WM_QUIT {
                return Ok(());
            }
            if message.hwnd == window && message.message == WM_TIMER && message.wParam.0 == TIMER {
                due = true;
                continue;
            }
            // SAFETY: Dispatches a message just retrieved.
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        if APPEARED.replace(false) {
            presenter.window_appeared();
            appeared_at = Instant::now();
            due = true;
        }
        if due {
            let snapshot = desktop::snapshot();
            let pending = presenter.reconcile(Instant::now(), &snapshot, &mut desktop::Native(symbols), log);
            let pending = pending || appeared_at.elapsed() < SETTLE;
            if pending != ticking {
                // SAFETY: The window belongs to this thread.
                unsafe {
                    if pending {
                        SetTimer(Some(window), TIMER, INTERVAL_MS, None);
                    } else {
                        let _ = KillTimer(Some(window), TIMER);
                    }
                }
                ticking = pending;
            }
            let pids: HashSet<u32> = presenter.presented().map(|(frame, _)| frame.pid).collect();
            served |= !pids.is_empty();
            PRESENTED.set(presenter.presented().map(|(frame, _)| frame.hwnd).collect());
            let now_open = orphans::list(&snapshot, presenter.presented().map(|(_, core)| *core));
            if now_open != open && orphans::remember(root, &now_open).is_ok() {
                open = now_open;
            }
            if !pending && recycle(served, !pids.is_empty(), working_set()) {
                return Err(Failure::Recycled("working set over budget with no app open".into()));
            }
            location.retain(|pid, _| pids.contains(pid));
            placed.retain(|frame| presenter.presented().any(|(live, _)| live == frame));
            for (frame, _) in presenter.presented() {
                if placed.insert(*frame) {
                    desktop::give_room(*frame);
                }
            }
            for pid in pids {
                if let std::collections::hash_map::Entry::Vacant(entry) = location.entry(pid) {
                    if let Some(hook) = Hook::new(EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_LOCATIONCHANGE, pid, moved) {
                        entry.insert(hook);
                    }
                }
            }
            MOVED.set(true);
        }
        if MOVED.replace(false) {
            told.retain(|frame, _| presenter.presented().any(|(live, _)| live == frame));
            for (frame, core) in presenter.presented() {
                let Some(size) = desktop::fit(*frame, *core) else { continue };
                let told = told.entry(*frame).or_default();
                if told.due(size) {
                    let result = wrapper::find(frame.hwnd, symbols).and_then(|found| wrapper::change_bounds(&found, size.0, size.1, symbols));
                    if let Err(error) = &result {
                        log(&format!("phase=bounds frame=0x{:x} error={error}", frame.hwnd));
                    }
                    told.record(size, result.is_ok());
                }
            }
        }
        // SAFETY: Waits for the parent to exit or for input on this thread.
        let woke = unsafe { MsgWaitForMultipleObjects(Some(&[parent]), false, INFINITE, QS_ALLINPUT) };
        if woke == WAIT_OBJECT_0 {
            log("phase=parent result=exited");
            return Ok(());
        }
        if woke.0 != WAIT_OBJECT_0.0 + 1 {
            return Err(Failure::Runtime("wait for the parent failed".into()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::{
        core::w,
        Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE},
    };

    #[test]
    fn recycles_only_a_bloated_host_with_nothing_open() {
        assert!(recycle(true, false, RECYCLE_BYTES + 1));
        assert!(!recycle(true, true, RECYCLE_BYTES + 1));
        assert!(!recycle(false, false, RECYCLE_BYTES + 1));
        assert!(!recycle(true, false, RECYCLE_BYTES));
        assert!(working_set() > 0);
    }

    #[test]
    fn a_failed_relayout_is_retried_a_bounded_number_of_times() {
        let mut told = Told::default();
        assert!(told.due((1, 2)));
        told.record((1, 2), true);
        assert!(!told.due((1, 2)) && told.due((3, 4)));
        for _ in 0..BOUNDS_TRIES {
            assert!(told.due((3, 4)));
            told.record((3, 4), false);
        }
        assert!(!told.due((3, 4)) && !told.due((5, 6)));
    }

    #[test]
    fn only_a_window_that_exists_and_does_not_pump_is_hung() {
        // SAFETY: A message-only window owned by this thread, which never pumps while the other one asks.
        let window = unsafe { CreateWindowExW(WINDOW_EX_STYLE(0), w!("STATIC"), w!(""), WINDOW_STYLE(0), 0, 0, 0, 0, Some(HWND_MESSAGE), None, None, None) }.unwrap();
        let raw = window.0 as usize;
        let while_busy = std::thread::spawn(move || hung(raw, 200));
        std::thread::sleep(Duration::from_millis(600));
        assert!(while_busy.join().unwrap());
        // SAFETY: Destroyed by the thread that created it.
        unsafe { DestroyWindow(window) }.unwrap();
        assert!(!hung(raw, 200));
    }
}
