use crate::i18n::tr;
use std::{
    fs, io,
    os::windows::fs::{FileTypeExt, MetadataExt},
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

const HIDDEN_OR_SYSTEM: u32 = 0x2 | 0x4;

static WATCH_TOKEN: AtomicU64 = AtomicU64::new(0);

// Retires every running watcher and returns the token for the next one.
pub fn next_watch_token() -> u64 {
    WATCH_TOKEN.fetch_add(1, Ordering::SeqCst) + 1
}

// Blocks until something in `path` changes, then waits for the burst to settle; false when superseded or unwatchable.
// shortcut: shares of network drives may not report changes, F5 still refreshes them.
pub fn wait_for_change(path: &std::path::Path, token: u64) -> bool {
    use windows::{
        Win32::{
            Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
            Storage::FileSystem::{FILE_NOTIFY_CHANGE_ATTRIBUTES, FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE, FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification},
            System::Threading::WaitForSingleObject,
        },
        core::HSTRING,
    };
    const FILTER: windows::Win32::Storage::FileSystem::FILE_NOTIFY_CHANGE =
        windows::Win32::Storage::FileSystem::FILE_NOTIFY_CHANGE(FILE_NOTIFY_CHANGE_FILE_NAME.0 | FILE_NOTIFY_CHANGE_DIR_NAME.0 | FILE_NOTIFY_CHANGE_ATTRIBUTES.0 | FILE_NOTIFY_CHANGE_SIZE.0 | FILE_NOTIFY_CHANGE_LAST_WRITE.0);
    const SETTLE_MS: u32 = 250;
    let Ok(handle) = (unsafe { FindFirstChangeNotificationW(&HSTRING::from(path.as_os_str()), false, FILTER) }) else { return false };
    let mut changed = false;
    loop {
        if WATCH_TOKEN.load(Ordering::SeqCst) != token {
            break;
        }
        let wait = if changed { SETTLE_MS } else { 500 };
        match unsafe { WaitForSingleObject(handle, wait) } {
            WAIT_OBJECT_0 => {
                changed = true;
                if unsafe { FindNextChangeNotification(handle) }.is_err() {
                    break;
                }
            }
            WAIT_TIMEOUT if changed => break,
            WAIT_TIMEOUT => {}
            _ => {
                changed = false;
                break;
            }
        }
    }
    let _ = unsafe { FindCloseChangeNotification(handle) };
    changed && WATCH_TOKEN.load(Ordering::SeqCst) == token
}

#[derive(Clone)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_directory: bool,
    pub hidden: bool,
    pub attributes: u32,
    pub created: Option<std::time::SystemTime>,
    pub size: u64,
    pub modified: Option<std::time::SystemTime>,
}

fn entry_of(entry: &fs::DirEntry) -> io::Result<Entry> {
    let file_type = entry.file_type()?;
    let metadata = entry.metadata().ok();
    Ok(Entry {
        name: entry.file_name().to_string_lossy().into_owned(),
        path: entry.path(),
        is_directory: file_type.is_dir() || file_type.is_symlink_dir(),
        hidden: metadata.as_ref().is_some_and(|m| m.file_attributes() & HIDDEN_OR_SYSTEM != 0),
        attributes: metadata.as_ref().map_or(0, |m| m.file_attributes()),
        created: metadata.as_ref().and_then(|m| m.created().ok()),
        size: metadata.as_ref().map_or(0, |m| m.len()),
        modified: metadata.and_then(|m| m.modified().ok()),
    })
}

