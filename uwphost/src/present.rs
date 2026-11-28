// Ported from nickel-uwu (commit 298f87a), Copyright (c) 2026 Steven Horton, MIT. See THIRD_PARTY_NOTICES.md.
//! Pairs each application frame with its CoreWindow until it is presented, and keeps it sized.

use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

const STABLE: Duration = Duration::from_secs(1);
const DEADLINE: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Window {
    pub hwnd: usize,
    pub pid: u32,
}

#[derive(Clone)]
pub struct Frame {
    pub window: Window,
    pub app_id: String,
}

#[derive(Clone)]
pub struct Core {
    pub window: Window,
    pub app_id: String,
    pub parent: Option<usize>,
}

pub struct Snapshot {
    pub frames: Vec<Frame>,
    pub cores: Vec<Core>,
}

pub trait Platform {
    /// The CoreWindow the frame's private wrapper already owns, or 0.
    fn client(&mut self, frame: Window) -> Result<usize, String>;
    /// One step towards presentation; true once the view is fully presented.
    fn present(&mut self, frame: Window, core: Window) -> Result<bool, String>;
}

struct Pending {
    started: Instant,
    stable_since: Option<Instant>,
    core: Option<Window>,
    last_error: String,
}

impl Pending {
    fn new(now: Instant) -> Self {
        Self { started: now, stable_since: None, core: None, last_error: String::new() }
    }

    fn observe(&mut self, now: Instant, ready: bool) -> bool {
        if !ready {
            self.stable_since = None;
            return false;
        }
        now.duration_since(*self.stable_since.get_or_insert(now)) >= STABLE
    }
}

/// Explicit ownership wins over app identity: never steal a child of another frame.
fn select_core(frame: Window, app_id: &str, client: usize, remembered: Option<Window>, cores: &[Core]) -> Result<Option<Window>, String> {
    let mut eligible = cores.iter().filter(|core| core.app_id == app_id && (core.parent.is_none() || core.parent == Some(frame.hwnd)));
    if client != 0 {
        return eligible
            .find(|core| core.window.hwnd == client)
            .map(|core| Some(core.window))
            .ok_or_else(|| "wrapper client is gone or belongs to another frame".into());
    }
    if let Some(remembered) = remembered {
        return eligible
            .find(|core| core.window == remembered)
            .map(|core| Some(core.window))
            .ok_or_else(|| "pending CoreWindow is gone or belongs to another frame".into());
    }
    let mut children = eligible.filter(|core| core.parent == Some(frame.hwnd));
    let first = children.next().map(|core| core.window);
    if children.next().is_some() {
        return Err("several CoreWindows belong to the frame".into());
    }
    Ok(first)
}

/// A CoreWindow created after the host started wins over an orphan; anything ambiguous is nothing.
fn unique_core(app_id: &str, frames: &[Frame], cores: &[Core], resolved: &HashSet<Window>, preexisting: &HashSet<Window>) -> Option<Window> {
    let mut open = frames.iter().filter(|f| f.app_id == app_id && !resolved.contains(&f.window));
    open.next()?;
    if open.next().is_some() {
        return None;
    }
    let free = |core: &&Core| core.parent.is_none() && core.app_id == app_id;
    let mut new = cores.iter().filter(free).filter(|c| !preexisting.contains(&c.window));
    if let Some(core) = new.next() {
        return new.next().is_none().then_some(core.window);
    }
    let mut old = cores.iter().filter(free);
    let core = old.next()?.window;
    old.next().is_none().then_some(core)
}

#[derive(Default)]
pub struct Presenter {
    preexisting: HashSet<Window>,
    complete: HashMap<Window, Window>,
    pending: HashMap<Window, Pending>,
    failed: HashSet<Window>,
    lost: HashSet<Window>,
}

impl Presenter {
    pub fn new(preexisting: HashSet<Window>) -> Self {
        Self { preexisting, ..Self::default() }
    }

    /// Presented frame and CoreWindow pairs, for repositioning.
    pub fn presented(&self) -> impl Iterator<Item = (&Window, &Window)> {
        self.complete.iter()
    }

    /// A window appeared: frames that ran out of time get another chance.
    pub fn window_appeared(&mut self) {
        self.failed.clear();
    }

