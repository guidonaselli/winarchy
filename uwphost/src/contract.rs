use std::{
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, PartialEq)]
pub enum Failure {
    Symbols(String),
    Layout(String),
    ShellWindow(String),
    // transient: the shell relaunches after these
    Runtime(String),
}

pub const EXIT_OK: u8 = 0;
pub const EXIT_USAGE: u8 = 1;
pub const EXIT_ALREADY_RUNNING: u8 = 2;
pub const EXIT_SYMBOLS: u8 = 3;
pub const EXIT_LAYOUT: u8 = 4;
pub const EXIT_SHELL_WINDOW: u8 = 5;
pub const EXIT_WATCHDOG: u8 = 6;

impl Failure {
    pub fn code(&self) -> u8 {
        match self {
            Failure::Symbols(_) => EXIT_SYMBOLS,
            Failure::Layout(_) => EXIT_LAYOUT,
            Failure::ShellWindow(_) => EXIT_SHELL_WINDOW,
            Failure::Runtime(_) => EXIT_WATCHDOG,
        }
    }

    pub fn reason(&self) -> &str {
        match self {
            Failure::Symbols(reason) | Failure::Layout(reason) | Failure::ShellWindow(reason) | Failure::Runtime(reason) => reason,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Args {
    pub parent: u32,
    pub root: PathBuf,
}

pub fn parse_args(args: &[String]) -> Option<Args> {
    let (mut parent, mut root) = (None, None);
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--parent" => parent = Some(rest.next()?.parse().ok()?),
            "--root" => root = Some(PathBuf::from(rest.next()?)),
            _ => return None,
        }
    }
    Some(Args { parent: parent?, root: root? })
}

/// `state/uwphost.status` (`pid|phase|twinui_version|reason`) and `state/uwphost.log`.
#[derive(Clone)]
pub struct Report {
    state: PathBuf,
    version: String,
}

impl Report {
    pub fn new(root: &Path, version: String) -> Self {
        let state = root.join("state");
        let _ = std::fs::create_dir_all(&state);
        Self { state, version }
    }

    pub fn status(&self, phase: &str, reason: &str) {
        let temporary = self.state.join("uwphost.status.tmp");
        let reason = reason.replace(['|', '\r', '\n'], " ");
        let line = format!("{}|{phase}|{}|{reason}", std::process::id(), self.version);
        let written = std::fs::write(&temporary, line).and_then(|()| std::fs::rename(&temporary, self.state.join("uwphost.status")));
        if let Err(error) = written {
            self.log(&format!("phase=status error={error}"));
        }
    }

    pub fn log(&self, text: &str) {
        let path = self.state.join("uwphost.log");
        if std::fs::metadata(&path).is_ok_and(|m| m.len() > 64 * 1024) {
            let _ = std::fs::rename(&path, path.with_extension("log.1"));
        }
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let t = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
            let _ = writeln!(file, "t={t} pid={} {text}", std::process::id());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Option<Args> {
        parse_args(&list.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn accepts_only_the_full_contract() {
        assert_eq!(args(&["--parent", "7", "--root", "C:\\w"]), Some(Args { parent: 7, root: "C:\\w".into() }));
        for bad in [&["--parent", "7"][..], &["--root", "x"], &["--parent", "x", "--root", "y"], &["--parent", "7", "--root", "y", "--debug"]] {
            assert_eq!(args(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn each_failure_has_its_own_exit_code() {
        let codes = [
            Failure::Symbols(String::new()).code(),
            Failure::Layout(String::new()).code(),
            Failure::ShellWindow(String::new()).code(),
        ];
        assert_eq!(codes, [3, 4, 5]);
        assert_eq!([EXIT_OK, EXIT_USAGE, EXIT_ALREADY_RUNNING, EXIT_WATCHDOG], [0, 1, 2, 6]);
    }

    #[test]
    fn status_replaces_the_previous_one_without_leaving_the_temporary() {
        let root = std::env::temp_dir().join(format!("uwphost-status-{}", std::process::id()));
        let report = Report::new(&root, "10.0.1".into());
        report.status("starting", "");
        report.status("failed", "a|b\nc");
        let state = root.join("state");
        let text = std::fs::read_to_string(state.join("uwphost.status")).unwrap();
        assert_eq!(text, format!("{}|failed|10.0.1|a b c", std::process::id()));
        assert!(!state.join("uwphost.status.tmp").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