pub fn entry_at(path: &std::path::Path) -> Option<Entry> {
    let metadata = fs::metadata(path).ok()?;
    Some(Entry {
        name: path.file_name().map_or_else(|| path.to_string_lossy().into_owned(), |name| name.to_string_lossy().into_owned()),
        path: path.to_path_buf(),
        is_directory: metadata.is_dir(),
        hidden: false,
        attributes: metadata.file_attributes(),
        created: metadata.created().ok(),
        size: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

// Newest first, as the shell lists them: the target of every shortcut in the user's Recent folder.
pub fn recent(limit: usize) -> Vec<Entry> {
    let Some(folder) = std::env::var_os("APPDATA").map(|appdata| PathBuf::from(appdata).join("Microsoft\\Windows\\Recent")) else {
        return Vec::new();
    };
    let Ok(read) = fs::read_dir(folder) else { return Vec::new() };
    let mut links: Vec<_> = read
        .filter_map(Result::ok)
        .filter(|link| link.path().extension().is_some_and(|extension| extension.eq_ignore_ascii_case("lnk")))
        .filter_map(|link| Some((link.metadata().ok()?.modified().ok()?, link.path())))
        .collect();
    links.sort_by(|a, b| b.0.cmp(&a.0));
    links.into_iter().filter_map(|(_, link)| entry_at(&crate::shell::link_target(&link)?)).filter(|entry| !entry.is_directory).take(limit).collect()
}

pub fn read(path: &PathBuf) -> io::Result<Vec<Entry>> {
    let mut entries = fs::read_dir(path)?.map(|entry| entry_of(&entry?)).collect::<io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| (!entry.is_directory, entry.name.to_lowercase()));
    Ok(entries)
}

// Ceiling for one recursive listing, so a whole-drive walk cannot exhaust memory.
pub const SEARCH_LIMIT: usize = 500_000;
const WALK_WORKERS: usize = 8;
const FIRST_BATCH: Duration = Duration::from_millis(150);
// Each UI refresh clones the visible list, so batches get sparser as the result grows.
const BATCH_GROWTH_PER_ENTRY: Duration = Duration::from_micros(2000);

#[derive(Clone)]
pub struct Batch {
    pub entries: Vec<Entry>,
    pub finished: bool,
    pub error: Option<String>,
}

// A parallel directory walk: workers share one folder queue and the UI drains the found entries in batches.
// Names are paths relative to the root; symlinked folders are listed but not entered, so cycles cannot loop.
pub struct Walk {
    root: PathBuf,
    queue: Mutex<Vec<PathBuf>>,
    wake: Condvar,
    busy: AtomicUsize,
    live: AtomicUsize,
    found: AtomicUsize,
    cancelled: AtomicBool,
    buffer: Mutex<(Vec<Entry>, bool, Option<String>)>,
    progress: Condvar,
}

impl Walk {
    pub fn start(root: PathBuf) -> Arc<Self> {
        let workers = std::thread::available_parallelism().map_or(4, usize::from).min(WALK_WORKERS);
        let walk = Arc::new(Self {
            queue: Mutex::new(vec![root.clone()]),
            root,
            wake: Condvar::new(),
            busy: AtomicUsize::new(0),
            live: AtomicUsize::new(workers),
            found: AtomicUsize::new(0),
            cancelled: AtomicBool::new(false),
            buffer: Mutex::new((Vec::new(), false, None)),
            progress: Condvar::new(),
        });
        for _ in 0..workers {
            let walk = Arc::clone(&walk);
            std::thread::spawn(move || walk.work());
        }
        walk
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
        self.wake.notify_all();
        self.progress.notify_all();
    }

    pub fn found(&self) -> usize {
        self.found.load(Ordering::Relaxed)
    }

    fn work(&self) {
        while let Some(folder) = self.take_folder() {
            match fs::read_dir(&folder) {
                Ok(children) => {
                    let mut subfolders = Vec::new();
                    let mut entries = Vec::new();
                    for child in children.flatten() {
                        let Ok(mut entry) = entry_of(&child) else { continue };
                        if child.file_type().is_ok_and(|kind| kind.is_dir()) {
                            subfolders.push(entry.path.clone());
                        }
                        entry.name = entry.path.strip_prefix(&self.root).map_or(entry.name, |relative| relative.to_string_lossy().into_owned());
                        entries.push(entry);
                    }
                    let total = self.found.fetch_add(entries.len(), Ordering::Relaxed) + entries.len();
                    self.buffer.lock().unwrap().0.append(&mut entries);
                    if total >= SEARCH_LIMIT {
                        self.cancelled.store(true, Ordering::Relaxed);
                    }
                    self.queue.lock().unwrap().extend(subfolders);
                }
                Err(error) if folder == self.root => self.buffer.lock().unwrap().2 = Some(format!("{}: {error}", folder.display())),
                Err(_) => {}
            }
            let _queue = self.queue.lock().unwrap();
            self.busy.fetch_sub(1, Ordering::SeqCst);
            self.wake.notify_all();
        }
        if self.live.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.buffer.lock().unwrap().1 = true;
            self.progress.notify_all();
        }
    }

    // None once the queue is empty and no worker can add more folders to it.
    fn take_folder(&self) -> Option<PathBuf> {
        let mut queue = self.queue.lock().unwrap();
        loop {
            if self.cancelled.load(Ordering::Relaxed) {
                return None;
            }
            if let Some(folder) = queue.pop() {
                self.busy.fetch_add(1, Ordering::SeqCst);
                return Some(folder);
            }
            if self.busy.load(Ordering::SeqCst) == 0 {
                self.wake.notify_all();
                return None;
            }
            queue = self.wake.wait(queue).unwrap();
        }
    }

    // Blocks until the next batch is due or the walk ends.
    pub fn next_batch(&self) -> Batch {
        let due = Instant::now() + FIRST_BATCH + BATCH_GROWTH_PER_ENTRY * (self.found() / 1000) as u32;
        let mut buffer = self.buffer.lock().unwrap();
        while !buffer.1 && !self.cancelled.load(Ordering::Relaxed) {
            let Some(left) = due.checked_duration_since(Instant::now()) else { break };
            buffer = self.progress.wait_timeout(buffer, left).unwrap().0;
        }
        let entries = std::mem::take(&mut buffer.0);
        let finished = buffer.1 || self.cancelled.load(Ordering::Relaxed);
        Batch { entries, finished, error: buffer.2.take() }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DriveKind {
    Fixed,
    Removable,
    Optical,
    Network,
}

#[derive(Clone)]
pub struct Drive {
    pub label: String,
    pub path: PathBuf,
    pub kind: DriveKind,
}

pub fn drives() -> Vec<Drive> {
    use windows::{
        Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW},
        core::PCWSTR,
    };
    let mask = unsafe { GetLogicalDrives() };
    let mut drives = Vec::new();
    for index in 0..26u8 {
        if mask & (1 << index) == 0 {
            continue;
        }
        let letter = (b'A' + index) as char;
        let root: Vec<u16> = format!("{letter}:\\").encode_utf16().chain(Some(0)).collect();
        // DRIVE_REMOVABLE = 2, DRIVE_REMOTE = 4, DRIVE_CDROM = 5
        let kind = match unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) } {
            2 => DriveKind::Removable,
            4 => DriveKind::Network,
            5 => DriveKind::Optical,
            _ => DriveKind::Fixed,
        };
        // a network share or an empty reader can block here, so only local volumes are asked for their name
        let mut name = [0u16; 64];
        let named = kind != DriveKind::Network
            && unsafe { GetVolumeInformationW(PCWSTR(root.as_ptr()), Some(&mut name), None, None, None, None) }.is_ok();
        let volume = if named { String::from_utf16_lossy(&name[..name.iter().position(|&c| c == 0).unwrap_or(0)]) } else { String::new() };
        let volume = if volume.is_empty() {
            tr(match kind {
                DriveKind::Fixed => "Local Disk",
                DriveKind::Removable => "Removable Disk",
                DriveKind::Optical => "CD/DVD Drive",
                DriveKind::Network => "Network Drive",
            })
            .to_owned()
        } else {
            volume
        };
        let label = format!("{volume} ({letter}:)");
        drives.push(Drive { label, path: PathBuf::from(format!("{letter}:{}", std::path::MAIN_SEPARATOR)), kind });
    }
    drives
}

