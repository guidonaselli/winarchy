use crate::i18n::tr;
use std::{
    fs, io,
    os::windows::fs::{FileTypeExt, MetadataExt},
    path::PathBuf,
};

const HIDDEN_OR_SYSTEM: u32 = 0x2 | 0x4;

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

pub fn read(path: &PathBuf) -> io::Result<Vec<Entry>> {
    let mut entries = fs::read_dir(path)?
        .map(|entry| {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let metadata = entry.metadata().ok();
            Ok(Entry {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: entry.path(),
                is_directory: file_type.is_dir() || file_type.is_symlink_dir(),
                hidden: metadata
                    .as_ref()
                    .is_some_and(|m| m.file_attributes() & HIDDEN_OR_SYSTEM != 0),
                attributes: metadata.as_ref().map_or(0, |m| m.file_attributes()),
                created: metadata.as_ref().and_then(|m| m.created().ok()),
                size: metadata.as_ref().map_or(0, |m| m.len()),
                modified: metadata.and_then(|m| m.modified().ok()),
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| (!entry.is_directory, entry.name.to_lowercase()));
    Ok(entries)
}

pub const SEARCH_LIMIT: usize = 20_000;

// Names are paths relative to `root`; symlinked folders are listed but not entered, so cycles cannot loop.
pub fn read_recursive(root: &PathBuf) -> io::Result<Vec<Entry>> {
    let mut found = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(folder) = pending.pop() {
        let Ok(children) = read(&folder) else {
            if &folder == root {
                return Err(io::Error::last_os_error());
            }
            continue;
        };
        for mut entry in children {
            if found.len() >= SEARCH_LIMIT {
                return Ok(found);
            }
            if entry.is_directory
                && fs::symlink_metadata(&entry.path).is_ok_and(|m| !m.file_type().is_symlink())
            {
                pending.push(entry.path.clone());
            }
            entry.name = entry
                .path
                .strip_prefix(root)
                .map_or(entry.name, |relative| relative.to_string_lossy().into_owned());
            found.push(entry);
        }
    }
    Ok(found)
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
    let mask = unsafe { windows::Win32::Storage::FileSystem::GetLogicalDrives() };
    for index in 0..26u8 {
        if mask & (1 << index) != 0 {
            let letter = (b'A' + index) as char;
            places.push((
                format!("{letter}:"),
                PathBuf::from(format!("{letter}:{}", std::path::MAIN_SEPARATOR)),
            ));
        }
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

const PREVIEW_BYTES: u64 = 2048;
const PREVIEW_LINES: usize = 6;

// None for folders, empty files and anything that is not valid UTF-8 text without NUL bytes.
pub fn preview(entry: &Entry) -> Option<String> {
    use std::io::Read;
    if entry.is_directory || entry.size == 0 {
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
        && entry.path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| {
            ["png", "jpg", "jpeg", "gif", "bmp", "webp", "ico"].contains(&ext.to_lowercase().as_str())
        })
}

pub fn is_pdf(entry: &Entry) -> bool {
    !entry.is_directory && entry.path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
}

pub fn icon(entry: &Entry) -> windows_reactor::Symbol {
    use windows_reactor::Symbol;
    if entry.is_directory {
        return Symbol::Folder;
    }
    let ext = entry.path.extension().and_then(|ext| ext.to_str()).unwrap_or_default().to_lowercase();
    match ext.as_str() {
        _ if is_image(entry) => Symbol::Pictures,
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" => Symbol::Audio,
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "webm" => Symbol::Video,
        "pdf" | "doc" | "docx" | "txt" | "md" | "rtf" | "odt" | "xls" | "xlsx" | "ppt" | "pptx" => Symbol::Document,
        "zip" | "7z" | "rar" | "tar" | "gz" => Symbol::MoveToFolder,
        _ => Symbol::Page,
    }
}

pub fn properties(entry: &Entry) -> String {
    format!(
        "{}: {} · {}: {} · {}: {}",
        tr("Path"),
        entry.path.display(),
        tr("Created"),
        format_time(entry.created),
        tr("Attributes"),
        format_attributes(entry.attributes)
    )
}

fn format_time(time: Option<std::time::SystemTime>) -> String {
    let Some(modified) = time else {
        return String::new();
    };
    let Ok(seconds) = modified.duration_since(std::time::UNIX_EPOCH) else {
        return String::new();
    };
    let secs = seconds.as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // civil-from-days (Howard Hinnant), UTC
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year}-{month:02}-{day:02} {:02}:{:02} UTC",
        rem / 3_600,
        rem % 3_600 / 60
    )
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
        assert_eq!(
            format_modified(&entry(0, 951_782_400, false)),
            "2000-02-29 00:00 UTC"
        );
    }

    #[test]
    fn recursive_read_lists_nested_entries_with_relative_names() {
        let root = std::env::temp_dir().join(format!("wf-recursive-{}", std::process::id()));
        fs::create_dir_all(root.join("a").join("b")).unwrap();
        fs::write(root.join("top.txt"), "").unwrap();
        fs::write(root.join("a").join("b").join("deep.txt"), "").unwrap();
        let mut names: Vec<_> = read_recursive(&root)
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .collect();
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
