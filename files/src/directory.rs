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
                size: metadata.as_ref().map_or(0, |m| m.len()),
                modified: metadata.and_then(|m| m.modified().ok()),
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| (!entry.is_directory, entry.name.to_lowercase()));
    Ok(entries)
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
                places.push((name.to_owned(), path));
            }
        }
        places.insert(0, ("Home".to_owned(), home));
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
    let Some(modified) = entry.modified else {
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
}