pub fn places() -> Vec<(String, PathBuf)> {
    let mut places = Vec::new();
    if let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        for name in [
            "Desktop",
            "Documents",
            "Downloads",
            "Pictures",
            "Music",
            "Videos",
        ] {
            let path = home.join(name);
            if path.is_dir() {
                places.push((tr(name).to_owned(), path));
            }
        }
        places.insert(0, (tr("Home").to_owned(), home));
    }
    places
}

pub fn format_size(entry: &Entry) -> String {
    if entry.is_directory {
        return String::new();
    }
    let mut size = entry.size as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < 4 {
        size /= 1024.0;
        unit += 1;
    }
    match unit {
        0 => format!("{} B", entry.size),
        _ => format!("{size:.1} {}", ["", "KB", "MB", "GB", "TB"][unit]),
    }
}

pub fn format_modified(entry: &Entry) -> String {
    format_time(entry.modified)
}

pub fn attribute_names(attributes: u32) -> Vec<&'static str> {
    [(0x1, "Read-only"), (0x2, "Hidden"), (0x4, "System"), (0x20, "Archive")]
        .into_iter()
        .filter(|(bit, _)| attributes & bit != 0)
        .map(|(_, name)| name)
        .collect()
}

pub fn format_attributes(attributes: u32) -> String {
    let names: Vec<_> = attribute_names(attributes).into_iter().map(tr).collect();
    if names.is_empty() { tr("None").to_owned() } else { names.join(", ") }
}

// OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS: reading such a file makes the cloud provider download it.
const CLOUD_ONLY: u32 = 0x1000 | 0x40000 | 0x400000;

pub fn is_cloud_only(entry: &Entry) -> bool {
    entry.attributes & CLOUD_ONLY != 0
}

const PREVIEW_BYTES: u64 = 2048;
const PREVIEW_LINES: usize = 6;

// None for folders, empty files and anything that is not valid UTF-8 text without NUL bytes.
pub fn preview(entry: &Entry) -> Option<String> {
    use std::io::Read;
    if entry.is_directory || entry.size == 0 || is_cloud_only(entry) {
        return None;
    }
    let mut buffer = Vec::new();
    fs::File::open(&entry.path).ok()?.take(PREVIEW_BYTES).read_to_end(&mut buffer).ok()?;
    if buffer.contains(&0) {
        return None;
    }
    // a multibyte character may be cut at the 2 KB boundary
    let text = match std::str::from_utf8(&buffer) {
        Ok(text) => text,
        Err(error) if error.error_len().is_none() => std::str::from_utf8(&buffer[..error.valid_up_to()]).ok()?,
        Err(_) => return None,
    };
    Some(text.lines().take(PREVIEW_LINES).collect::<Vec<_>>().join("
"))
}

pub fn is_image(entry: &Entry) -> bool {
    !entry.is_directory
        && !is_cloud_only(entry)
        && entry.path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| {
            ["png", "jpg", "jpeg", "gif", "bmp", "webp", "ico"].contains(&ext.to_lowercase().as_str())
        })
}

pub fn is_pdf(entry: &Entry) -> bool {
    !entry.is_directory && !is_cloud_only(entry) && entry.path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
}

pub fn icon(entry: &Entry) -> crate::icons::Icon {
    use crate::icons::Icon;
    if entry.is_directory {
        return Icon::Folder;
    }
    let ext = entry.path.extension().and_then(|ext| ext.to_str()).unwrap_or_default().to_lowercase();
    match ext.as_str() {
        _ if is_image(entry) => Icon::FileImage,
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" => Icon::FileAudio,
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "webm" => Icon::FileVideo,
        "pdf" => Icon::FilePdf,
        "doc" | "docx" | "rtf" | "odt" => Icon::FileDoc,
        "xls" | "xlsx" | "csv" => Icon::FileXls,
        "ppt" | "pptx" => Icon::FilePpt,
        "txt" | "md" | "log" => Icon::FileText,
        "zip" | "7z" | "rar" | "tar" | "gz" => Icon::FileZip,
        "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "json" | "toml" | "yml" | "yaml" | "html" | "css" | "c" | "h" | "cpp" | "cs" | "java" | "go" | "sh" | "ps1" | "psm1" | "ahk" | "xml" | "sql" => Icon::FileCode,
        _ => Icon::File,
    }
}

pub fn properties(entry: &Entry) -> String {
    format!(
        "{}: {}
{}: {}
{}: {}",
        tr("Path"),
        entry.path.display(),
        tr("Created"),
        format_time(entry.created),
        tr("Attributes"),
        format_attributes(entry.attributes)
    )
}

fn format_time(time: Option<std::time::SystemTime>) -> String {
    let Some(secs) = time
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|seconds| seconds.as_secs() as i64)
    else {
        return String::new();
    };
    local_text(secs).unwrap_or_else(|| format!("{} UTC", utc_text(secs)))
}

