use crate::{directory::Entry, i18n::tr_dynamic};
use std::{sync::Arc, time::SystemTime};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Group {
    None,
    Type,
    Date,
    Size,
}

impl Group {
    pub fn name(self) -> &'static str {
        match self {
            Group::None => "none",
            Group::Type => "type",
            Group::Date => "date",
            Group::Size => "size",
        }
    }

    pub fn from_name(name: &str) -> Self {
        match name {
            "type" => Group::Type,
            "date" => Group::Date,
            "size" => Group::Size,
            _ => Group::None,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Group::None => Group::Type,
            Group::Type => Group::Date,
            Group::Date => Group::Size,
            Group::Size => Group::None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Group::None => "Group: none",
            Group::Type => "Group: type",
            Group::Date => "Group: date",
            Group::Size => "Group: size",
        }
    }
}

pub enum Line {
    Header(String),
    Row(Vec<Arc<Entry>>),
}

const DAY: u64 = 86_400;
const MEGABYTE: u64 = 1024 * 1024;

// Folders always lead; the rank keeps buckets in a natural order, the label names them.
fn bucket(group: Group, entry: &Entry, now: SystemTime) -> (u8, String) {
    if entry.is_directory {
        return (0, "Folders".to_owned());
    }
    match group {
        Group::None => (1, String::new()),
        Group::Type => {
            let extension = entry.path.extension().map(|ext| ext.to_string_lossy().to_uppercase());
            match extension {
                Some(ext) => (2, format!(".{ext}")),
                None => (1, "No extension".to_owned()),
            }
        }
        Group::Date => {
            let days = entry
                .modified
                .and_then(|modified| now.duration_since(modified).ok())
                .map_or(u64::MAX, |age| age.as_secs() / DAY);
            match days {
                0 => (1, "Today".to_owned()),
                1 => (2, "Yesterday".to_owned()),
                2..=7 => (3, "Last 7 days".to_owned()),
                8..=30 => (4, "Last 30 days".to_owned()),
                31..=365 => (5, "Last year".to_owned()),
                _ => (6, "Older".to_owned()),
            }
        }
        Group::Size => match entry.size {
            0 => (1, "Empty".to_owned()),
            1..MEGABYTE => (2, "Under 1 MB".to_owned()),
            MEGABYTE..=100_000_000 => (3, "1 to 100 MB".to_owned()),
            _ => (4, "Over 100 MB".to_owned()),
        },
    }
}

// Stable: within a bucket the incoming (sorted) order is kept.
pub fn arrange(mut entries: Vec<Arc<Entry>>, group: Group, now: SystemTime) -> Vec<Arc<Entry>> {
    if group != Group::None {
        entries.sort_by_cached_key(|entry| bucket(group, entry, now));
    }
    entries
}

// `entries` must already be arranged; each group is chunked into rows of `per_row`.
pub fn lines(entries: &[Arc<Entry>], group: Group, per_row: usize, now: SystemTime) -> Vec<Line> {
    let per_row = per_row.max(1);
    let mut lines = Vec::new();
    let mut start = 0;
    while start < entries.len() {
        let label = bucket(group, &entries[start], now);
        let end = entries[start..]
            .iter()
            .position(|entry| bucket(group, entry, now) != label)
            .map_or(entries.len(), |offset| start + offset);
        if group != Group::None {
            lines.push(Line::Header(format!("{} ({})", tr_dynamic(&label.1), end - start)));
        }
        lines.extend(entries[start..end].chunks(per_row).map(|chunk| Line::Row(chunk.to_vec())));
        start = end;
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::PathBuf, time::Duration};

    fn entry(name: &str, directory: bool, size: u64, age_days: u64, now: SystemTime) -> Arc<Entry> {
        Arc::new(Entry {
            name: name.to_owned(),
            path: PathBuf::from(name),
            is_directory: directory,
            hidden: false,
            attributes: 0,
            created: None,
            size,
            modified: Some(now - Duration::from_secs(age_days * DAY)),
        })
    }

    fn names(entries: &[Arc<Entry>]) -> Vec<&str> {
        entries.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn groups_by_date_with_folders_first_and_stable_order() {
        let now = SystemTime::now();
        let entries = vec![
            entry("old.txt", false, 1, 400, now),
            entry("today.txt", false, 1, 0, now),
            entry("dir", true, 0, 400, now),
            entry("week.txt", false, 1, 3, now),
            entry("today2.txt", false, 1, 0, now),
        ];
        let arranged = arrange(entries, Group::Date, now);
        assert_eq!(names(&arranged), ["dir", "today.txt", "today2.txt", "week.txt", "old.txt"]);
        let result = lines(&arranged, Group::Date, 1, now);
        let counts: Vec<_> = result.iter().filter_map(|l| if let Line::Header(h) = l { h.rsplit(' ').next() } else { None }).collect();
        assert_eq!(counts, ["(1)", "(2)", "(1)", "(1)"]);
    }

    #[test]
    fn groups_by_size_and_type() {
        let now = SystemTime::now();
        let entries = vec![
            entry("big.bin", false, 200_000_000, 0, now),
            entry("empty.txt", false, 0, 0, now),
            entry("mid.txt", false, 5 * MEGABYTE, 0, now),
        ];
        let arranged = arrange(entries.clone(), Group::Size, now);
        assert_eq!(names(&arranged), ["empty.txt", "mid.txt", "big.bin"]);
        let by_type = arrange(entries, Group::Type, now);
        assert_eq!(names(&by_type), ["big.bin", "empty.txt", "mid.txt"]);
    }

    #[test]
    fn chunks_rows_inside_each_group_and_adds_no_headers_when_ungrouped() {
        let now = SystemTime::now();
        let entries: Vec<Arc<Entry>> = (0..5).map(|i| entry(&format!("f{i}.txt"), false, 1, 0, now)).collect();
        let ungrouped = lines(&entries, Group::None, 2, now);
        assert_eq!(ungrouped.len(), 3);
        assert!(ungrouped.iter().all(|l| matches!(l, Line::Row(_))));
        let grouped = lines(&entries, Group::Size, 2, now);
        assert_eq!(grouped.len(), 4);
    }
}