    /// Returns true while some frame still needs attention, i.e. while the timer must run.
    pub fn reconcile(&mut self, now: Instant, snapshot: &Snapshot, platform: &mut impl Platform, log: &dyn Fn(&str)) -> bool {
        let Snapshot { frames, cores } = snapshot;
        let live: HashSet<_> = frames.iter().map(|f| f.window).chain(cores.iter().map(|c| c.window)).collect();
        self.lost.extend(self.complete.iter().filter(|(_, core)| !live.contains(core)).map(|(frame, _)| *frame));
        self.complete.retain(|frame, core| live.contains(frame) && live.contains(core));
        self.lost.retain(|frame| live.contains(frame));
        self.pending.retain(|frame, _| live.contains(frame));
        self.failed.retain(|frame| live.contains(frame));
        let resolved: HashSet<_> = frames
            .iter()
            .map(|f| f.window)
            .filter(|f| self.complete.contains_key(f) || cores.iter().any(|c| c.parent == Some(f.hwnd)))
            .collect();
        for entry in frames {
            let Frame { window: frame, app_id } = entry;
            if self.complete.contains_key(frame) || self.failed.contains(frame) {
                continue;
            }
            // a closing frame outlives its CoreWindow: nothing to do until there is another one
            if self.lost.contains(frame) {
                if !cores.iter().any(|c| c.app_id == *app_id && c.parent.is_none_or(|p| p == frame.hwnd)) {
                    continue;
                }
                self.lost.remove(frame);
            }
            let pending = self.pending.entry(*frame).or_insert_with(|| Pending::new(now));
            if pending.core.is_some_and(|core| !live.contains(&core)) {
                *pending = Pending::new(now);
            }
            let preexisting = &self.preexisting;
            let result = platform.client(*frame).and_then(|client| {
                let core = select_core(*frame, app_id, client, pending.core, cores)?
                    .or_else(|| unique_core(app_id, frames, cores, &resolved, preexisting))
                    .ok_or("waiting for an unambiguous CoreWindow")?;
                if pending.core != Some(core) {
                    pending.core = Some(core);
                    pending.stable_since = None;
                }
                platform.present(*frame, core)
            });
            let elapsed = now.duration_since(pending.started);
            let ready = result.unwrap_or_else(|error| {
                if error != pending.last_error {
                    log(&format!("phase=pending aumid={app_id} frame={:#x} ms={} error={error}", frame.hwnd, elapsed.as_millis()));
                    pending.last_error = error;
                }
                false
            });
            if pending.observe(now, ready) {
                if let Some(core) = pending.core {
                    log(&format!("phase=presented aumid={app_id} frame={:#x} core={:#x} ms={}", frame.hwnd, core.hwnd, elapsed.as_millis()));
                    self.complete.insert(*frame, core);
                }
                self.pending.remove(frame);
            } else if elapsed >= DEADLINE && !ready {
                log(&format!("phase=gave-up aumid={app_id} frame={:#x} ms={} error={}", frame.hwnd, elapsed.as_millis(), pending.last_error));
                self.failed.insert(*frame);
                self.pending.remove(frame);
            }
        }
        !self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(hwnd: usize) -> Window {
        Window { hwnd, pid: 10 }
    }

    fn frame(hwnd: usize) -> Frame {
        Frame { window: window(hwnd), app_id: "app".into() }
    }

    fn core(hwnd: usize, parent: Option<usize>) -> Core {
        Core { window: window(hwnd), app_id: "app".into(), parent }
    }

    struct Fake {
        client: usize,
        result: Result<bool, String>,
        attempts: Vec<Window>,
    }

    impl Platform for Fake {
        fn client(&mut self, _: Window) -> Result<usize, String> {
            Ok(self.client)
        }
        fn present(&mut self, _: Window, core: Window) -> Result<bool, String> {
            self.attempts.push(core);
            self.result.clone()
        }
    }

    fn fake() -> Fake {
        Fake { client: 0, result: Ok(false), attempts: Vec::new() }
    }

    struct Run {
        presenter: Presenter,
        platform: Fake,
        start: Instant,
    }

    impl Run {
        fn new() -> Self {
            Self { presenter: Presenter::default(), platform: fake(), start: Instant::now() }
        }

        fn tick(&mut self, millis: u64, cores: &[Core]) -> bool {
            let snapshot = Snapshot { frames: vec![frame(1)], cores: cores.to_vec() };
            self.presenter.reconcile(self.start + Duration::from_millis(millis), &snapshot, &mut self.platform, &|_| {})
        }
    }

    #[test]
    fn presented_only_after_staying_ready_and_then_left_alone() {
        let mut run = Run::new();
        run.platform.result = Ok(true);
        for millis in [0, 250] {
            assert!(run.tick(millis, &[core(2, Some(1))]) || millis > 0);
        }
        run.platform.result = Ok(false);
        run.tick(500, &[core(2, Some(1))]);
        run.platform.result = Ok(true);
        assert!(run.tick(750, &[core(2, Some(1))]));
        assert!(run.tick(1500, &[core(2, Some(1))]));
        assert!(!run.tick(1750, &[core(2, Some(1))]), "nothing pending: the timer stops");
        let attempts = run.platform.attempts.len();
        run.tick(5000, &[core(2, Some(1))]);
        assert_eq!(run.platform.attempts.len(), attempts);
        assert_eq!(run.presenter.presented().collect::<Vec<_>>(), vec![(&window(1), &window(2))]);
    }

    #[test]
    fn gives_up_after_the_deadline_until_a_window_appears() {
        let mut run = Run::new();
        assert!(run.tick(0, &[core(2, None)]));
        assert!(!run.tick(15_000, &[core(2, None)]));
        assert!(!run.tick(16_000, &[core(2, None)]));
        assert_eq!(run.platform.attempts.len(), 2);
        run.presenter.window_appeared();
        assert!(run.tick(17_000, &[core(2, None)]));
        assert_eq!(run.platform.attempts.len(), 3);
    }

    #[test]
    fn keeps_the_pair_while_attachment_is_asynchronous() {
        let mut run = Run::new();
        run.tick(0, &[core(2, None)]);
        run.tick(250, &[core(2, Some(1)), core(3, None)]);
        assert_eq!(run.platform.attempts, vec![window(2); 2]);
    }

    #[test]
    fn a_replacement_core_restarts_the_confirmation() {
        let mut run = Run::new();
        run.platform.result = Ok(true);
        run.tick(0, &[core(2, None)]);
        run.tick(1000, &[core(3, None)]);
        assert_eq!(run.presenter.presented().count(), 0);
        run.tick(2000, &[core(3, None)]);
        assert_eq!(run.presenter.presented().collect::<Vec<_>>(), vec![(&window(1), &window(3))]);
    }

    #[test]
    fn a_closed_view_is_forgotten() {
        let mut run = Run::new();
        run.platform.result = Ok(true);
        run.tick(0, &[core(2, Some(1))]);
        run.tick(1000, &[core(2, Some(1))]);
        run.tick(2000, &[]);
        assert_eq!(run.presenter.presented().count(), 0);
    }

    #[test]
    fn a_frame_that_lost_its_core_waits_for_a_new_one() {
        let mut run = Run::new();
        run.platform.result = Ok(true);
        run.tick(0, &[core(2, Some(1))]);
        run.tick(1000, &[core(2, Some(1))]);
        let attempts = run.platform.attempts.len();
        assert!(!run.tick(2000, &[]), "the timer stops");
        assert!(!run.tick(2250, &[]));
        assert_eq!(run.platform.attempts.len(), attempts);
        assert!(run.tick(3000, &[core(3, None)]));
        assert_eq!(run.platform.attempts.last(), Some(&window(3)));
    }

    #[test]
    fn ownership_beats_identity() {
        assert_eq!(select_core(window(1), "app", 2, None, &[core(2, None), core(3, None)]), Ok(Some(window(2))));
        assert!(select_core(window(1), "app", 2, None, &[core(2, Some(3))]).is_err());
        assert!(select_core(window(1), "app", 0, Some(window(2)), &[core(2, Some(3))]).is_err());
        assert!(select_core(window(1), "app", 0, None, &[core(2, Some(1)), core(3, Some(1))]).is_err());
    }

    #[test]
    fn ambiguous_pairings_are_refused() {
        let none = HashSet::new();
        let one = [frame(1)];
        let two = [frame(1), frame(2)];
        assert_eq!(unique_core("app", &two, &[core(3, None)], &none, &none), None);
        assert_eq!(unique_core("app", &one, &[core(3, None), core(4, None)], &none, &none), None);
        assert_eq!(unique_core("app", &one, &[core(2, None), core(3, None)], &none, &HashSet::from([window(2), window(3)])), None);
        assert_eq!(unique_core("app", &one, &[core(2, None), core(3, None)], &none, &HashSet::from([window(2)])), Some(window(3)));
        assert_eq!(unique_core("app", &one, &[core(2, None)], &none, &HashSet::from([window(2)])), Some(window(2)));
        assert_eq!(unique_core("app", &two, &[core(3, None)], &HashSet::from([window(1)]), &none), Some(window(3)));
        assert_eq!(unique_core("app", &one, &[core(3, Some(9))], &none, &none), None);
    }
}