#[cfg(windows)]
fn local_text(secs: i64) -> Option<String> {
    use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
    const EPOCH_GAP: i64 = 11_644_473_600;
    let ticks = u64::try_from((secs + EPOCH_GAP) * 10_000_000).ok()?;
    let file_time = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    let (mut utc, mut local) = (SYSTEMTIME::default(), SYSTEMTIME::default());
    // SAFETY: both SYSTEMTIME values outlive the calls
    unsafe {
        FileTimeToSystemTime(&file_time, &mut utc).ok()?;
        SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).ok()?;
    }
    Some(format!(
        "{}-{:02}-{:02} {:02}:{:02}",
        local.wYear, local.wMonth, local.wDay, local.wHour, local.wMinute
    ))
}

#[cfg(not(windows))]
fn local_text(_secs: i64) -> Option<String> {
    None
}

fn utc_text(secs: i64) -> String {
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year}-{month:02}-{day:02} {:02}:{:02}", rem / 3_600, rem % 3_600 / 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn entry(size: u64, secs: u64, dir: bool) -> Entry {
        Entry {
            name: String::new(),
            path: PathBuf::new(),
            is_directory: dir,
            hidden: false,
            attributes: 0,
            created: None,
            size,
            modified: Some(UNIX_EPOCH + Duration::from_secs(secs)),
        }
    }

    #[test]
    fn formats_size_and_date() {
        assert_eq!(format_size(&entry(512, 0, false)), "512 B");
        assert_eq!(format_size(&entry(1536, 0, false)), "1.5 KB");
        assert_eq!(format_size(&entry(9, 0, true)), "");
        assert_eq!(utc_text(951_782_400), "2000-02-29 00:00");
    }

    #[test]
    fn recursive_read_lists_nested_entries_with_relative_names() {
        let root = std::env::temp_dir().join(format!("wf-recursive-{}", std::process::id()));
        fs::create_dir_all(root.join("a").join("b")).unwrap();
        fs::write(root.join("top.txt"), "").unwrap();
        fs::write(root.join("a").join("b").join("deep.txt"), "").unwrap();
        let walk = Walk::start(root.clone());
        let mut names = Vec::new();
        loop {
            let batch = walk.next_batch();
            names.extend(batch.entries.into_iter().map(|entry| entry.name));
            if batch.finished {
                break;
            }
        }
        names.sort();
        fs::remove_file(root.join("top.txt")).unwrap();
        fs::remove_file(root.join("a").join("b").join("deep.txt")).unwrap();
        fs::remove_dir(root.join("a").join("b")).unwrap();
        fs::remove_dir(root.join("a")).unwrap();
        fs::remove_dir(&root).unwrap();
        let expected = [
            "a".to_owned(),
            ["a", "b"].join(std::path::MAIN_SEPARATOR_STR),
            ["a", "b", "deep.txt"].join(std::path::MAIN_SEPARATOR_STR),
            "top.txt".to_owned(),
        ];
        assert_eq!(names, expected);
    }

    #[test]
    fn lists_set_attributes_only() {
        assert!(attribute_names(0).is_empty());
        assert_eq!(attribute_names(0x1 | 0x2), ["Read-only", "Hidden"]);
        assert!(attribute_names(0x10).is_empty());
    }
}
