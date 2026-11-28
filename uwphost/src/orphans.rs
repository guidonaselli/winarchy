//! Apps presented by a host that died stay alive without a view and refuse activation (0x80040904).
//! They are remembered in `state/uwphost.apps` and closed by the next host.

use crate::{
    desktop,
    present::{Snapshot, Window},
};
use std::path::{Path, PathBuf};
use windows::Win32::{
    Foundation::CloseHandle,
    System::Threading::{OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE},
};

fn file(root: &Path) -> PathBuf {
    root.join("state").join("uwphost.apps")
}

/// `pid<TAB>app_id` for every presented CoreWindow, in a stable order.
pub fn list(snapshot: &Snapshot, presented: impl Iterator<Item = Window>) -> String {
    let mut rows: Vec<String> = presented
        .filter_map(|window| snapshot.cores.iter().find(|core| core.window == window))
        .map(|core| format!("{}\t{}\n", core.window.pid, core.app_id))
        .collect();
    rows.sort();
    rows.dedup();
    rows.concat()
}

pub fn remember(root: &Path, apps: &str) -> std::io::Result<()> {
    std::fs::write(file(root), apps)
}

fn parse(text: &str) -> impl Iterator<Item = (u32, &str)> {
    text.lines().filter_map(|line| line.split_once('\t')).filter_map(|(pid, app)| Some((pid.parse().ok()?, app)))
}

pub fn close(root: &Path, log: &dyn Fn(&str)) {
    let Ok(text) = std::fs::read_to_string(file(root)) else { return };
    for (pid, app) in parse(&text) {
        // SAFETY: Query, terminate and wait on one handle, closed here: the pid cannot be reused in between.
        unsafe {
            match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid) {
                Ok(process) => {
                    // the same identity under the same pid: a reused pid never matches
                    if desktop::app_id_of(process).as_deref() == Some(app) {
                        let closed = TerminateProcess(process, 1).is_ok();
                        WaitForSingleObject(process, 2000);
                        log(&format!("phase=orphan app={app} pid={pid} closed={closed}"));
                    }
                    let _ = CloseHandle(process);
                }
                Err(error) => log(&format!("phase=orphan app={app} pid={pid} error={error}")),
            }
        }
    }
    let _ = std::fs::remove_file(file(root));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::present::Core;

    #[test]
    fn remembers_only_what_was_presented_and_reads_it_back() {
        let core = |hwnd, pid, app: &str| Core { window: Window { hwnd, pid }, app_id: app.into(), parent: None };
        let snapshot = Snapshot { frames: Vec::new(), cores: vec![core(1, 40, "b!App"), core(2, 30, "a!App"), core(3, 50, "other!App")] };
        let text = list(&snapshot, [Window { hwnd: 1, pid: 40 }, Window { hwnd: 2, pid: 30 }].into_iter());
        assert_eq!(parse(&text).collect::<Vec<_>>(), [(30, "a!App"), (40, "b!App")]);
        assert_eq!(parse("x\ty\nno-tab\n").count(), 0);
    }
}
