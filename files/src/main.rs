#![windows_subsystem = "windows"]

mod directory;
mod i18n;
mod pdf;
mod clipboard;
mod keys;
mod settings;
mod shell;
mod style;
mod theme;

use directory::Entry;
use std::{
    collections::HashSet,
    fs,
    path::{Component as PathComponent, Path, PathBuf},
};
use style::*;
use theme::Theme;
use i18n::{tr, tr_dynamic};
use windows_reactor::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum SortKey {
    Name,
    Size,
    Modified,
    Type,
}

impl SortKey {
    fn name(self) -> &'static str {
        match self {
            SortKey::Name => "name",
            SortKey::Size => "size",
            SortKey::Modified => "modified",
            SortKey::Type => "type",
        }
    }

    fn from_name(name: &str) -> Self {
        match name {
            "size" => SortKey::Size,
            "modified" => SortKey::Modified,
            "type" => SortKey::Type,
            _ => SortKey::Name,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Layout {
    Details,
    List,
    Tiles,
    Grid,
}

impl Layout {
    fn name(self) -> &'static str {
        match self {
            Layout::Details => "details",
            Layout::List => "list",
            Layout::Tiles => "tiles",
            Layout::Grid => "grid",
        }
    }

    fn from_name(name: &str) -> Self {
        match name {
            "list" => Layout::List,
            "tiles" => Layout::Tiles,
            "grid" => Layout::Grid,
            _ => Layout::Details,
        }
    }

    fn next(self) -> Self {
        match self {
            Layout::Details => Layout::List,
            Layout::List => Layout::Tiles,
            Layout::Tiles => Layout::Grid,
            Layout::Grid => Layout::Details,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Layout::Details => "Layout: details",
            Layout::List => "Layout: list",
            Layout::Tiles => "Layout: tiles",
            Layout::Grid => "Layout: grid",
        }
    }

    fn cell_width(self) -> Option<f64> {
        match self {
            Layout::Tiles => Some(TILE_CELL_WIDTH),
            Layout::Grid => Some(GRID_CELL_WIDTH),
            _ => None,
        }
    }
}

fn places_with(pinned: &[PathBuf]) -> Vec<(String, PathBuf)> {
    let mut places = directory::places();
    places.extend(pinned.iter().map(|path| {
        let label = path.file_name().map_or_else(|| path.to_string_lossy().into_owned(), |name| name.to_string_lossy().into_owned());
        (label, path.clone())
    }));
    places
}

fn extension_of(entry: &Entry) -> String {
    entry.path.extension().map(|ext| ext.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn count_word(count: usize, one: &'static str, many: &'static str) -> &'static str {
    tr(if count == 1 { one } else { many })
}

struct Command {
    label: String,
    message: Option<Message>,
    icon: Option<Symbol>,
    enabled: bool,
    primary: bool,
}

// Breadcrumb clicks return only the item text, so each one carries its position as trailing zero-width spaces.
const ZERO_WIDTH: char = '\u{200B}';

impl Explorer {
    fn crumbs(&self) -> Vec<(String, PathBuf)> {
        let mut crumbs: Vec<_> = self
            .path
            .ancestors()
            .map(|path| {
                let label = path
                    .file_name()
                    .map_or_else(|| path.to_string_lossy().into_owned(), |name| name.to_string_lossy().into_owned());
                (label, path.to_path_buf())
            })
            .collect();
        crumbs.reverse();
        crumbs
    }
}

impl Explorer {
    // The selected folder when exactly one is selected, otherwise the folder being shown.
    fn pin_target(&self) -> PathBuf {
        match self.selected.iter().next() {
            Some(path) if self.selected.len() == 1 && path.is_dir() => path.clone(),
            _ => self.path.clone(),
        }
    }
}

// Every action reachable from the command bar; primary ones show with an icon, the rest sit in the overflow menu.
impl Explorer {
    fn commands(&self) -> Vec<Command> {
        let (some, one) = (!self.selected.is_empty(), self.selected.len() == 1);
        let idle = !self.deleting && !self.transferring;
        let command = |label: &str, message: Message, icon: Option<Symbol>, enabled: bool, primary: bool| Command {
            label: tr_dynamic(label),
            message: Some(message),
            icon,
            enabled,
            primary,
        };
        let separator = || Command { label: String::new(), message: None, icon: None, enabled: true, primary: true };
        vec![
            command("Create folder", Message::CreateFolder, Some(Symbol::NewFolder), idle, true),
            command("Create file", Message::CreateFile, Some(Symbol::Add), idle, true),
            separator(),
            command("Cut", Message::Clip(true), Some(Symbol::Cut), some, true),
            command("Copy", Message::Clip(false), Some(Symbol::Copy), some, true),
            command("Paste here", Message::Paste, Some(Symbol::Paste), clipboard::has_files() && idle, true),
            separator(),
            command("Rename selected", Message::RenameSelected, Some(Symbol::Rename), one && idle, true),
            command("Delete to Recycle Bin", Message::DeleteSelected, Some(Symbol::Delete), some && idle, true),
            command("Delete permanently", Message::DeletePermanently, None, some && idle, false),
            command("Copy path", Message::CopyPath(false), None, true, false),
            command("Copy path with quotes", Message::CopyPath(true), None, true, false),
            command("Show in folder", Message::ShowInFolder, None, one, false),
            command("Paste as shortcut", Message::PasteShortcut, None, clipboard::has_files() && idle, false),
            command("Create shortcut", Message::CreateShortcut, None, one, false),
            command("Compress to zip", Message::Compress, None, some && !self.transferring, false),
            command("Extract here", Message::Extract, None, one && !self.transferring, false),
            command("Edit in Notepad", Message::RunVerb("notepad"), None, one, false),
            command("Properties", Message::RunVerb("properties"), None, one, false),
            command("Terminal here", Message::OpenTerminal, None, true, false),
            command("Terminal as administrator", Message::RunVerb("admin-terminal"), None, true, false),
            command("Git init", Message::Git("init"), None, !self.transferring, false),
            command("Git clone (URL in name field)", Message::Git("clone"), None, !self.transferring, false),
            command("Git fetch", Message::Git("fetch"), None, self.git_branch.is_some() && !self.transferring, false),
            command("Git pull", Message::Git("pull"), None, self.git_branch.is_some() && !self.transferring, false),
            command("Git push", Message::Git("push"), None, self.git_branch.is_some() && !self.transferring, false),
            command(
                if self.pinned.contains(&self.pin_target()) {
                    "Unpin from sidebar"
                } else {
                    "Pin to sidebar"
                },
                Message::TogglePin,
                None,
                true,
                false,
            ),
            command("Open in editor", Message::OpenEditor, None, true, false),
            command("Storage settings", Message::OpenStorageSense, None, true, false),
            command("Open Recycle Bin", Message::OpenRecycleBin, None, true, false),
            command("Empty Recycle Bin", Message::EmptyRecycleBin, None, true, false),
            command(if self.sort == SortKey::Type { "Group by type: on" } else { "Group by type: off" }, Message::Sort(SortKey::Type), None, true, false),
            command(self.layout.label(), Message::CycleLayout, None, true, false),
            command("Select none", Message::SelectNone, None, some, false),
            command("Invert selection", Message::InvertSelection, None, true, false),
            command(if self.hide_extensions { "Extensions: hidden" } else { "Extensions: shown" }, Message::ToggleExtensions, None, true, false),
            command(if self.show_hidden { "Hidden: shown" } else { "Hidden: off" }, Message::ToggleHidden, None, true, false),
            command(if self.recursive { "Subfolders: on" } else { "Subfolders: off" }, Message::ToggleRecursive, None, true, false),
            command("New window", Message::NewWindow, None, true, false),
            command(
                match self.language {
                    "es" => "Language: Spanish",
                    "en" => "Language: English",
                    _ => "Language: auto",
                },
                Message::CycleLanguage,
                None,
                true,
                false,
            ),
        ]
    }
}

// Only operations that can be reversed without guessing: names that did not exist before.
enum Undo {
    RemoveDir(PathBuf),
    MakeDir(PathBuf),
    RemoveFile(PathBuf),
    MakeFile(PathBuf),
    Rename { from: PathBuf, to: PathBuf },
    // (copy, original) pairs, so removing a copy can be redone by copying the original again.
    Recycle(Vec<(PathBuf, PathBuf)>),
    Copy(Vec<(PathBuf, PathBuf)>),
    Move(Vec<(PathBuf, PathBuf)>),
}

impl Undo {
    fn inverse(&self) -> Option<Undo> {
        Some(match self {
            Undo::RemoveDir(path) => Undo::MakeDir(path.clone()),
            Undo::MakeDir(path) => Undo::RemoveDir(path.clone()),
            Undo::RemoveFile(path) => Undo::MakeFile(path.clone()),
            Undo::MakeFile(path) => Undo::RemoveFile(path.clone()),
            Undo::Rename { from, to } => Undo::Rename {
                from: to.clone(),
                to: from.clone(),
            },
            Undo::Recycle(items) => Undo::Copy(
                items
                    .iter()
                    .map(|(copy, original)| Some((original.clone(), copy.parent()?.to_path_buf())))
                    .collect::<Option<_>>()?,
            ),
            Undo::Copy(items) => Undo::Recycle(
                items
                    .iter()
                    .map(|(original, folder)| Some((folder.join(original.file_name()?), original.clone())))
                    .collect::<Option<_>>()?,
            ),
            Undo::Move(items) => Undo::Move(
                items
                    .iter()
                    .map(|(current, parent)| {
                        Some((parent.join(current.file_name()?), current.parent()?.to_path_buf()))
                    })
                    .collect::<Option<_>>()?,
            ),
        })
    }

    fn run(self) -> Result<bool, String> {
        match self {
            Undo::RemoveDir(path) => fs::remove_dir(path).map(|()| true).map_err(|e| e.to_string()),
            Undo::MakeDir(path) => fs::create_dir(path).map(|()| true).map_err(|e| e.to_string()),
            Undo::RemoveFile(path) => fs::remove_file(path).map(|()| true).map_err(|e| e.to_string()),
            Undo::MakeFile(path) => fs::File::create_new(path).map(|_| true).map_err(|e| e.to_string()),
            Undo::Rename { from, to } => shell::rename(&from, &to).map(|()| true).map_err(|e| e.to_string()),
            Undo::Recycle(items) => {
                shell::recycle(&items.into_iter().map(|(copy, _)| copy).collect::<Vec<_>>())
            }
            Undo::Copy(items) => items.iter().try_fold(true, |done, (original, folder)| {
                shell::transfer(std::slice::from_ref(original), folder, false).map(|ok| done && ok)
            }),
            Undo::Move(items) => items.iter().try_fold(true, |done, (current, parent)| {
                shell::transfer(std::slice::from_ref(current), parent, true).map(|ok| done && ok)
            }),
        }
    }
}

fn undo_for_transfer(paths: &[PathBuf], destination: &Path, cut: bool) -> Option<Undo> {
    let mut created = Vec::new();
    for path in paths {
        let target = destination.join(path.file_name()?);
        if path.parent()? == destination || target.exists() {
            continue;
        }
        created.push((target, path.clone()));
    }
    if created.is_empty() {
        None
    } else if cut {
        created
            .iter()
            .map(|(target, original)| Some((target.clone(), original.parent()?.to_path_buf())))
            .collect::<Option<_>>()
            .map(Undo::Move)
    } else {
        Some(Undo::Recycle(created))
    }
}

#[derive(Clone)]
enum Message {
    Dropped(DroppedData),
    NewWindow,
    NewTab,
    Back,
    Forward,
    CreateFile,
    CopyPath(bool),
    DeletePermanently,
    ShowInFolder,
    CreateShortcut,
    OpenTerminal,
    CloseTab,
    DuplicateTab,
    ReopenTab,
    CloseOtherTabs,
    NextTab,
    SwitchTab(usize),
    ToggleRecursive,
    ToggleHidden,
    ToggleExtensions,
    Command(String),
    Crumb(String),
    Compress,
    Extract,
    ArchiveDone(Result<(), String>, Option<PathBuf>),
    RunVerb(&'static str),
    EmptyRecycleBin,
    OpenRecycleBin,
    TogglePin,
    OpenEditor,
    OpenStorageSense,
    PasteShortcut,
    Git(&'static str),
    GitBranch(PathBuf, Option<String>),
    GitDone(&'static str, Result<String, String>),
    SelectNone,
    InvertSelection,
    CycleLanguage,
    CycleLayout,
    Undo,
    Redo,
    UndoComplete(Result<bool, String>),
    Sort(SortKey),
    Clip(bool),
    Refresh,
    Focus(bool),
    Resize(WindowSize),
    Key(keys::Receiver, Option<keys::Key>),
    RetryFocus,
    Paste,
    Filter(String),
    Address(String),
    NewFolderName(String),
    CreateFolder,
    FolderCreated(PathBuf, String, Result<(), String>),
    Selection(PathBuf, bool),
    RenameSelected,
    RenameComplete(PathBuf, String, Result<(), String>),
    DeleteSelected,
    DeleteComplete(PathBuf, Result<bool, String>),
    TransferDestination(String),
    TransferSelected(bool),
    TransferComplete(bool, Result<bool, String>),
    #[cfg(windows)]
    ThemeChanged(Result<usize, String>),
    Go,
    OpenFolder(PathBuf),
    OpenFile(PathBuf),
    Parent,
    Loaded(u64, PathBuf, Result<Vec<Entry>, String>),
}

struct Explorer {
    address: String,
    tabs: Vec<PathBuf>,
    closed_tabs: Vec<PathBuf>,
    back: Vec<PathBuf>,
    forward: Vec<PathBuf>,
    active_tab: usize,
    filter: String,
    clipboard: Vec<PathBuf>,
    clipboard_cut: bool,
    pending_undo: Option<Undo>,
    undo: Vec<Undo>,
    redo: Vec<Undo>,
    pending_inverse: Option<Undo>,
    redoing: bool,
    details: String,
    git_branch: Option<String>,
    notice: Option<String>,
    preview: String,
    preview_image: Option<PathBuf>,
    show_hidden: bool,
    hide_extensions: bool,
    language: &'static str,
    layout: Layout,
    recursive: bool,
    sort: SortKey,
    descending: bool,
    new_folder_name: String,
    transfer_destination: String,
    path: PathBuf,
    load_generation: u64,
    entries: Vec<Entry>,
    selected: HashSet<PathBuf>,
    deleting: bool,
    transferring: bool,
    status: String,
    has_error: bool,
    theme: Option<Theme>,
    theme_version: u64,
    address_ref: ElementRef<TextBox>,
    focus_requested: bool,
    focus_attempts: u8,
    places: Vec<(String, PathBuf)>,
    pinned: Vec<PathBuf>,
    list_width: f64,
}

impl Component for Explorer {
    type Input = PathBuf;
    type Message = Message;

    fn create(input: &PathBuf, context: &ComponentContext<Self>) -> Self {
        let path = input.clone();
        let saved = settings::load();
        i18n::set_choice(&saved.language);
        #[cfg(windows)]
        let theme_watcher = Theme::start_watching();
        let (theme, status, has_error) = match Theme::load() {
            Ok(theme) => (theme, tr("Loading folder…").to_owned(), false),
            Err(error) => (
                None,
                format!("{}: {error}", tr("Winarchy theme could not be loaded")),
                true,
            ),
        };
        let mut app = Self {
            address: path.to_string_lossy().into_owned(),
            tabs: vec![path.clone()],
            closed_tabs: Vec::new(),
            back: Vec::new(),
            forward: Vec::new(),
            active_tab: 0,
            filter: String::new(),
            clipboard: Vec::new(),
            clipboard_cut: false,
            pending_undo: None,
            undo: Vec::new(),
            redo: Vec::new(),
            pending_inverse: None,
            redoing: false,
            details: String::new(),
            git_branch: None,
            notice: None,
            preview: String::new(),
            preview_image: None,
            show_hidden: saved.show_hidden,
            hide_extensions: saved.hide_extensions,
            language: i18n::choice(),
            layout: Layout::from_name(&saved.layout),
            recursive: false,
            sort: SortKey::from_name(&saved.sort),
            descending: saved.descending,
            new_folder_name: String::new(),
            transfer_destination: String::new(),
            path: path.clone(),
            load_generation: 0,
            entries: Vec::new(),
            selected: HashSet::new(),
            deleting: false,
            transferring: false,
            status,
            has_error,
            theme,
            theme_version: 0,
            address_ref: ElementRef::new(),
            focus_requested: false,
            focus_attempts: 0,
            places: places_with(&saved.pinned),
            pinned: saved.pinned.clone(),
            list_width: DEFAULT_LIST_WIDTH,
        };
        app.queue_load(path, context);
        match keys::install() {
            Ok(receiver) => app.watch_keys(receiver, context),
            Err(error) => {
                app.status = format!("{}: {error}", tr("Keyboard shortcuts unavailable"));
                app.has_error = true;
            }
        }
        #[cfg(windows)]
        match theme_watcher {
            Ok(handle) => app.watch_theme_handle(handle, context),
            Err(error) => {
                app.status = format!("{}: {error}", tr("Theme watch failed"));
                app.has_error = true;
            }
        }
        app
    }

    fn update(&mut self, message: Message, context: &ComponentContext<Self>) {
        match message {
            Message::Address(address) => self.address = address,
            Message::Filter(filter) => {
                self.filter = filter;
                self.refresh_status();
            }
            Message::Clip(cut) => {
                if !self.selected.is_empty() {
                    self.clipboard = self.selected.iter().cloned().collect();
                    self.clipboard_cut = cut;
                    let shared = clipboard::set_files(&self.clipboard, cut);
                    self.has_error = shared.is_err();
                    self.status = match shared {
                        Ok(()) => format!(
                            "{} {} {}",
                            self.clipboard.len(),
                            tr("items"),
                            tr(if cut { "cut" } else { "copied" })
                        ),
                        Err(error) => format!("{}: {error}", tr("Clipboard failed")),
                    };
                }
            }
            Message::Paste => self.paste(context),
            Message::Undo => self.step_history(false, context),
            Message::Redo => self.step_history(true, context),
            Message::Dropped(DroppedData::StorageItems(items)) => {
                if !(self.deleting || self.transferring) {
                    let paths = items.into_iter().map(|item| PathBuf::from(item.path)).collect();
                    self.begin_transfer(paths, false, tr("Copying dropped items…"), context);
                }
            }
            Message::Dropped(_) => {}
            Message::Back | Message::Forward => {
                let backwards = matches!(message, Message::Back);
                let (from, to) = if backwards { (&mut self.back, &mut self.forward) } else { (&mut self.forward, &mut self.back) };
                if let Some(path) = from.pop() {
                    to.push(self.path.clone());
                    self.load_path(path, context);
                }
            }
            Message::CreateFile => {
                let name = self.new_folder_name.trim().to_owned();
                if self.deleting || self.transferring {
                    return;
                }
                if !is_single_name(&name) {
                    self.status = tr("Enter a single file name").to_owned();
                    self.has_error = true;
                    return;
                }
                let file = self.path.join(&name);
                match fs::File::create_new(&file) {
                    Ok(_) => {
                        self.record(Undo::RemoveFile(file));
                        self.new_folder_name.clear();
                        self.load_path(self.path.clone(), context);
                    }
                    Err(error) => {
                        self.status = format!("{}: {error}", tr("Could not create file"));
                        self.has_error = true;
                    }
                }
            }
            Message::CopyPath(quoted) => {
                let text = if self.selected.is_empty() {
                    if quoted { format!("\"{}\"", self.path.display()) } else { self.path.to_string_lossy().into_owned() }
                } else {
                    let mut paths: Vec<_> = self
                        .selected
                        .iter()
                        .map(|p| if quoted { format!("\"{}\"", p.display()) } else { p.to_string_lossy().into_owned() })
                        .collect();
                    paths.sort();
                    paths.join("\r\n")
                };
                match clipboard::set_text(&text) {
                    Ok(()) => {
                        self.has_error = false;
                        self.status = tr("Path copied").to_owned();
                    }
                    Err(error) => {
                        self.has_error = true;
                        self.status = format!("{}: {error}", tr("Clipboard failed"));
                    }
                }
            }
            Message::TogglePin => {
                let target = self.pin_target();
                match self.pinned.iter().position(|pinned| *pinned == target) {
                    Some(index) => {
                        self.pinned.remove(index);
                    }
                    None => self.pinned.push(target),
                }
                self.places = places_with(&self.pinned);
                self.save_settings();
            }
            Message::OpenEditor => {
                use std::os::windows::process::CommandExt;
                let target = match self.selected.iter().next() {
                    Some(path) if self.selected.len() == 1 => path.clone(),
                    _ => self.path.clone(),
                };
                let opened = std::process::Command::new("cmd.exe")
                    .args(["/c", "code"])
                    .arg(&target)
                    .creation_flags(0x0800_0000)
                    .spawn();
                if let Err(error) = opened {
                    self.status = format!("{}: {error}", tr("Could not run"));
                    self.has_error = true;
                }
            }
            Message::OpenStorageSense => {
                if let Err(error) = shell::execute("open", "ms-settings:storagesense", None) {
                    self.status = format!("{}: {error}", tr("Could not run"));
                    self.has_error = true;
                }
            }
            Message::OpenTerminal => {
                let opened = std::process::Command::new("wt.exe")
                    .arg("-d")
                    .arg(&self.path)
                    .spawn()
                    .or_else(|_| {
                        std::process::Command::new("cmd.exe")
                            .args(["/c", "start", "cmd.exe"])
                            .current_dir(&self.path)
                            .spawn()
                    });
                self.has_error = opened.is_err();
                if let Err(error) = opened {
                    self.status = format!("{}: {error}", tr("Could not open terminal"));
                }
            }
            Message::NewTab => {
                self.tabs.push(self.path.clone());
                self.active_tab = self.tabs.len() - 1;
            }
            Message::CloseTab => {
                if self.tabs.len() > 1 {
                    self.closed_tabs.push(self.tabs.remove(self.active_tab));
                    self.active_tab = self.active_tab.min(self.tabs.len() - 1);
                    self.back.clear();
                    self.forward.clear();
                    self.load_path(self.tabs[self.active_tab].clone(), context);
                }
            }
            Message::DuplicateTab => {
                self.tabs.insert(self.active_tab + 1, self.path.clone());
                self.active_tab += 1;
            }
            Message::ReopenTab => {
                if let Some(path) = self.closed_tabs.pop() {
                    self.tabs.push(path.clone());
                    self.active_tab = self.tabs.len() - 1;
                    self.back.clear();
                    self.forward.clear();
                    self.load_path(path, context);
                }
            }
            Message::CloseOtherTabs => {
                let keep = self.tabs.swap_remove(self.active_tab);
                self.closed_tabs.append(&mut self.tabs);
                self.tabs.push(keep);
                self.active_tab = 0;
            }
            Message::NextTab => {
                let next = (self.active_tab + 1) % self.tabs.len();
                self.update(Message::SwitchTab(next), context);
            }
            Message::SwitchTab(index) => {
                if index < self.tabs.len() && index != self.active_tab {
                    self.active_tab = index;
                    self.back.clear();
                    self.forward.clear();
                    self.load_path(self.tabs[index].clone(), context);
                }
            }
            Message::NewWindow => {
                let folder = match self.selected.iter().collect::<Vec<_>>()[..] {
                    [path] if path.is_dir() => path.clone(),
                    _ => self.path.clone(),
                };
                let opened = std::env::current_exe()
                    .and_then(|exe| std::process::Command::new(exe).arg(&folder).spawn());
                self.has_error = opened.is_err();
                self.status = match opened {
                    Ok(_) => format!("{}: {}", tr("Opened in a new window"), folder.display()),
                    Err(error) => format!("{}: {error}", tr("New window failed")),
                };
            }
            Message::ToggleRecursive => {
                self.recursive = !self.recursive;
                self.selected.clear();
                self.entries.clear();
                self.status = tr("Loading folder…").to_owned();
                self.queue_load(self.path.clone(), context);
            }
            Message::CycleLayout => {
                self.layout = self.layout.next();
                self.save_settings();
            }
            Message::CycleLanguage => {
                let choices = i18n::choices();
                let next = choices.iter().position(|c| *c == self.language).map_or(0, |i| (i + 1) % choices.len());
                i18n::set_choice(choices[next]);
                self.language = i18n::choice();
                self.save_settings();
                self.refresh_status();
            }
            Message::Compress => {
                if self.selected.is_empty() || self.transferring {
                    return;
                }
                let mut items: Vec<_> = self.selected.iter().cloned().collect();
                items.sort();
                let stem = match &items[..] {
                    [one] => one.file_name().map(|n| n.to_string_lossy().into_owned()),
                    _ => self.path.file_name().map(|n| n.to_string_lossy().into_owned()),
                }
                .unwrap_or_else(|| "archive".to_owned());
                let archive = self.path.join(format!("{stem}.zip"));
                if archive.exists() {
                    self.status = tr("Archive already exists").to_owned();
                    self.has_error = true;
                    return;
                }
                let parent = self.path.clone();
                self.transferring = true;
                self.has_error = false;
                self.status = tr("Compress to zip").to_owned() + "…";
                let _task = context.spawn_background(move |_| {
                    let result = shell::compress(&items, &parent, &archive);
                    Message::ArchiveDone(result, Some(archive))
                });
            }
            Message::Extract => {
                let Some(archive) = self
                    .selected
                    .iter()
                    .next()
                    .filter(|path| self.selected.len() == 1 && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")))
                    .cloned()
                else {
                    return;
                };
                if self.transferring {
                    return;
                }
                let destination = archive.with_extension("");
                self.transferring = true;
                self.has_error = false;
                self.status = tr("Extract here").to_owned() + "…";
                let _task = context.spawn_background(move |_| {
                    Message::ArchiveDone(shell::extract(&archive, &destination), None)
                });
            }
            Message::ArchiveDone(result, created) => {
                self.transferring = false;
                match result {
                    Ok(()) => {
                        if let Some(archive) = created {
                            self.record(Undo::RemoveFile(archive));
                        }
                        self.load_path(self.path.clone(), context);
                    }
                    Err(error) => {
                        self.status = format!("{}: {error}", tr("Archive failed"));
                        self.has_error = true;
                    }
                }
            }
            Message::RunVerb(verb) => {
                let target = match &self.selected.iter().collect::<Vec<_>>()[..] {
                    [one] => (*one).clone(),
                    _ => self.path.clone(),
                };
                let result = match verb {
                    "notepad" => shell::execute("open", "notepad.exe", Some(&format!("\"{}\"", target.display()))),
                    "admin-terminal" => shell::execute("runas", "wt.exe", Some(&format!("-d \"{}\"", self.path.display())))
                        .or_else(|_| shell::execute("runas", "cmd.exe", Some(&format!("/k cd /d \"{}\"", self.path.display())))),
                    other => shell::execute(other, &target.to_string_lossy(), None),
                };
                self.has_error = result.is_err();
                if let Err(error) = result {
                    self.status = format!("{}: {error}", tr("Could not run"));
                }
            }
            Message::GitBranch(path, branch) => {
                if path == self.path {
                    self.git_branch = branch;
                }
            }
            Message::Git(action) => {
                if self.transferring {
                    return;
                }
                let url = self.new_folder_name.trim().to_owned();
                if action == "clone" && url.is_empty() {
                    self.status = tr("Type the repository URL in the name field").to_owned();
                    self.has_error = true;
                    return;
                }
                let dir = self.path.clone();
                self.transferring = true;
                self.has_error = false;
                self.status = format!("git {action}…");
                let _task = context.spawn_background(move |_| {
                    let result = match action {
                        "clone" => shell::git(&["clone", &url], &dir),
                        other => shell::git(&[other], &dir),
                    };
                    Message::GitDone(action, result)
                });
            }
            Message::GitDone(action, result) => {
                self.transferring = false;
                match result {
                    Ok(output) => {
                        self.load_path(self.path.clone(), context);
                        self.notice = Some(format!("git {action}: {}", output.lines().last().unwrap_or("OK")));
                    }
                    Err(error) => {
                        self.status = format!("git {action}: {}", error.lines().last().unwrap_or_default());
                        self.has_error = true;
                    }
                }
            }
            Message::OpenRecycleBin => {
                if let Err(error) = shell::execute("open", "shell:RecycleBinFolder", None) {
                    self.status = format!("{}: {error}", tr("Could not run"));
                    self.has_error = true;
                }
            }
            Message::EmptyRecycleBin => {
                if let Err(error) = shell::empty_recycle_bin() {
                    self.status = format!("{}: {error}", tr("Could not run"));
                    self.has_error = true;
                }
            }
            Message::Crumb(label) => {
                let index = label.chars().rev().take_while(|c| *c == ZERO_WIDTH).count();
                if let Some((_, path)) = self.crumbs().into_iter().nth(index) {
                    self.navigate(path, context);
                }
            }
            Message::Command(label) => {
                if let Some(command) = self.commands().into_iter().find(|c| c.label == label && c.enabled)
                    && let Some(message) = command.message
                {
                    self.update(message, context);
                }
            }
            Message::ToggleExtensions => {
                self.hide_extensions = !self.hide_extensions;
                self.save_settings();
            }
            Message::SelectNone => {
                self.selected.clear();
                self.refresh_status();
            }
            Message::InvertSelection => {
                let needle = self.filter.to_lowercase();
                self.selected = self
                    .entries
                    .iter()
                    .filter(|entry| self.matches(entry, &needle) && !self.selected.contains(&entry.path))
                    .map(|entry| entry.path.clone())
                    .collect();
                self.refresh_status();
            }
            Message::ToggleHidden => {
                self.show_hidden = !self.show_hidden;
                self.save_settings();
                self.selected.clear();
                self.refresh_status();
            }
            Message::UndoComplete(result) => {
                self.transferring = false;
                self.entries.clear();
                match result {
                    Ok(true) => {
                        if let Some(inverse) = self.pending_inverse.take() {
                            if self.redoing {
                                self.undo.push(inverse);
                            } else {
                                self.redo.push(inverse);
                            }
                        }
                        self.status = tr(if self.redoing { "Redone" } else { "Undone" }).to_owned();
                        self.has_error = false;
                    }
                    Ok(false) => {
                        self.pending_inverse = None;
                        self.status = tr("Undo canceled").to_owned();
                        self.has_error = true;
                    }
                    Err(error) => {
                        self.pending_inverse = None;
                        self.status = format!("{}: {error}", tr("Undo failed"));
                        self.has_error = true;
                    }
                }
                self.queue_load(self.path.clone(), context);
            }
            Message::Key(receiver, key) => {
                if let Some(key) = key {
                    self.watch_keys(receiver, context);
                    self.handle_key(key, context);
                }
            }
            Message::Resize(size) => self.list_width = (size.width - SIDEBAR_AND_SCROLL).max(MIN_LIST_WIDTH),
            Message::Focus(true) => {}
            Message::Focus(false) => {
                if self.focus_attempts < 10 {
                    let _task = context.spawn_background(|_| {
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        Message::RetryFocus
                    });
                }
            }
            Message::RetryFocus => self.request_address_focus(context),
            Message::Refresh => self.navigate(self.path.clone(), context),
            Message::Sort(key) => {
                self.descending = self.sort == key && !self.descending;
                self.sort = key;
                self.apply_sort();
                self.save_settings();
            }
            Message::NewFolderName(name) => self.new_folder_name = name,
            Message::CreateFolder => self.create_folder(context),
            Message::Selection(path, true) => {
                self.selected.insert(path);
                self.refresh_status();
            }
            Message::Selection(path, false) => {
                self.selected.remove(&path);
                self.refresh_status();
            }
            Message::RenameSelected => self.rename_selected(context),
            Message::DeleteSelected => self.delete_selected(false, context),
            Message::DeletePermanently => self.delete_selected(true, context),
            Message::ShowInFolder => {
                if let [path] = self.selected.iter().collect::<Vec<_>>()[..]
                    && let Some(parent) = path.parent().map(PathBuf::from)
                {
                    self.navigate(parent, context);
                }
            }
            Message::PasteShortcut => {
                let files = match clipboard::get_files() {
                    Ok(Some((files, _))) => files,
                    Ok(None) => return,
                    Err(error) => {
                        self.status = format!("{}: {error}", tr("Clipboard failed"));
                        self.has_error = true;
                        return;
                    }
                };
                let mut failure = None;
                for target in files {
                    let Some(name) = target.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
                    let link = self.path.join(format!("{name} - {}.lnk", tr("Shortcut")));
                    match shell::create_shortcut(&target, &link) {
                        Ok(()) => self.record(Undo::RemoveFile(link)),
                        Err(error) => failure = Some(format!("{}: {error}", tr("Could not create shortcut"))),
                    }
                }
                self.load_path(self.path.clone(), context);
                self.notice = failure;
            }
            Message::CreateShortcut => {
                let Some(target) = self.selected.iter().next().filter(|_| self.selected.len() == 1).cloned() else { return };
                let Some(name) = target.file_name().map(|n| n.to_string_lossy().into_owned()) else { return };
                let link = self.path.join(format!("{name} - {}.lnk", tr("Shortcut")));
                match shell::create_shortcut(&target, &link) {
                    Ok(()) => {
                        self.record(Undo::RemoveFile(link));
                        self.load_path(self.path.clone(), context);
                    }
                    Err(error) => {
                        self.status = format!("{}: {error}", tr("Could not create shortcut"));
                        self.has_error = true;
                    }
                }
            }
            Message::DeleteComplete(parent, result) => {
                self.delete_complete(parent, result, context)
            }
            Message::TransferDestination(destination) => self.transfer_destination = destination,
            Message::TransferSelected(move_items) => self.transfer_selected(move_items, context),
            Message::TransferComplete(move_items, result) => {
                self.transfer_complete(move_items, result, context)
            }
            #[cfg(windows)]
            Message::ThemeChanged(Ok(handle)) => {
                if let Err(error) = Theme::rearm_watcher(handle) {
                    Theme::close_watcher(handle);
                    self.status = format!("{}: {error}", tr("Theme watch failed"));
                    self.has_error = true;
                } else {
                    self.watch_theme_handle(handle, context);
                    match Theme::load() {
                        Ok(Some(theme)) => {
                            self.theme = Some(theme);
                            self.theme_version += 1;
                            self.has_error = false;
                            self.refresh_status();
                        }
                        Ok(None) => {
                            self.status = tr("Winarchy theme file is missing").to_owned();
                            self.has_error = true;
                        }
                        Err(error) => {
                            self.status = format!("{}: {error}", tr("Winarchy theme could not be loaded"));
                            self.has_error = true;
                        }
                    }
                }
            }
            #[cfg(windows)]
            Message::ThemeChanged(Err(error)) => {
                self.status = format!("{}: {error}", tr("Theme watch failed"));
                self.has_error = true;
            }
            Message::RenameComplete(parent, name, result) => {
                self.rename_complete(parent, name, result, context)
            }
            Message::FolderCreated(parent, name, result) => {
                self.folder_created(parent, name, result, context)
            }
            Message::Go => self.navigate(PathBuf::from(&self.address), context),
            Message::OpenFolder(path) => self.navigate(path, context),
            Message::OpenFile(path) => {
                if let Err(error) = shell::open(&path) {
                    self.status = format!("{}: {error}", path.display());
                    self.has_error = true;
                } else {
                    self.has_error = false;
                    self.refresh_status();
                }
            }
            Message::Parent => {
                if let Some(parent) = self.path.parent().map(PathBuf::from) {
                    self.navigate(parent, context);
                }
            }
            Message::Loaded(generation, path, result)
                if generation == self.load_generation && path == self.path =>
            {
                match result {
                    Ok(entries) => {
                        if !self.focus_requested {
                            self.focus_requested = true;
                            self.request_address_focus(context);
                        }
                        self.entries = entries;
                        self.apply_sort();
                        self.refresh_status();
                        if let Some(notice) = self.notice.take() {
                            self.status = notice;
                        }
                    }
                    Err(error) => {
                        self.status = error;
                        self.has_error = true;
                    }
                }
            }
            Message::Loaded(_, _, _) => {}
        }
    }

    fn view(&self, _input: &PathBuf, context: &mut ViewContext<Self>) -> View {
        context.window_title("Files — Winarchy");
        if let Some(theme) = &self.theme {
            context.window_visuals(WindowVisuals::new().theme(theme.window_theme()));
        }
        context.on_window_size(context.callback(Message::Resize));
        let address_changed = context.callback(Message::Address);
        let folder_name_changed = context.callback(Message::NewFolderName);
        let transfer_destination_changed = context.callback(Message::TransferDestination);
        let needle = self.filter.to_lowercase();
        let visible: Vec<Entry> = self
            .entries
            .iter()
            .filter(|entry| self.matches(entry, &needle))
            .cloned()
            .collect();
        let revision = self.list_revision(&needle, visible.len());
        let (selected, theme, sender, busy) = (
            self.selected.clone(),
            self.theme.clone(),
            context.sender(),
            self.deleting || self.transferring,
        );
        let width = self.list_width;
        let hide_extensions = self.hide_extensions;
        let layout = self.layout;
        let per_row = layout.cell_width().map_or(1, |cell| ((width / cell) as usize).max(1));
        let source = VirtualSource::new(
            revision,
            visible.len().div_ceil(per_row),
            |index| index,
            move |index| {
                let build = |entry: &Entry| match layout {
                    Layout::Details | Layout::List => row(
                        entry,
                        selected.contains(&entry.path),
                        theme.as_ref(),
                        &sender,
                        busy,
                        width,
                        hide_extensions,
                        layout == Layout::List,
                    ),
                    Layout::Tiles | Layout::Grid => {
                        tile(entry, selected.contains(&entry.path), theme.as_ref(), &sender, busy, layout, hide_extensions)
                    }
                };
                if per_row == 1 {
                    return build(&visible[index]);
                }
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .keyed_children(
                        visible[index * per_row..((index + 1) * per_row).min(visible.len())]
                            .iter()
                            .enumerate()
                            .map(|(slot, entry)| KeyedView::new(slot, build(entry)))
                            .collect::<Vec<_>>(),
                    )
                    .into()
            },
        );

        let tab_buttons = self.tabs.iter().enumerate().map(|(index, path)| {
            let name = path
                .file_name()
                .map_or_else(|| path.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned());
            let marker = if index == self.active_tab { "▸ " } else { "" };
            KeyedView::new(
                index,
                Button::new()
                    .automation_name(format!("{} {}", tr("Tab"), index + 1))
                    .on_click(context.message(Message::SwitchTab(index)))
                    .content(format!("{marker}{name}")),
            )
        });
        let (primary, secondary) = {
            let commands = self.commands();
            let build = |primary: bool| {
                commands
                    .iter()
                    .filter(|command| command.primary == primary)
                    .enumerate()
                    .map(|(index, command)| match (&command.message, command.icon) {
                        (None, _) => CommandBarCommand::separator(index),
                        (Some(_), Some(icon)) => {
                            match CommandBarCommand::button_with_icon(index, command.label.clone(), icon) {
                                CommandBarCommand::Button { key, label, icon, .. } => {
                                    CommandBarCommand::Button { key, label, icon, enabled: command.enabled }
                                }
                                other => other,
                            }
                        }
                        (Some(_), None) => CommandBarCommand::Button {
                            key: index.into(),
                            label: command.label.clone(),
                            icon: None,
                            enabled: command.enabled,
                        },
                    })
                    .collect::<Vec<_>>()
            };
            (build(true), build(false))
        };
        let muted = self.theme.as_ref().map(Theme::muted);
        let mut info_items = Vec::new();
        if !self.details.is_empty() {
            info_items.push(KeyedView::new(0usize, text(&self.details, muted.clone())));
        }
        if !self.preview.is_empty() {
            info_items.push(KeyedView::new(1usize, text(&self.preview, muted)));
        }
        if let Some(branch) = &self.git_branch {
            info_items.push(KeyedView::new(3usize, text(&format!("Git: {branch}"), self.theme.as_ref().map(Theme::accent))));
        }
        if let Some(image) = self.preview_image.as_ref().and_then(|path| Image::new().source_file(path).ok()) {
            info_items.push(KeyedView::new(
                2usize,
                image.stretch(Stretch::Uniform).horizontal_alignment(HorizontalAlignment::Left).height(PREVIEW_HEIGHT),
            ));
        }
        let toolbar = StackPanel::new().spacing(STACK_GAP).children((
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(ROW_GAP)
                .children((
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .spacing(ROW_GAP)
                        .keyed_children(tab_buttons),
                    Button::new()
                        .automation_name(tr("New tab"))
                        .on_click(context.message(Message::NewTab))
                        .content("+"),
                    Button::new()
                        .automation_name(tr("Close tab"))
                        .is_enabled(self.tabs.len() > 1)
                        .on_click(context.message(Message::CloseTab))
                        .content("×"),
                    Button::new()
                        .automation_name(tr("Duplicate tab"))
                        .on_click(context.message(Message::DuplicateTab))
                        .content("⧉"),
                    Button::new()
                        .automation_name(tr("Reopen closed tab"))
                        .is_enabled(!self.closed_tabs.is_empty())
                        .on_click(context.message(Message::ReopenTab))
                        .content("↶"),
                    Button::new()
                        .automation_name(tr("Close other tabs"))
                        .is_enabled(self.tabs.len() > 1)
                        .on_click(context.message(Message::CloseOtherTabs))
                        .content("⊗"),
                )),
            Grid::new()
                .columns([GridLength::Auto, GridLength::Auto, GridLength::Auto, GridLength::STAR, GridLength::Auto])
                .column_spacing(ROW_GAP)
                .children((
                    Button::new()
                        .grid_column(0)
                        .is_enabled(!self.back.is_empty())
                        .on_click(context.message(Message::Back))
                        .content(tr("Back")),
                    Button::new()
                        .grid_column(1)
                        .is_enabled(!self.forward.is_empty())
                        .on_click(context.message(Message::Forward))
                        .content(tr("Forward")),
                    Button::new()
                        .grid_column(2)
                        .on_click(context.message(Message::Parent))
                        .content(tr("Up")),
                    TextBox::new()
                        .element_ref(&self.address_ref)
                        .automation_name(tr("Address"))
                        .text(&self.address)
                        .grid_column(3)
                        .on_text_changed(address_changed),
                    Button::new()
                        .grid_column(4)
                        .on_click(context.message(Message::Go))
                        .content(tr("Go")),
                )),
            BreadcrumbBar::new()
                .items_source(
                    self.crumbs()
                        .into_iter()
                        .enumerate()
                        .map(|(index, (label, _))| format!("{label}{}", ZERO_WIDTH.to_string().repeat(index)))
                        .collect::<Vec<_>>(),
                )
                .on_item_clicked(context.callback(Message::Crumb)),
            CommandBar::new().horizontal_alignment(HorizontalAlignment::Left).owned_commands(primary, secondary, context.callback(Message::Command)),
            TextBox::new()
                .text(&self.new_folder_name)
                .automation_name(tr("Item name"))
                .placeholder_text(tr("Name for a new folder or file, or the new name"))
                .on_text_changed(folder_name_changed),
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto, GridLength::Auto])
                .column_spacing(ROW_GAP)
                .children((
                    TextBox::new()
                        .text(&self.transfer_destination)
                        .automation_name(tr("Destination folder path"))
                        .placeholder_text(tr("Copy or move the selection to this folder"))
                        .is_enabled(!self.transferring)
                        .grid_column(0)
                        .on_text_changed(transfer_destination_changed),
                    Button::new()
                        .grid_column(1)
                        .is_enabled(
                            !self.selected.is_empty() && !self.deleting && !self.transferring,
                        )
                        .on_click(context.message(Message::TransferSelected(false)))
                        .content(tr("Copy selected")),
                    Button::new()
                        .grid_column(2)
                        .is_enabled(
                            !self.selected.is_empty() && !self.deleting && !self.transferring,
                        )
                        .on_click(context.message(Message::TransferSelected(true)))
                        .content(tr("Move selected")),
                )),
            TextBox::new()
                .text(&self.filter)
                .placeholder_text(tr("Filter by name"))
                .on_text_changed(context.callback(Message::Filter)),
            Grid::new()
                .columns([
                    GridLength::Pixel(CHECK_WIDTH),
                    GridLength::STAR,
                    GridLength::Pixel(SIZE_WIDTH),
                    GridLength::Pixel(MODIFIED_WIDTH + ROW_TRAILING),
                ])
                .column_spacing(COLUMN_GAP)
                .children((
                    self.sort_button(context, SortKey::Name, tr("Name"), 1, HorizontalAlignment::Left),
                    self.sort_button(context, SortKey::Size, tr("Size"), 2, HorizontalAlignment::Right),
                    self.sort_button(context, SortKey::Modified, tr("Modified"), 3, HorizontalAlignment::Right),
                )),
            StackPanel::new().spacing(STACK_GAP).keyed_children(info_items),
            text(
                &self.status,
                self.theme.as_ref().map(|theme| {
                    if self.has_error {
                        theme.urgent()
                    } else {
                        theme.accent()
                    }
                }),
            ),
        ));
        let content = Grid::new()
            .key_accelerators(KeyAccelerators::new([KeyAccelerator::new(
                AcceleratorKey::R,
                AcceleratorModifiers::Control,
                context.message(Message::Refresh),
            )]))
            .rows([GridLength::Auto, GridLength::STAR])
            .children((
                Grid::new().grid_row(0).children((toolbar,)),
                Grid::new()
                    .grid_row(1)
                    .background_optional(self.theme.as_ref().map(Theme::surface))
                    .children((ScrollViewer::new()
                        .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
                        .content(
                            ItemsRepeater::new()
                                .horizontal_alignment(HorizontalAlignment::Stretch)
                                .virtual_source(source),
                        ),)),
            ));
        let sidebar_items = self
            .places
            .iter()
            .enumerate()
            .map(|(index, (label, path))| {
                KeyedView::new(
                    index,
                    Button::new()
                        .horizontal_content_alignment(HorizontalAlignment::Left)
                        .style(ButtonStyle::Subtle)
                        .on_click(context.message(Message::OpenFolder(path.clone())))
                        .content(text(label, self.theme.as_ref().map(Theme::foreground))),
                )
            });
        let body = Grid::new()
            .columns([GridLength::Auto, GridLength::STAR])
            .children((
                Grid::new().grid_column(0).children((ScrollViewer::new()
                    .content(StackPanel::new().spacing(TIGHT_GAP).keyed_children(sidebar_items)),)),
                Border::new()
                    .grid_column(1)
                    .background_optional(self.theme.as_ref().map(Theme::background))
                    .drop_policy(
                        DragDropPolicy::new().storage_items(
                            DragDropAction::new(DragDropOperation::Copy).caption(tr("Copy here")),
                        ),
                    )
                    .on_drop(context.callback(Message::Dropped))
                    .content(content),
            ));
        if let Some(theme) = &self.theme {
            Grid::new()
                .background(theme.background())
                .children((body,))
                .into()
        } else {
            body
        }
    }
}

impl Explorer {
    fn watch_keys(&self, receiver: keys::Receiver, context: &ComponentContext<Self>) {
        let _task = context.spawn_background(move |cancel| {
            let key = keys::next(&receiver, &cancel);
            Message::Key(receiver, key)
        });
    }

    fn handle_key(&mut self, key: keys::Key, context: &ComponentContext<Self>) {
        match key {
            keys::Key::Copy => self.update(Message::Clip(false), context),
            keys::Key::Cut => self.update(Message::Clip(true), context),
            keys::Key::Paste => self.update(Message::Paste, context),
            keys::Key::Undo => self.update(Message::Undo, context),
            keys::Key::NewWindow => self.update(Message::NewWindow, context),
            keys::Key::NewTab => self.update(Message::NewTab, context),
            keys::Key::CloseTab => self.update(Message::CloseTab, context),
            keys::Key::NextTab => self.update(Message::NextTab, context),
            keys::Key::Redo => self.update(Message::Redo, context),
            keys::Key::Delete => self.update(Message::DeleteSelected, context),
            keys::Key::Up => self.update(Message::Parent, context),
            keys::Key::Refresh => self.update(Message::Refresh, context),
            keys::Key::MoveUp | keys::Key::MoveDown => {
                let needle = self.filter.to_lowercase();
                let visible: Vec<&Entry> = self.entries.iter().filter(|entry| self.matches(entry, &needle)).collect();
                let position = |down: bool| {
                    let mut selected = visible.iter().enumerate().filter(|(_, e)| self.selected.contains(&e.path));
                    if down { selected.last() } else { selected.next() }.map(|(index, _)| index)
                };
                let down = key == keys::Key::MoveDown;
                let target = match (position(down), down) {
                    (Some(index), true) => (index + 1).min(visible.len().saturating_sub(1)),
                    (Some(index), false) => index.saturating_sub(1),
                    (None, true) => 0,
                    (None, false) => visible.len().saturating_sub(1),
                };
                if let Some(entry) = visible.get(target) {
                    self.selected = HashSet::from([entry.path.clone()]);
                    self.refresh_status();
                }
            }
            keys::Key::Open => {
                let mut chosen = self.selected.iter();
                if let (Some(path), None) = (chosen.next().cloned(), chosen.next())
                    && let Some(entry) = self.entries.iter().find(|entry| entry.path == path)
                {
                    let message = if entry.is_directory { Message::OpenFolder(path) } else { Message::OpenFile(path) };
                    self.update(message, context);
                }
            }
            keys::Key::SelectAll => {
                let needle = self.filter.to_lowercase();
                self.selected = self
                    .entries
                    .iter()
                    .filter(|entry| self.matches(entry, &needle))
                    .map(|entry| entry.path.clone())
                    .collect();
                self.refresh_status();
            }
        }
    }

    fn request_address_focus(&mut self, context: &ComponentContext<Self>) {
        self.focus_attempts += 1;
        let sender = context.sender();
        let accepted = self.address_ref.request_focus_result(move |result| {
            let _ = sender.send(Message::Focus(matches!(result, Ok(true))));
        });
        if !accepted {
            self.focus_attempts = u8::MAX;
        }
    }

    fn list_revision(&self, needle: &str, visible: usize) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (
            self.load_generation,
            self.sort,
            self.descending,
            needle,
            visible,
            self.theme_version,
            self.hide_extensions,
            self.layout,
            self.deleting || self.transferring,
            self.list_width.to_bits(),
        )
            .hash(&mut hasher);
        // order-independent so HashSet iteration order cannot change the revision
        let selection = self
            .selected
            .iter()
            .map(|path| {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                path.hash(&mut hasher);
                hasher.finish()
            })
            .fold(0u64, |acc, hash| acc ^ hash);
        (hasher.finish() ^ selection).wrapping_add(self.selected.len() as u64)
    }

    fn sort_button(
        &self,
        context: &ViewContext<Self>,
        key: SortKey,
        label: &str,
        column: i32,
        align: HorizontalAlignment,
    ) -> View {
        let arrow = match (self.sort == key, self.descending) {
            (false, _) => "",
            (true, false) => " ▲",
            (true, true) => " ▼",
        };
        Button::new()
            .grid_column(column)
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .horizontal_content_alignment(align)
            .style(ButtonStyle::Subtle)
            .on_click(context.message(Message::Sort(key)))
            .content(format!("{label}{arrow}"))
    }

    fn paste(&mut self, context: &ComponentContext<Self>) {
        if self.deleting || self.transferring {
            return;
        }
        let (paths, cut) = match clipboard::get_files() {
            Ok(Some(files)) => files,
            Ok(None) => return,
            Err(error) => {
                self.has_error = true;
                self.status = format!("{}: {error}", tr("Clipboard failed"));
                return;
            }
        };
        self.begin_transfer(paths, cut, tr("Pasting…"), context);
    }

    fn begin_transfer(
        &mut self,
        paths: Vec<PathBuf>,
        cut: bool,
        label: &str,
        context: &ComponentContext<Self>,
    ) {
        let destination = self.path.clone();
        self.pending_undo = undo_for_transfer(&paths, &destination, cut);
        self.transferring = true;
        self.has_error = false;
        self.status = label.to_owned();
        let _task = context.spawn_background(move |_| {
            let result = shell::transfer(&paths, &destination, cut);
            Message::TransferComplete(cut, result)
        });
    }

    fn record(&mut self, undo: Undo) {
        self.undo.push(undo);
        self.redo.clear();
    }

    fn step_history(&mut self, redo: bool, context: &ComponentContext<Self>) {
        if self.deleting || self.transferring {
            return;
        }
        let history = if redo { &mut self.redo } else { &mut self.undo };
        let Some(action) = history.pop() else {
            self.status = tr(if redo { "Nothing to redo" } else { "Nothing to undo" }).to_owned();
            self.has_error = false;
            return;
        };
        self.redoing = redo;
        self.pending_inverse = action.inverse();
        self.transferring = true;
        self.has_error = false;
        self.status = tr(if redo { "Redoing…" } else { "Undoing…" }).to_owned();
        let _task = context.spawn_background(move |_| Message::UndoComplete(action.run()));
    }

    fn apply_sort(&mut self) {
        let (key, descending) = (self.sort, self.descending);
        self.entries.sort_by(|a, b| {
            let order = match key {
                SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortKey::Size => a.size.cmp(&b.size),
                SortKey::Modified => a.modified.cmp(&b.modified),
                SortKey::Type => extension_of(a).cmp(&extension_of(b)).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
            };
            b.is_directory.cmp(&a.is_directory).then(if descending {
                order.reverse()
            } else {
                order
            })
        });
    }

    #[cfg(windows)]
    fn watch_theme_handle(&self, handle: usize, context: &ComponentContext<Self>) {
        let _task = context.spawn_background(move |cancel| {
            Message::ThemeChanged(Theme::wait_for_change(handle, cancel))
        });
    }

    fn save_settings(&self) {
        settings::save(&settings::Settings {
            show_hidden: self.show_hidden,
            hide_extensions: self.hide_extensions,
            sort: self.sort.name().to_owned(),
            descending: self.descending,
            language: self.language.to_owned(),
            layout: self.layout.name().to_owned(),
            pinned: self.pinned.clone(),
        });
    }

    fn matches(&self, entry: &Entry, needle: &str) -> bool {
        (self.show_hidden || !entry.hidden) && entry.name.to_lowercase().contains(needle)
    }

    fn refresh_status(&mut self) {
        if !self.has_error {
            let needle = self.filter.to_lowercase();
            let shown = self
                .entries
                .iter()
                .filter(|entry| self.matches(entry, &needle))
                .count();
            self.status = if shown == self.entries.len() {
                format!("{shown} {} · {} {}", count_word(shown, "item", "items"), self.selected.len(), count_word(self.selected.len(), "selected|one", "selected"))
            } else {
                format!(
                    "{shown} {} {} {} · {} {}",
                    tr("of"),
                    self.entries.len(),
                    tr("items"),
                    self.selected.len(),
                    count_word(self.selected.len(), "selected|one", "selected")
                )
            };
            if self.recursive && self.entries.len() >= directory::SEARCH_LIMIT {
                self.status.push_str(" · limited to the first 20000");
            }
            self.details.clear();
            self.preview.clear();
            self.preview_image = None;
            if let [path] = self.selected.iter().collect::<Vec<_>>()[..]
                && let Some(entry) = self.entries.iter().find(|entry| &entry.path == path)
            {
                self.details = directory::properties(entry);
                self.preview = if directory::is_pdf(entry) { String::new() } else { directory::preview(entry).unwrap_or_default() };
                self.preview_image = if directory::is_image(entry) {
                    Some(entry.path.clone())
                } else {
                    directory::is_pdf(entry).then(|| pdf::first_page(&entry.path)).flatten()
                };
                let kind = tr(if entry.is_directory { "Folder" } else { "File" });
                self.status.push_str(&format!(
                    " · {kind} “{}” {} {}",
                    entry.name,
                    directory::format_size(entry),
                    directory::format_modified(entry)
                ));
            }
        }
    }

    fn navigate(&mut self, path: PathBuf, context: &ComponentContext<Self>) {
        if path != self.path {
            self.back.push(self.path.clone());
            self.forward.clear();
        }
        self.load_path(path, context);
    }

    fn load_path(&mut self, path: PathBuf, context: &ComponentContext<Self>) {
        self.path = path.clone();
        self.tabs[self.active_tab] = path.clone();
        self.recursive = false;
        self.address = path.to_string_lossy().into_owned();
        self.new_folder_name.clear();
        self.selected.clear();
        self.entries.clear();
        self.git_branch = None;
        self.status = tr("Loading folder…").to_owned();
        self.has_error = false;
        self.queue_load(path, context);
    }

    fn queue_load(&mut self, path: PathBuf, context: &ComponentContext<Self>) {
        self.load_generation += 1;
        let generation = self.load_generation;
        let requested_path = path.clone();
        let path_for_git = path.clone();
        let recursive = self.recursive;
        let _task = context.spawn_background(move |_| {
            let result = if recursive {
                directory::read_recursive(&path)
            } else {
                directory::read(&path)
            }
            .map_err(|error| format!("{}: {error}", path.display()));
            Message::Loaded(generation, requested_path, result)
        });
        let branch_path = path_for_git;
        let _branch = context.spawn_background(move |_| {
            let branch = shell::git(&["branch", "--show-current"], &branch_path).ok();
            Message::GitBranch(branch_path, branch)
        });
    }

    fn delete_selected(&mut self, permanent: bool, context: &ComponentContext<Self>) {
        if self.deleting || self.transferring || self.selected.is_empty() {
            return;
        }
        let paths: Vec<_> = self.selected.iter().cloned().collect();
        let parent = self.path.clone();
        self.deleting = true;
        self.status = tr(if permanent { "Deleting permanently…" } else { "Deleting to Recycle Bin…" }).to_owned();
        self.has_error = false;
        let _task = context.spawn_background(move |_| {
            let result = shell::delete(&paths, permanent);
            Message::DeleteComplete(parent, result)
        });
    }

    fn delete_complete(
        &mut self,
        parent: PathBuf,
        result: Result<bool, String>,
        context: &ComponentContext<Self>,
    ) {
        self.deleting = false;
        if parent != self.path {
            self.refresh_status();
            return;
        }
        match result {
            Ok(true) => {
                self.selected.clear();
                self.entries.clear();
                self.status = tr("Loading folder…").to_owned();
                self.has_error = false;
                self.queue_load(parent, context);
            }
            Ok(false) => {
                self.selected.clear();
                self.entries.clear();
                self.status = tr("Loading folder…").to_owned();
                self.has_error = false;
                self.queue_load(parent, context);
            }
            Err(error) => {
                self.selected.clear();
                self.entries.clear();
                self.status = format!("{}: {error}", tr("Delete failed"));
                self.has_error = true;
                self.queue_load(parent, context);
            }
        }
    }

    fn transfer_selected(&mut self, move_items: bool, context: &ComponentContext<Self>) {
        if self.deleting || self.transferring || self.selected.is_empty() {
            return;
        }
        let destination = self.transfer_destination.trim();
        if destination.is_empty() {
            self.status = tr("Enter a destination folder path").to_owned();
            self.has_error = true;
            return;
        }

        let paths: Vec<_> = self.selected.iter().cloned().collect();
        let destination = PathBuf::from(destination);
        self.pending_undo = undo_for_transfer(&paths, &destination, move_items);
        self.transferring = true;
        self.status = if move_items {
            tr("Moving selected items…").to_owned()
        } else {
            tr("Copying selected items…").to_owned()
        };
        self.has_error = false;
        let _task = context.spawn_background(move |_| {
            let result = shell::transfer(&paths, &destination, move_items);
            Message::TransferComplete(move_items, result)
        });
    }

    fn transfer_complete(
        &mut self,
        move_items: bool,
        result: Result<bool, String>,
        context: &ComponentContext<Self>,
    ) {
        self.transferring = false;
        if move_items {
            self.selected.clear();
            self.clipboard.clear();
            clipboard::clear();
        }
        self.entries.clear();
        let undo = self.pending_undo.take();
        if let (Ok(true), Some(undo)) = (&result, undo) {
            self.record(undo);
        }
        match result {
            Ok(true) => {
                self.status = if move_items {
                    tr("Move completed").to_owned()
                } else {
                    tr("Copy completed").to_owned()
                };
                self.has_error = false;
            }
            Ok(false) => {
                self.status = tr("Operation canceled; some items may have transferred").to_owned();
                self.has_error = true;
            }
            Err(error) => {
                self.status = format!("{}: {error}", tr("File operation failed"));
                self.has_error = true;
            }
        }
        self.queue_load(self.path.clone(), context);
    }

    fn rename_complete(
        &mut self,
        parent: PathBuf,
        name: String,
        result: Result<(), String>,
        context: &ComponentContext<Self>,
    ) {
        if parent != self.path {
            return;
        }
        match result {
            Ok(()) => {
                if let Some(undo) = self.pending_undo.take() {
                    self.record(undo);
                }
                self.selected.clear();
                self.new_folder_name.clear();
                self.entries.clear();
                self.status = tr("Loading folder…").to_owned();
                self.has_error = false;
                self.queue_load(parent, context);
            }
            Err(error) => {
                self.pending_undo = None;
                self.new_folder_name = name;
                self.status = format!("{}: {error}", tr("Rename failed"));
                self.has_error = true;
            }
        }
    }

    fn folder_created(
        &mut self,
        parent: PathBuf,
        name: String,
        result: Result<(), String>,
        context: &ComponentContext<Self>,
    ) {
        if parent != self.path {
            return;
        }
        match result {
            Ok(()) => {
                self.record(Undo::RemoveDir(parent.join(&name)));
                self.entries.clear();
                self.selected.clear();
                self.status = tr("Loading folder…").to_owned();
                self.has_error = false;
                self.queue_load(parent, context);
            }
            Err(error) => {
                self.status = format!("{name}: {error}");
                self.new_folder_name = name;
                self.has_error = true;
            }
        }
    }

    fn rename_selected(&mut self, context: &ComponentContext<Self>) {
        if self.deleting || self.transferring {
            return;
        }
        let Some(source) = self
            .selected
            .iter()
            .next()
            .filter(|_| self.selected.len() == 1)
            .cloned()
        else {
            self.status = tr("Select one item to rename").to_owned();
            self.has_error = true;
            return;
        };
        let name = self.new_folder_name.trim();
        if !is_single_name(name) {
            self.status = tr("Enter a single item name").to_owned();
            self.has_error = true;
            return;
        }
        let Some(parent) = source.parent().map(Path::to_path_buf) else {
            self.status = tr("Cannot rename this item").to_owned();
            self.has_error = true;
            return;
        };
        let target = parent.join(name);
        if target == source {
            self.new_folder_name.clear();
            self.refresh_status();
            return;
        }

        let name = name.to_owned();
        self.pending_undo = Some(Undo::Rename {
            from: target.clone(),
            to: source.clone(),
        });
        self.status = tr("Renaming…").to_owned();
        self.has_error = false;
        let _task = context.spawn_background(move |_| {
            let result = shell::rename(&source, &target).map_err(|error| error.to_string());
            Message::RenameComplete(parent, name, result)
        });
    }

    fn create_folder(&mut self, context: &ComponentContext<Self>) {
        if self.deleting || self.transferring {
            return;
        }
        let name = self.new_folder_name.trim();
        if !is_single_name(name) {
            self.status = tr("Enter a single folder name").to_owned();
            self.has_error = true;
            return;
        }

        let name = name.to_owned();
        let parent = self.path.clone();
        let folder = parent.join(&name);
        self.new_folder_name.clear();
        self.status = tr("Creating folder…").to_owned();
        self.has_error = false;
        let _task = context.spawn_background(move |_| {
            let result = fs::create_dir(folder).map_err(|error| error.to_string());
            Message::FolderCreated(parent, name, result)
        });
    }
}


fn display_name(entry: &Entry, hide_extensions: bool) -> String {
    if hide_extensions && !entry.is_directory {
        Path::new(&entry.name).with_extension("").to_string_lossy().into_owned()
    } else {
        entry.name.clone()
    }
}

fn select_box(entry: &Entry, is_selected: bool, sender: &LocalSender<Message>, busy: bool) -> CheckBox {
    let selected_path = entry.path.clone();
    CheckBox::new()
        .automation_name(format!("{} {}", tr("Select"), entry.name))
        .min_width(0.0)
        .width(CHECK_WIDTH)
        .is_enabled(!busy)
        .is_checked(is_selected)
        .on_is_checked_changed(
            sender.callback(move |checked| Message::Selection(selected_path.clone(), checked)),
        )
}

fn open_button(entry: &Entry, sender: &LocalSender<Message>, column: i32, content: impl Into<View>) -> View {
    let action = if entry.is_directory {
        Message::OpenFolder(entry.path.clone())
    } else {
        Message::OpenFile(entry.path.clone())
    };
    Button::new()
        .grid_column(column)
        .horizontal_content_alignment(HorizontalAlignment::Left)
        .style(ButtonStyle::Subtle)
        .automation_name(entry.name.clone())
        .on_click(sender.message(action))
        .content(content)
}

fn selection_background(is_selected: bool, theme: Option<&Theme>) -> Option<Brush> {
    if is_selected { theme.and_then(Theme::selection) } else { None }
}

fn row(
    entry: &Entry,
    is_selected: bool,
    theme: Option<&Theme>,
    sender: &LocalSender<Message>,
    busy: bool,
    width: f64,
    hide_extensions: bool,
    compact: bool,
) -> View {
    let name = StackPanel::new()
        .horizontal_alignment(HorizontalAlignment::Left)
        .orientation(Orientation::Horizontal)
        .spacing(ROW_GAP)
        .children((
            SymbolIcon::new().symbol(directory::icon(entry)),
            text(&display_name(entry, hide_extensions), theme.map(Theme::foreground)),
        ));
    let check = select_box(entry, is_selected, sender, busy).grid_column(0);
    let button = open_button(entry, sender, 1, name);
    let grid = Grid::new()
        .width(width)
        .background_optional(selection_background(is_selected, theme))
        .column_spacing(COLUMN_GAP);
    if compact {
        return grid.columns([GridLength::Auto, GridLength::STAR]).children((check, button)).into();
    }
    grid.columns([
        GridLength::Pixel(CHECK_WIDTH),
        GridLength::STAR,
        GridLength::Pixel(SIZE_WIDTH),
        GridLength::Pixel(MODIFIED_WIDTH + ROW_TRAILING),
    ])
        .children((
            check,
            button,
            text(&directory::format_size(entry), theme.map(Theme::muted))
                .grid_column(2)
                .horizontal_alignment(HorizontalAlignment::Right)
                .vertical_alignment(VerticalAlignment::Center),
            text(&directory::format_modified(entry), theme.map(Theme::muted))
                .grid_column(3)
                .horizontal_alignment(HorizontalAlignment::Right)
                .vertical_alignment(VerticalAlignment::Center)
                .margin(Thickness::new(0.0, 0.0, ROW_TRAILING, 0.0)),
        ))
        .into()
}

// Tiles show the name with its size beside the icon; the grid stacks a larger icon over the name.
fn tile(
    entry: &Entry,
    is_selected: bool,
    theme: Option<&Theme>,
    sender: &LocalSender<Message>,
    busy: bool,
    layout: Layout,
    hide_extensions: bool,
) -> View {
    let name = text(&display_name(entry, hide_extensions), theme.map(Theme::foreground));
    let icon = SymbolIcon::new().symbol(directory::icon(entry));
    let content: View = if layout == Layout::Grid {
        StackPanel::new().spacing(TIGHT_GAP).horizontal_alignment(HorizontalAlignment::Center).children((icon, name)).into()
    } else {
        StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(ROW_GAP)
            .children((
                icon,
                StackPanel::new().children((name, text(&directory::format_size(entry), theme.map(Theme::muted)))),
            ))
            .into()
    };
    let width = if layout == Layout::Grid { GRID_CELL_WIDTH } else { TILE_CELL_WIDTH };
    Grid::new()
        .width(width)
        .background_optional(selection_background(is_selected, theme))
        .columns([GridLength::Auto, GridLength::STAR])
        .children((
            select_box(entry, is_selected, sender, busy).grid_column(0),
            open_button(entry, sender, 1, content),
        ))
        .into()
}

fn is_single_name(name: &str) -> bool {
    let mut components = Path::new(name).components();
    matches!(components.next(), Some(PathComponent::Normal(_)))
        && components.next().is_none()
        && !name.ends_with('.')
        && !name.ends_with(' ')
        && !name
            .chars()
            .any(|character| character.is_control() || r#"<>:"/\\|?*"#.contains(character))
}

fn text(value: &str, foreground: Option<Brush>) -> TextBlock {
    let block = TextBlock::new().text(value);
    foreground
        .map(|brush| block.clone().foreground(brush))
        .unwrap_or(block)
}

fn main() {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("C:\\"));
    App::run_component::<Explorer>(path).unwrap();
}

#[cfg(test)]
mod undo_tests {
    use super::*;

    #[test]
    fn undoing_a_copy_can_be_redone_by_copying_the_original_again() {
        let pair = (PathBuf::from("C:/dest/a.txt"), PathBuf::from("C:/src/a.txt"));
        let Some(Undo::Copy(copy)) = Undo::Recycle(vec![pair.clone()]).inverse() else { panic!("no redo") };
        assert_eq!(copy, [(PathBuf::from("C:/src/a.txt"), PathBuf::from("C:/dest"))]);
        let Some(Undo::Recycle(back)) = Undo::Copy(copy).inverse() else { panic!("no undo") };
        assert_eq!(back, [pair]);
    }
}
