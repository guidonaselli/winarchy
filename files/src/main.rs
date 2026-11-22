#![windows_subsystem = "windows"]

mod directory;
mod group;
mod i18n;
mod pdf;
mod clipboard;
mod iconset;
mod icons;
mod keys;
mod scroll;
mod settings;
mod shell;
mod style;
mod theme;

use directory::Entry;
use group::{Group, Line};
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
}

impl SortKey {
    fn name(self) -> &'static str {
        match self {
            SortKey::Name => "name",
            SortKey::Size => "size",
            SortKey::Modified => "modified",
        }
    }

    fn from_name(name: &str) -> Self {
        match name {
            "size" => SortKey::Size,
            "modified" => SortKey::Modified,
            _ => SortKey::Name,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Column {
    Size,
    Modified,
}

#[derive(Clone, Copy)]
struct Columns {
    size: f64,
    modified: f64,
}

impl Columns {
    const DEFAULT: Columns = Columns { size: SIZE_WIDTH, modified: MODIFIED_WIDTH };

    fn from_saved(size: f64, modified: f64) -> Self {
        let pick = |saved: f64, default: f64| if saved > 0.0 { saved.clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH) } else { default };
        Columns { size: pick(size, SIZE_WIDTH), modified: pick(modified, MODIFIED_WIDTH) }
    }

    fn get(self, column: Column) -> f64 {
        match column {
            Column::Size => self.size,
            Column::Modified => self.modified,
        }
    }

    fn set(&mut self, column: Column, width: f64) {
        let width = width.clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH);
        match column {
            Column::Size => self.size = width,
            Column::Modified => self.modified = width,
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

fn place_icon(path: &Path, pinned: bool) -> icons::Icon {
    use icons::Icon;
    if path.parent().is_none() {
        return Icon::HardDrive;
    }
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from);
    if home.as_deref() == Some(path) {
        return Icon::House;
    }
    match path.file_name().and_then(|name| name.to_str()) {
        Some("Documents") if home.as_deref() == path.parent() => Icon::FileText,
        Some("Downloads") if home.as_deref() == path.parent() => Icon::DownloadSimple,
        Some("Pictures") if home.as_deref() == path.parent() => Icon::Image,
        Some("Music") if home.as_deref() == path.parent() => Icon::MusicNotes,
        Some("Videos") if home.as_deref() == path.parent() => Icon::VideoCamera,
        Some("Desktop") if home.as_deref() == path.parent() => Icon::Desktop,
        _ if pinned => Icon::PushPin,
        _ => Icon::Folder,
    }
}

fn count_word(count: usize, one: &'static str, many: &'static str) -> &'static str {
    tr(if count == 1 { one } else { many })
}

struct Command {
    label: String,
    message: Option<Message>,
    icon: Option<icons::Icon>,
    enabled: bool,
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
        let command = |label: &str, message: Message, icon: Option<icons::Icon>, enabled: bool| Command {
            label: tr_dynamic(label),
            message: Some(message),
            icon,
            enabled,
        };
        let separator = || Command { label: String::new(), message: None, icon: None, enabled: true };
        vec![
            command("Create folder", Message::Ask(Prompt::NewFolder), Some(icons::Icon::FolderPlus), idle),
            command("Create file", Message::Ask(Prompt::NewFile), Some(icons::Icon::FilePlus), idle),
            separator(),
            command("Cut", Message::Clip(true), Some(icons::Icon::Scissors), some),
            command("Copy", Message::Clip(false), Some(icons::Icon::Copy), some),
            command("Paste here", Message::Paste, Some(icons::Icon::ClipboardText), clipboard::has_files() && idle),
            separator(),
            command("Rename selected", Message::Ask(Prompt::Rename), Some(icons::Icon::PencilSimple), one && idle),
            command("Copy to…", Message::Ask(Prompt::CopyTo), Some(icons::Icon::Copy), some && idle),
            command("Move to…", Message::Ask(Prompt::MoveTo), Some(icons::Icon::Scissors), some && idle),
            command("Delete to Recycle Bin", Message::DeleteSelected, Some(icons::Icon::Trash), some && idle),
            command("Delete permanently", Message::DeletePermanently, Some(icons::Icon::Trash), some && idle),
            command("Copy path", Message::CopyPath(false), Some(icons::Icon::LinkSimple), true),
            command("Copy path with quotes", Message::CopyPath(true), Some(icons::Icon::LinkSimple), true),
            command("Show in folder", Message::ShowInFolder, Some(icons::Icon::FolderOpen), one),
            command("Paste as shortcut", Message::PasteShortcut, Some(icons::Icon::LinkSimple), clipboard::has_files() && idle),
            command("Create shortcut", Message::CreateShortcut, Some(icons::Icon::LinkSimple), one),
            command("Compress to zip", Message::Compress, Some(icons::Icon::FileZip), some && !self.transferring),
            command("Extract here", Message::Extract, Some(icons::Icon::FileZip), one && !self.transferring && self.selected.iter().next().is_some_and(|path| path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")))),
            command("Edit in Notepad", Message::RunVerb("notepad"), Some(icons::Icon::PencilSimple), one),
            command("Share", Message::Share, Some(icons::Icon::ShareNetwork), some),
            command("Add tags", Message::Ask(Prompt::Tags), Some(icons::Icon::Tag), some),
            command("Clear tags", Message::Tag(false), Some(icons::Icon::Tag), some),
            command("Properties", Message::RunVerb("properties"), Some(icons::Icon::Info), one),
            command("Show more options", Message::NativeMenu, Some(icons::Icon::DotsThree), true),
            command("Terminal here", Message::OpenTerminal, Some(icons::Icon::TerminalWindow), true),
            command("Terminal as administrator", Message::RunVerb("admin-terminal"), Some(icons::Icon::TerminalWindow), true),
            command("Git init", Message::Git("init"), Some(icons::Icon::GitBranch), !self.transferring),
            command("Git clone", Message::Ask(Prompt::GitClone), Some(icons::Icon::GitBranch), !self.transferring),
            command("Git fetch", Message::Git("fetch"), Some(icons::Icon::GitBranch), self.git_branch.is_some() && !self.transferring),
            command("Git pull", Message::Git("pull"), Some(icons::Icon::GitBranch), self.git_branch.is_some() && !self.transferring),
            command("Git push", Message::Git("push"), Some(icons::Icon::GitBranch), self.git_branch.is_some() && !self.transferring),
            command(
                if self.pinned.contains(&self.pin_target()) {
                    "Unpin from sidebar"
                } else {
                    "Pin to sidebar"
                },
                Message::TogglePin,
                None,
                true,
            ),
            command("Open in editor", Message::OpenEditor, Some(icons::Icon::PencilSimple), true),
            command("Settings file", Message::OpenSettings, None, true),
            command("Storage settings", Message::OpenStorageSense, None, true),
            command("Open Recycle Bin", Message::OpenRecycleBin, Some(icons::Icon::Trash), true),
            command("Empty Recycle Bin", Message::EmptyRecycleBin, Some(icons::Icon::Trash), true),
            command(self.sort_label(SortKey::Name, "Sort by name"), Message::Sort(SortKey::Name), None, true),
            command(self.sort_label(SortKey::Size, "Sort by size"), Message::Sort(SortKey::Size), None, true),
            command(self.sort_label(SortKey::Modified, "Sort by date"), Message::Sort(SortKey::Modified), None, true),
            command(self.group.label(), Message::CycleGroup, None, true),
            command(if self.hide_pane { "Preview pane: off" } else { "Preview pane: on" }, Message::TogglePane, None, true),
            command(self.layout.label(), Message::CycleLayout, None, true),
            command("Reset column widths", Message::ResetColumns, None, self.layout == Layout::Details),
            command("Duplicate tab", Message::DuplicateTab, None, true),
            command("Reopen closed tab", Message::ReopenTab, None, !self.closed_tabs.is_empty()),
            command("Close other tabs", Message::CloseOtherTabs, None, self.tabs.len() > 1),
            command("Select none", Message::SelectNone, None, some),
            command("Invert selection", Message::InvertSelection, None, true),
            command(if self.hide_extensions { "Extensions: hidden" } else { "Extensions: shown" }, Message::ToggleExtensions, None, true),
            command(if self.show_hidden { "Hidden: shown" } else { "Hidden: off" }, Message::ToggleHidden, None, true),
            command(if self.recursive { "Subfolders: on" } else { "Subfolders: off" }, Message::ToggleRecursive, None, true),
            command("New window", Message::NewWindow, None, true),
            command(
                match self.language {
                    "es" => "Language: Spanish",
                    "en" => "Language: English",
                    _ => "Language: auto",
                },
                Message::CycleLanguage,
                None,
                true,
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

#[derive(Clone, Copy, Debug, PartialEq)]
enum Prompt {
    NewFolder,
    NewFile,
    Rename,
    Tags,
    GitClone,
    CopyTo,
    MoveTo,
}

impl Prompt {
    fn label(self) -> &'static str {
        match self {
            Prompt::NewFolder => "New folder name",
            Prompt::NewFile => "New file name",
            Prompt::Rename => "New name",
            Prompt::Tags => "Tags, separated by commas",
            Prompt::GitClone => "Repository URL",
            Prompt::CopyTo => "Copy to this folder",
            Prompt::MoveTo => "Move to this folder",
        }
    }

    fn uses_destination(self) -> bool {
        matches!(self, Prompt::CopyTo | Prompt::MoveTo)
    }

    fn message(self) -> Message {
        match self {
            Prompt::NewFolder => Message::CreateFolder,
            Prompt::NewFile => Message::CreateFile,
            Prompt::Rename => Message::RenameSelected,
            Prompt::Tags => Message::Tag(true),
            Prompt::GitClone => Message::Git("clone"),
            Prompt::CopyTo => Message::TransferSelected(false),
            Prompt::MoveTo => Message::TransferSelected(true),
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
    CloseTabAt(usize),
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
    OpenSettings,
    OpenStorageSense,
    PasteShortcut,
    Git(&'static str),
    GitBranch(PathBuf, Option<String>),
    GitDone(&'static str, Result<String, String>),
    SelectNone,
    InvertSelection,
    CycleLanguage,
    CycleLayout,
    CycleGroup,
    TogglePane,
    Ask(Prompt),
    Confirm,
    CancelPrompt,
    EditAddress,
    Share,
    Tag(bool),
    Undo,
    Redo,
    UndoComplete(Result<bool, String>),
    Sort(SortKey),
    Clip(bool),
    Refresh,
    Focus(bool),
    Resize(WindowSize),
    ResizeStart(Column, f64),
    ResizeDrag(f64, bool),
    ResizeEnd,
    HandleHover(Option<Column>),
    CloseHover(Option<usize>),
    SubfoldersHover(bool),
    BarAnchor(f64, f64),
    OpenDropdown(MenuKind),
    ContextItem(PathBuf, f64, f64),
    ContextBackground(f64, f64),
    CloseMenu,
    MenuCommand(String),
    SidebarStart(f64),
    SidebarDrag(f64, bool),
    SidebarEnd,
    ResetColumns,
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
    RowClick(PathBuf),
    OpenInTab(PathBuf),
    RowHover(Option<PathBuf>),
    NativeMenu,
    NativeMenuRun,
    OpenFolder(PathBuf),
    OpenFile(PathBuf),
    Parent,
    Loaded(u64, PathBuf, Result<Vec<Entry>, String>),
    Searched(u64, PathBuf, directory::Batch),
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
    group: Group,
    columns: Columns,
    hide_pane: bool,
    window_width: f64,
    window_height: f64,
    menu: Option<(f64, f64, MenuKind)>,
    bar_anchor: (f64, f64),
    last_click: Option<(PathBuf, std::time::Instant)>,
    anchor: Option<PathBuf>,
    cursor: Option<PathBuf>,
    menu_focus: Option<usize>,
    menu_closed_at: Option<std::time::Instant>,
    hovered_row: Option<PathBuf>,
    type_ahead: (String, std::time::Instant),
    filter_ref: ElementRef<TextBox>,
    hovered_handle: Option<Column>,
    hovered_close: Option<usize>,
    hovered_subfolders: bool,
    walk: Option<std::sync::Arc<directory::Walk>>,
    searching: bool,
    drag: Option<(Column, f64, f64)>,
    terminal: String,
    sidebar_width: f64,
    sidebar_drag: Option<(f64, f64)>,
    recursive: bool,
    sort: SortKey,
    descending: bool,
    new_folder_name: String,
    transfer_destination: String,
    path: PathBuf,
    load_generation: u64,
    entries: Vec<std::sync::Arc<Entry>>,
    selected: HashSet<PathBuf>,
    deleting: bool,
    transferring: bool,
    status: String,
    has_error: bool,
    theme: Option<Theme>,
    theme_version: u64,
    address_ref: ElementRef<TextBox>,
    prompt_ref: ElementRef<TextBox>,
    prompt: Option<Prompt>,
    editing_address: bool,
    focus_prompt: bool,
    focus_requested: bool,
    focus_attempts: u8,
    places: Vec<(String, PathBuf)>,
    pinned: Vec<PathBuf>,
    tags: std::collections::BTreeMap<PathBuf, Vec<String>>,
    list_width: f64,
}

impl Component for Explorer {
    type Input = PathBuf;
    type Message = Message;

    fn create(input: &PathBuf, context: &ComponentContext<Self>) -> Self {
        let saved = settings::load();
        let explicit = std::env::args_os().nth(1).is_some_and(|arg| PathBuf::from(arg).is_dir());
        let restored: Vec<PathBuf> = if explicit { Vec::new() } else { saved.tabs.iter().filter(|tab| tab.is_dir()).cloned().collect() };
        let (tabs, active_tab) = if restored.is_empty() { (vec![input.clone()], 0) } else { (restored.clone(), saved.active_tab.min(restored.len() - 1)) };
        let path = tabs[active_tab].clone();
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
            tabs,
            closed_tabs: Vec::new(),
            back: Vec::new(),
            forward: Vec::new(),
            active_tab,
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
            group: Group::from_name(&saved.group),
            columns: Columns::from_saved(saved.size_width, saved.modified_width),
            tags: saved.tags.clone(),
            drag: None,
            sidebar_width: if saved.sidebar_width > 0.0 { saved.sidebar_width.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH) } else { SIDEBAR_DEFAULT_WIDTH },
            sidebar_drag: None,
            terminal: saved.terminal.clone(),
            hide_pane: saved.hide_pane,
            window_height: 600.0,
            menu: None,
            bar_anchor: (0.0, 0.0),
            last_click: None,
            anchor: None,
            cursor: None,
            menu_focus: None,
            menu_closed_at: None,
            hovered_row: None,
            type_ahead: (String::new(), std::time::Instant::now()),
            filter_ref: ElementRef::new(),
            hovered_handle: None,
            hovered_close: None,
            hovered_subfolders: false,
            walk: None,
            searching: false,
            window_width: DEFAULT_LIST_WIDTH + SIDEBAR_DEFAULT_WIDTH + SPLITTER_WIDTH + SCROLL_ALLOWANCE + 2.0 * WINDOW_MARGIN,
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
            prompt_ref: ElementRef::new(),
            prompt: None,
            editing_address: false,
            focus_prompt: false,
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
                self.pinned = settings::load().pinned;
                let target = self.pin_target();
                match self.pinned.iter().position(|pinned| *pinned == target) {
                    Some(index) => {
                        self.pinned.remove(index);
                    }
                    None => self.pinned.push(target),
                }
                self.places = places_with(&self.pinned);
                self.write_settings(self.pinned.clone());
            }
            Message::OpenSettings => {
                self.save_settings();
                if let Some(path) = settings::location() {
                    self.update(Message::OpenFile(path), context);
                }
            }
            Message::OpenEditor => {
                use std::os::windows::process::CommandExt;
                let target = match self.selected.iter().next() {
                    Some(path) if self.selected.len() == 1 => path.clone(),
                    _ => self.path.clone(),
                };
                // code.cmd is spawned directly so Rust escapes the argument; going through cmd /c would run `&` in a file name.
                let opened = std::process::Command::new("code.cmd")
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
                let mut custom = self.terminal.split_whitespace();
                let opened = match custom.next() {
                    Some(program) => std::process::Command::new(program)
                        .args(custom.map(|arg| arg.replace("{path}", &self.path.to_string_lossy())))
                        .current_dir(&self.path)
                        .spawn(),
                    None => std::process::Command::new("wt.exe").arg("-d").arg(&self.path).spawn(),
                }
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
                self.save_settings();
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
            Message::CloseTabAt(index) => {
                if self.tabs.len() > 1 && index < self.tabs.len() {
                    self.update(Message::SwitchTab(index), context);
                    self.update(Message::CloseTab, context);
                }
            }
            Message::DuplicateTab => {
                self.tabs.insert(self.active_tab + 1, self.path.clone());
                self.active_tab += 1;
                self.save_settings();
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
                self.save_settings();
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
            Message::Tag(add) => {
                let names: Vec<String> = self.new_folder_name.split(',').map(|tag| tag.trim().to_owned()).filter(|tag| !tag.is_empty()).collect();
                if add && names.is_empty() {
                    self.status = tr("Type at least one tag").to_owned();
                    return;
                }
                for path in &self.selected {
                    if add {
                        let current = self.tags.entry(path.clone()).or_default();
                        for name in &names {
                            if !current.iter().any(|tag| tag.eq_ignore_ascii_case(name)) {
                                current.push(name.clone());
                            }
                        }
                    } else {
                        self.tags.remove(path);
                    }
                }
                if add {
                    self.new_folder_name.clear();
                }
                self.save_settings();
                self.refresh_status();
            }
            Message::Share => {
                let files: Vec<PathBuf> = self.selected.iter().filter(|path| path.is_file()).cloned().collect();
                if files.is_empty() {
                    self.status = tr("Select files to share").to_owned();
                } else if let Err(error) = shell::share(&files) {
                    self.status = format!("{}: {error}", tr("Could not share"));
                    self.has_error = true;
                }
            }
            Message::CycleGroup => {
                self.group = self.group.next();
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
                if action == "clone" && (url.is_empty() || url.starts_with('-')) {
                    self.status = tr("Type the repository URL").to_owned();
                    self.has_error = true;
                    return;
                }
                let dir = self.path.clone();
                self.transferring = true;
                self.has_error = false;
                self.status = format!("git {action}…");
                let _task = context.spawn_background(move |_| {
                    let result = match action {
                        "clone" => shell::git(&["clone", "--", &url], &dir),
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
            Message::ResizeStart(column, x) => self.drag = Some((column, x, self.columns.get(column))),
            Message::ResizeDrag(x, pressed) => {
                if let (Some((column, start_x, start_width)), true) = (self.drag, pressed) {
                    self.columns.set(column, start_width + start_x - x);
                }
            }
            Message::HandleHover(column) => self.hovered_handle = column,
            Message::CloseHover(index) => self.hovered_close = index,
            Message::SubfoldersHover(hovered) => self.hovered_subfolders = hovered,
            Message::ResizeEnd => {
                if self.drag.take().is_some() {
                    self.save_settings();
                }
            }
            Message::ContextItem(path, x, y) => {
                self.menu_focus = None;
                if !self.selected.contains(&path) {
                    self.selected = HashSet::from([path.clone()]);
                    self.refresh_status();
                }
                self.menu = Some((x, y, MenuKind::Item));
            }
            Message::ContextBackground(x, y) => {
                if self.menu.is_none() {
                    self.menu_focus = None;
                    self.selected.clear();
                    self.refresh_status();
                    self.menu = Some((x, y, MenuKind::Background));
                }
            }
            // A button swallows pointer presses, so the bar remembers where the pointer entered it to anchor the dropdown.
            Message::BarAnchor(x, y) => self.bar_anchor = (x, y),
            Message::OpenDropdown(kind) => {
                self.menu_focus = None;
                let (x, y) = self.bar_anchor;
                self.menu = if self.menu.as_ref().is_some_and(|(_, _, open)| *open == kind) { None } else { Some((x, y + BAR_BUTTON_HEIGHT, kind)) };
            }
            Message::CloseMenu => self.close_menu(),
            Message::MenuCommand(label) => {
                self.close_menu();
                self.update(Message::Command(label), context);
            }
            Message::SidebarStart(x) => self.sidebar_drag = Some((x, self.sidebar_width)),
            Message::SidebarDrag(x, pressed) => {
                if let (Some((start_x, start_width)), true) = (self.sidebar_drag, pressed) {
                    self.sidebar_width = (start_width + x - start_x).clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
                    self.fit_list_width();
                }
            }
            Message::SidebarEnd => {
                if self.sidebar_drag.take().is_some() {
                    self.save_settings();
                }
            }
            Message::ResetColumns => {
                self.columns = Columns::DEFAULT;
                self.save_settings();
            }
            Message::Resize(size) => {
                self.window_width = size.width;
                self.window_height = size.height;
                self.fit_list_width();
            }
            Message::TogglePane => {
                self.hide_pane = !self.hide_pane;
                self.fit_list_width();
                self.save_settings();
            }
            Message::Ask(prompt) => {
                if !prompt.uses_destination() {
                    self.new_folder_name = if prompt == Prompt::Rename {
                        self.selected.iter().next().and_then(|path| path.file_name()).map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
                    } else {
                        String::new()
                    };
                }
                self.prompt = Some(prompt);
                self.focus_prompt = true;
                self.focus_attempts = 0;
                self.retry_focus_soon(context);
            }
            Message::Confirm => {
                if self.editing_address {
                    self.editing_address = false;
                    self.update(Message::Go, context);
                } else if let Some(prompt) = self.prompt {
                    self.has_error = false;
                    self.update(prompt.message(), context);
                    if !self.has_error {
                        self.prompt = None;
                    }
                }
            }
            Message::CancelPrompt => {
                self.close_menu();
                self.prompt = None;
                self.editing_address = false;
                self.address = self.path.to_string_lossy().into_owned();
                self.new_folder_name.clear();
            }
            Message::EditAddress => {
                self.editing_address = true;
                self.prompt = None;
                self.focus_prompt = false;
                self.focus_attempts = 0;
                self.retry_focus_soon(context);
            }
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
                if self.deleting || self.transferring {
                    return;
                }
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
                    let link = shell::free_name(&self.path, &format!("{name} - {}", tr("Shortcut")), "lnk");
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
                let link = shell::free_name(&self.path, &format!("{name} - {}", tr("Shortcut")), "lnk");
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
                            shell::apply_window_icon(theme.icon_colors(), false);
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
            Message::RowClick(path) => {
                // Enter on a keyboard-driven menu also clicks the row that still holds focus; that click must not change the selection.
                if self.menu.is_some() || self.menu_closed_at.is_some_and(|at| at.elapsed() < MENU_CLICK_GUARD) {
                    return;
                }
                let now = std::time::Instant::now();
                let window = std::time::Duration::from_millis(u64::from(unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime() }));
                let double = self.last_click.take().is_some_and(|(last, at)| last == path && now.duration_since(at) < window);
                if double {
                    let open = if path.is_dir() { Message::OpenFolder(path) } else { Message::OpenFile(path) };
                    self.update(open, context);
                    return;
                }
                self.last_click = Some((path.clone(), now));
                let ctrl = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(windows::Win32::UI::Input::KeyboardAndMouse::VK_CONTROL.0 as i32) } < 0;
                if keys::shift_down() {
                    let visible = self.visible_entries();
                    if let Some(target) = visible.iter().position(|entry| entry.path == path) {
                        self.select_span(&visible, target, true);
                    }
                } else {
                    if !ctrl {
                        self.selected.clear();
                    }
                    if !self.selected.remove(&path) || !ctrl {
                        self.selected.insert(path.clone());
                    }
                    self.anchor = Some(path.clone());
                }
                self.cursor = Some(path);
                self.refresh_status();
            }
            Message::OpenInTab(path) => {
                if path.is_dir() {
                    self.tabs.push(path);
                    self.save_settings();
                }
            }
            Message::RowHover(path) => self.hovered_row = path,
            // The shell menu blocks until it closes, so our own menu is dismissed and repainted first.
            Message::NativeMenu => {
                self.close_menu();
                let _task = context.spawn_background(|_| {
                    std::thread::sleep(std::time::Duration::from_millis(80));
                    Message::NativeMenuRun
                });
            }
            Message::NativeMenuRun => {
                let paths: Vec<PathBuf> = if self.selected.is_empty() { vec![self.path.clone()] } else { self.selected.iter().cloned().collect() };
                match shell::native_menu(&paths) {
                    Ok(true) => self.update(Message::Refresh, context),
                    Ok(false) => {}
                    Err(error) => {
                        self.status = error;
                        self.has_error = true;
                    }
                }
            }
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
                        self.entries = entries.into_iter().map(std::sync::Arc::new).collect();
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
            Message::Searched(generation, path, batch) if generation == self.load_generation && path == self.path => {
                if let Some(error) = batch.error {
                    self.status = error;
                    self.has_error = true;
                    self.searching = false;
                    return;
                }
                self.entries.extend(batch.entries.into_iter().map(std::sync::Arc::new));
                if batch.finished {
                    self.searching = false;
                    self.apply_sort();
                } else if let Some(walk) = self.walk.clone() {
                    self.wait_for_batch(generation, path, walk, context);
                }
                self.refresh_status();
            }
            Message::Searched(..) => {}
        }
    }

    fn view(&self, _input: &PathBuf, context: &mut ViewContext<Self>) -> View {
        context.window_title("Files — Winarchy");
        let pane = if self.pane_hidden() { 0.0 } else { PANE_WIDTH };
        let limits = WindowConstraints { min_width: Some(self.chrome_width() + pane + MIN_LIST_WIDTH), ..Default::default() };
        let visuals = WindowVisuals::new().constraints(limits);
        context.window_visuals(match &self.theme {
            Some(theme) => visuals.theme(theme.window_theme()),
            None => visuals,
        });
        context.on_window_size(context.callback(Message::Resize));
        let address_changed = context.callback(Message::Address);
        let folder_name_changed = context.callback(Message::NewFolderName);
        let transfer_destination_changed = context.callback(Message::TransferDestination);
        let needle = self.filter.to_lowercase();
        let now = std::time::SystemTime::now();
        let visible = self.visible_entries();
        let revision = self.list_revision(&needle, visible.len());
        keys::set_menu_open(self.menu.is_some());
        let hovered = self.hovered_row.clone();
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
        let group = self.group;
        let columns = self.columns;
        let compact = self.layout == Layout::List || (self.layout == Layout::Details && self.list_width < COMPACT_BELOW);
        let lines = group::lines(&visible, group, per_row, now);
        let source = VirtualSource::new(
            revision,
            lines.len(),
            |index| index,
            move |index| {
                let entries = match &lines[index] {
                    Line::Header(title) => return group_header(title, theme.as_ref()),
                    Line::Row(entries) => entries,
                };
                let build = |entry: &Entry| match layout {
                    Layout::Details | Layout::List => row(
                        entry,
                        selected.contains(&entry.path),
                        hovered.as_ref() == Some(&entry.path),
                        theme.as_ref(),
                        &sender,
                        busy,
                        width,
                        columns,
                        hide_extensions,
                        compact,
                    ),
                    Layout::Tiles | Layout::Grid => {
                        tile(entry, selected.contains(&entry.path), hovered.as_ref() == Some(&entry.path), theme.as_ref(), &sender, busy, layout, hide_extensions)
                    }
                };
                let build = |entry: &Entry| -> View {
                    let path = entry.path.clone();
                    let entered = entry.path.clone();
                    Border::new()
                        .on_pointer_entered(sender.callback(move |_: PointerEventInfo| Message::RowHover(Some(entered.clone()))))
                        .on_pointer_exited(sender.callback(|_: PointerEventInfo| Message::RowHover(None)))
                        .on_pointer_pressed(sender.callback(move |info: PointerEventInfo| {
                            if info.is_right_button_pressed {
                                Message::ContextItem(path.clone(), info.window_x, info.window_y)
                            } else if info.is_middle_button_pressed {
                                Message::OpenInTab(path.clone())
                            } else {
                                Message::CloseMenu
                            }
                        }))
                        .content(build(entry))
                        .into()
                };
                if per_row == 1 {
                    return build(&entries[0]);
                }
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .keyed_children(
                        entries
                            .iter()
                            .enumerate()
                            .map(|(slot, entry)| KeyedView::new(slot, build(entry)))
                            .collect::<Vec<_>>(),
                    )
                    .into()
            },
        );

        let theme = self.theme.as_ref();
        let tab_strip = {
            let closable = self.tabs.len() > 1;
            let mut tabs: Vec<View> = self
                .tabs
                .iter()
                .enumerate()
                .map(|(index, path)| {
                    let name = path.file_name().map_or_else(|| path.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned());
                    let active = index == self.active_tab;
                    let title = Button::new()
                        .style(ButtonStyle::Subtle)
                        .horizontal_alignment(HorizontalAlignment::Stretch)
                        .horizontal_content_alignment(HorizontalAlignment::Left)
                        .automation_name(name.clone())
                        .on_click(context.message(Message::SwitchTab(index)))
                        .content(
                            Border::new().margin(Thickness::new(0.0, 0.0, if closable { TAB_CLOSE_SPACE } else { 0.0 }, 0.0)).content(text(&name, theme.map(if active { Theme::foreground } else { Theme::muted }))),
                        )
                        .tooltip(path.to_string_lossy().into_owned());
                    // A Button's template padding clips a fixed-size icon, so the close target is a Border that tracks its own hover.
                    let close = closable.then(|| {
                        let tab_fill = if active { theme.map(Theme::surface) } else { theme.map(Theme::background) };
                        Border::new()
                            .horizontal_alignment(HorizontalAlignment::Right)
                            .vertical_alignment(VerticalAlignment::Center)
                            .width(TAB_CLOSE_SIZE)
                            .height(TAB_CLOSE_SIZE)
                            .margin(Thickness::new(0.0, 0.0, TAB_CLOSE_INSET, 0.0))
                            .corner_radius(CornerRadius::uniform(CARD_RADIUS))
                            .background_optional(if self.hovered_close == Some(index) { theme.and_then(Theme::selection) } else { tab_fill })
                            .on_pointer_entered(context.callback(move |_: PointerEventInfo| Message::CloseHover(Some(index))))
                            .on_pointer_exited(context.callback(|_: PointerEventInfo| Message::CloseHover(None)))
                            .on_pointer_pressed(context.callback(move |_: PointerEventInfo| Message::CloseTabAt(index)))
                            .content(Border::new().horizontal_alignment(HorizontalAlignment::Center).vertical_alignment(VerticalAlignment::Center).content(icon(icons::Icon::X, ICON_SIZE_SMALL, theme)))
                            .tooltip(tr("Close tab"))
                    });
                    Border::new()
                        .on_pointer_pressed(context.callback(move |info: PointerEventInfo| if info.is_middle_button_pressed { Message::CloseTabAt(index) } else { Message::CloseMenu }))
                        .max_width(TAB_MAX_WIDTH)
                        .corner_radius(CornerRadius::new(CONTENT_RADIUS, CONTENT_RADIUS, 0.0, 0.0))
                        .background_optional(if active { theme.map(Theme::surface) } else { None })
                        .content(Grid::new().children((title, close.unwrap_or_else(|| StackPanel::new().into()))))
                        .into()
                })
                .collect();
            tabs.push(
                Button::new()
                    .style(ButtonStyle::Subtle)
                    .automation_name(tr("New tab"))
                    .on_click(context.message(Message::NewTab))
                    .content(icon(icons::Icon::Plus, ICON_SIZE_SMALL, theme))
                    .tooltip(tr("New tab"))
                    .into(),
            );
            StackPanel::new().orientation(Orientation::Horizontal).keyed_children(tabs.into_iter().enumerate().map(|(index, view)| KeyedView::new(index, view)).collect::<Vec<_>>())
        };
        let command_bar = self.command_bar(context);
        let muted = self.theme.as_ref().map(Theme::muted);
        let mut info_items = Vec::new();
        if !self.details.is_empty() {
            let name = self.selected.iter().next().and_then(|path| path.file_name()).map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            info_items.push(KeyedView::new(4usize, title(&name, self.theme.as_ref().map(Theme::foreground))));
            info_items.push(KeyedView::new(0usize, wrapped_text(&self.details, muted, DETAILS_LINES)));
        }
        if !self.preview.is_empty() {
            info_items.push(KeyedView::new(1usize, wrapped_text(&self.preview, self.theme.as_ref().map(Theme::foreground), PREVIEW_LINES)));
        }
        if let Some(branch) = &self.git_branch {
            info_items.push(KeyedView::new(3usize, caption(&format!("Git: {branch}"), self.theme.as_ref().map(Theme::accent))));
        }
        if let Some(image) = self.preview_image.as_ref().and_then(|path| Image::new().source_file(path).ok()) {
            info_items.push(KeyedView::new(
                2usize,
                image.stretch(Stretch::Uniform).horizontal_alignment(HorizontalAlignment::Left).height(PREVIEW_HEIGHT),
            ));
        }
        let header: View = if self.layout == Layout::Details && !compact {
            Grid::new()
                .width(self.list_width)
                .horizontal_alignment(HorizontalAlignment::Left)
                .columns([
                    GridLength::Pixel(CHECK_WIDTH),
                    GridLength::STAR,
                    GridLength::Pixel(self.columns.size),
                    GridLength::Pixel(self.columns.modified + ROW_TRAILING),
                ])
                .column_spacing(COLUMN_GAP)
                .children((
                    self.sort_button(context, SortKey::Name, tr("Name"), 1, HorizontalAlignment::Left),
                    self.sort_button(context, SortKey::Size, tr("Size"), 2, HorizontalAlignment::Right),
                    self.sort_button(context, SortKey::Modified, tr("Modified"), 3, HorizontalAlignment::Right),
                    self.resize_handle(context, Column::Size, 2),
                    self.resize_handle(context, Column::Modified, 3),
                ))
                .into()
        } else {
            StackPanel::new().into()
        };
        let mut body_items = vec![KeyedView::new(
            0usize,
            Border::new()
                .grid_column(0)
                .background_optional(self.theme.as_ref().map(Theme::surface))
                .on_pointer_pressed(context.callback(|info: PointerEventInfo| {
                    if info.is_right_button_pressed {
                        Message::ContextBackground(info.window_x, info.window_y)
                    } else {
                        Message::CloseMenu
                    }
                }))
                .content(
                    ScrollViewer::new()
                        .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
                        .content(ItemsRepeater::new().horizontal_alignment(HorizontalAlignment::Stretch).virtual_source(source)),
                ),
        )];
        if !self.pane_hidden() {
            body_items.push(KeyedView::new(
                1usize,
                ScrollViewer::new()
                    .grid_column(1)
                    .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
                    .content(
                        StackPanel::new()
                            .spacing(STACK_GAP)
                            .margin(Thickness::new(CONTENT_MARGIN, STACK_GAP, CONTENT_MARGIN, 0.0))
                            .keyed_children(info_items),
                    ),
            ));
        }
        let footer = StackPanel::new()
            .spacing(STACK_GAP)
            .margin(Thickness::new(CARD_PADDING, FOOTER_GAP, CONTENT_MARGIN, FOOTER_GAP))
            .children((
                caption(
                    &self.status,
                    self.theme.as_ref().map(|theme| if self.has_error { theme.urgent() } else { theme.muted() }),
                ),
            ));
        let mut prompt_items = Vec::new();
        if let Some(prompt) = self.prompt {
            let (field, changed) = if prompt.uses_destination() {
                (&self.transfer_destination, transfer_destination_changed)
            } else {
                (&self.new_folder_name, folder_name_changed)
            };
            prompt_items.push(KeyedView::new(
                0usize,
                text(tr(prompt.label()), self.theme.as_ref().map(Theme::muted)).grid_column(0).vertical_alignment(VerticalAlignment::Center),
            ));
            prompt_items.push(KeyedView::new(
                1usize,
                TextBox::new()
                    .element_ref(&self.prompt_ref)
                    .grid_column(1)
                    .text(field)
                    .automation_name(tr(prompt.label()))
                    .on_text_changed(changed),
            ));
            prompt_items.push(KeyedView::new(
                2usize,
                Button::new().grid_column(2).style(ButtonStyle::Accent).on_click(context.message(Message::Confirm)).content(tr("OK")),
            ));
            prompt_items.push(KeyedView::new(
                3usize,
                Button::new().grid_column(3).on_click(context.message(Message::CancelPrompt)).content(tr("Cancel")),
            ));
        }
        let ui_theme = self.theme.as_ref();
        let icon_button = |glyph: icons::Icon, name: &str, enabled: bool, column: i32, message: Message| {
            Button::new()
                .grid_column(column)
                .automation_name(name)
                .style(ButtonStyle::Subtle)
                .is_enabled(enabled)
                .on_click(context.message(message))
                .content(icon_state(glyph, ICON_SIZE, ui_theme, enabled))
                .tooltip(name)
        };
        let address_view: View = if self.editing_address {
            TextBox::new()
                .element_ref(&self.address_ref)
                .automation_name(tr("Address"))
                .text(&self.address)
                .grid_column(4)
                .vertical_alignment(VerticalAlignment::Center)
                .on_text_changed(address_changed)
                .into()
        } else {
            Grid::new()
                .grid_column(4)
                .columns([GridLength::Auto, GridLength::STAR])
                .children((
                    BreadcrumbBar::new()
                        .grid_column(0)
                        .vertical_alignment(VerticalAlignment::Center)
                        .items_source(
                            self.crumbs()
                                .into_iter()
                                .enumerate()
                                .map(|(index, (label, _))| format!("{label}{}", ZERO_WIDTH.to_string().repeat(index)))
                                .collect::<Vec<_>>(),
                        )
                        .on_item_clicked(context.callback(Message::Crumb)),
                    Border::new()
                        .grid_column(1)
                        .opacity(MENU_DIM)
                        .background_optional(self.theme.as_ref().map(Theme::background))
                        .on_pointer_pressed(context.callback(|_: PointerEventInfo| Message::EditAddress)),
                ))
                .into()
        };
        let top = StackPanel::new().spacing(BAR_GAP).children((
            tab_strip,
            Grid::new()
                .columns([
                    GridLength::Auto,
                    GridLength::Auto,
                    GridLength::Auto,
                    GridLength::Auto,
                    GridLength::STAR,
                    GridLength::Pixel(FILTER_WIDTH),
                ])
                .column_spacing(ROW_GAP)
                .children((
                    icon_button(icons::Icon::ArrowLeft, tr("Back"), !self.back.is_empty(), 0, Message::Back),
                    icon_button(icons::Icon::ArrowRight, tr("Forward"), !self.forward.is_empty(), 1, Message::Forward),
                    icon_button(icons::Icon::ArrowUp, tr("Up"), true, 2, Message::Parent),
                    icon_button(icons::Icon::ArrowClockwise, tr("Refresh"), true, 3, Message::Refresh),
                    address_view,
                    Grid::new().grid_column(5).vertical_alignment(VerticalAlignment::Center).children((
                        TextBox::new()
                            .text(&self.filter)
                            .automation_name(tr("Filter by name"))
                            .placeholder_text(tr("Filter by name"))
                            .element_ref(&self.filter_ref)
                            .on_text_changed(context.callback(Message::Filter)),
                        self.subfolders_toggle(context),
                    )),
                )),
            command_bar,
            Grid::new()
                .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto, GridLength::Auto])
                .column_spacing(ROW_GAP)
                .keyed_children(prompt_items),
        ));
        let content = Grid::new()
            .key_accelerators(KeyAccelerators::new([KeyAccelerator::new(
                AcceleratorKey::R,
                AcceleratorModifiers::Control,
                context.message(Message::Refresh),
            )]))
            .rows([GridLength::Auto, GridLength::STAR, GridLength::Auto])
            .children((
                Grid::new().grid_row(0).margin(Thickness::new(CARD_PADDING, 0.0, 0.0, 0.0)).children((header,)),
                Border::new()
                    .grid_row(1)
                    .corner_radius(CornerRadius::uniform(CONTENT_RADIUS))
                    .background_optional(self.theme.as_ref().map(Theme::surface))
                    .padding(Thickness::new(CARD_PADDING, CARD_PADDING, CARD_PADDING, CARD_PADDING))
                    .content(
                        Grid::new()
                            .columns(if self.pane_hidden() { vec![GridLength::STAR] } else { vec![GridLength::STAR, GridLength::Pixel(PANE_WIDTH)] })
                            .keyed_children(body_items),
                    ),
                Grid::new().grid_row(2).children((footer,)),
            ));
        let sidebar_items = self
            .places
            .iter()
            .enumerate()
            .map(|(index, (label, path))| {
                let current = *path == self.path;
                let button = Button::new()
                        .grid_column(1)
                        .automation_name(label.clone())
                        .horizontal_alignment(HorizontalAlignment::Stretch)
                        .horizontal_content_alignment(HorizontalAlignment::Left)
                        .style(ButtonStyle::Subtle)
                        .on_click(context.message(Message::OpenFolder(path.clone())))
                        .content(
                            StackPanel::new()
                                .orientation(Orientation::Horizontal)
                                .spacing(ROW_GAP)
                                .children((
                                    icon(place_icon(path, self.pinned.contains(path)), ICON_SIZE, self.theme.as_ref()),
                                    text(label, self.theme.as_ref().map(Theme::foreground)),
                                )),
                        )
                        .tooltip(path.to_string_lossy().into_owned());
                let bar = Border::new()
                    .grid_column(0)
                    .width(ACCENT_BAR_WIDTH)
                    .height(ACCENT_BAR_HEIGHT)
                    .corner_radius(CornerRadius::uniform(ACCENT_BAR_WIDTH))
                    .background_optional(if current { self.theme.as_ref().map(Theme::accent) } else { None });
                let target = path.clone();
                KeyedView::new(
                    index,
                    Border::new()
                        .on_pointer_pressed(context.callback(move |info: PointerEventInfo| if info.is_middle_button_pressed { Message::OpenInTab(target.clone()) } else { Message::CloseMenu }))
                        .corner_radius(CornerRadius::uniform(CARD_RADIUS))
                        .background_optional(if current { self.theme.as_ref().and_then(Theme::selection) } else { None })
                        .content(Grid::new().columns([GridLength::Pixel(ACCENT_BAR_WIDTH + ROW_GAP), GridLength::STAR]).children((bar, button))),
                )
            });
        let splitter = Border::new()
            .grid_column(1)
            .width(SPLITTER_WIDTH)
            .background_optional(self.theme.as_ref().map(Theme::background))
            .capture_pointer_on_press(true)
            .on_pointer_pressed(context.callback(|_: PointerEventInfo| Message::SidebarStart(shell::cursor_x())))
            .on_pointer_moved(context.callback(|info: PointerEventInfo| Message::SidebarDrag(shell::cursor_x(), info.is_left_button_pressed)))
            .on_pointer_released(context.callback(|_: PointerEventInfo| Message::SidebarEnd))
            .content(
                Border::new()
                    .width(HANDLE_WIDTH)
                    .horizontal_alignment(HorizontalAlignment::Center)
                    .background_optional(self.theme.as_ref().map(Theme::muted)),
            );
        let body = Grid::new()
            .grid_row(1)
            .columns([GridLength::Auto, GridLength::Auto, GridLength::STAR])
            .children((
                Grid::new().grid_column(0).width(self.sidebar_width).children((ScrollViewer::new()
                    .content(StackPanel::new().spacing(TIGHT_GAP).keyed_children(sidebar_items)),)),
                splitter,
                Border::new()
                    .grid_column(2)
                    .background_optional(self.theme.as_ref().map(Theme::background))
                    .drop_policy(
                        DragDropPolicy::new().storage_items(
                            DragDropAction::new(DragDropOperation::Copy).caption(tr("Copy here")),
                        ),
                    )
                    .on_drop(context.callback(Message::Dropped))
                    .content(content),
            ));
        let background = self.theme.as_ref().map(Theme::background);
        let page = Grid::new()
            .margin(Thickness::new(WINDOW_MARGIN, WINDOW_MARGIN, WINDOW_MARGIN, WINDOW_MARGIN))
            .rows([GridLength::Auto, GridLength::STAR])
            .children((Grid::new().grid_row(0).margin(Thickness::new(0.0, 0.0, 0.0, BAR_GAP)).children((top,)), body));
        let mut items = vec![KeyedView::new(0usize, page)];
        if let Some((x, y, target)) = &self.menu {
            items.push(KeyedView::new(1usize, self.context_menu(context, *x, *y, *target)));
        }
        Grid::new().background_optional(background).keyed_children(items).into()
    }
}

impl Explorer {
    fn close_menu(&mut self) {
        if self.menu.take().is_some() {
            self.menu_closed_at = Some(std::time::Instant::now());
        }
        self.menu_focus = None;
    }

    fn menu_items(&self, kind: MenuKind) -> Vec<(String, bool)> {
        let commands = self.commands();
        self.menu_entries(&commands, kind).into_iter().map(|command| (command.label.clone(), command.enabled)).collect()
    }

    fn move_menu_focus(&mut self, down: bool) {
        let Some((_, _, kind)) = self.menu else { return };
        let items = self.menu_items(kind);
        let count = items.len();
        let mut index = self.menu_focus;
        for _ in 0..count {
            let next = match (index, down) {
                (None, true) => 0,
                (None, false) => count - 1,
                (Some(current), true) => (current + 1) % count,
                (Some(current), false) => (current + count - 1) % count,
            };
            index = Some(next);
            if items[next].1 {
                self.menu_focus = index;
                return;
            }
        }
    }

    // Selects the entry at `target`, or the span from the anchor to it when extending.
    fn select_span(&mut self, visible: &[std::sync::Arc<Entry>], target: usize, extend: bool) {
        let path = visible[target].path.clone();
        if extend {
            let from = self.anchor.clone().or_else(|| self.cursor.clone()).unwrap_or_else(|| path.clone());
            let start = visible.iter().position(|entry| entry.path == from).unwrap_or(target);
            self.anchor = Some(from);
            self.selected = visible[start.min(target)..=start.max(target)].iter().map(|entry| entry.path.clone()).collect();
        } else {
            self.anchor = Some(path.clone());
            self.selected = HashSet::from([path]);
        }
    }

    fn focus_entry(&mut self, visible: &[std::sync::Arc<Entry>], target: usize, extend: bool, context: &ComponentContext<Self>) {
        self.select_span(visible, target, extend);
        self.cursor = Some(visible[target].path.clone());
        self.refresh_status();
        let name = visible[target].name.clone();
        let fraction = target as f64 / visible.len().saturating_sub(1).max(1) as f64;
        let _task = context.spawn_background(move |_| {
            scroll::reveal(&name, fraction);
            Message::Focus(true)
        });
    }

    fn move_selection(&mut self, key: keys::Key, context: &ComponentContext<Self>) {
        let visible = self.visible_entries();
        if visible.is_empty() {
            return;
        }
        let last = visible.len() - 1;
        let per_row = self.layout.cell_width().map_or(1, |cell| ((self.list_width / cell) as usize).max(1));
        let backwards = matches!(key, keys::Key::MoveUp | keys::Key::MoveLeft | keys::Key::Home);
        let current = self.cursor.as_ref().and_then(|cursor| visible.iter().position(|entry| entry.path == *cursor)).or_else(|| {
            let mut chosen = visible.iter().enumerate().filter(|(_, entry)| self.selected.contains(&entry.path)).map(|(index, _)| index);
            if backwards { chosen.next() } else { chosen.last() }
        });
        let target = match (key, current) {
            (keys::Key::Home, _) => 0,
            (keys::Key::End, _) => last,
            (keys::Key::MoveDown, Some(index)) => (index + per_row).min(last),
            (keys::Key::MoveDown, None) => 0,
            (keys::Key::MoveUp, Some(index)) => index.saturating_sub(per_row),
            (keys::Key::MoveUp, None) => last,
            (keys::Key::MoveRight, Some(index)) if per_row > 1 => (index + 1).min(last),
            (keys::Key::MoveLeft, Some(index)) if per_row > 1 => index.saturating_sub(1),
            (_, Some(index)) => index,
            (_, None) => 0,
        };
        self.focus_entry(&visible, target, keys::shift_down(), context);
    }

    fn menu_entries<'a>(&self, commands: &'a [Command], kind: MenuKind) -> Vec<&'a Command> {
        let find = |label: &str| {
            let label = tr_dynamic(label);
            commands.iter().find(|command| command.label == label)
        };
        match kind {
            MenuKind::Item => ["Cut", "Copy", "Paste here", "Rename selected", "Delete to Recycle Bin", "Compress to zip", "Extract here", "Copy path", "Share", "Add tags", "Terminal here", "Properties", "Show more options"].iter().filter_map(|label| find(label)).collect(),
            MenuKind::Background => ["Paste here", "Paste as shortcut", "Create folder", "Create file", "Terminal here", "Open in editor", "Select none"].iter().filter_map(|label| find(label)).collect(),
            MenuKind::New => NEW_LABELS.iter().filter_map(|label| find(label)).collect(),
            MenuKind::Sort => self.sort_labels().iter().filter_map(|label| find(label)).collect(),
            MenuKind::More => {
                let shown: Vec<String> = NEW_LABELS.iter().chain(&TOOL_LABELS).copied().chain(self.sort_labels()).map(tr_dynamic).collect();
                commands.iter().filter(|command| command.message.is_some() && !shown.contains(&command.label)).collect()
            }
        }
    }

    fn sort_labels(&self) -> [&'static str; 5] {
        [self.sort_label(SortKey::Name, "Sort by name"), self.sort_label(SortKey::Size, "Sort by size"), self.sort_label(SortKey::Modified, "Sort by date"), self.group.label(), self.layout.label()]
    }

    fn context_menu(&self, context: &ViewContext<Self>, x: f64, y: f64, kind: MenuKind) -> View {
        let commands = self.commands();
        let entries = self.menu_entries(&commands, kind);
        let height = entries.len() as f64 * MENU_ITEM_HEIGHT + 2.0 * MENU_PADDING;
        let left = x.min(self.window_width - MENU_WIDTH - WINDOW_MARGIN).max(0.0);
        let height = height.min(self.window_height - 2.0 * WINDOW_MARGIN);
        let top = y.min(self.window_height - height - WINDOW_MARGIN).max(0.0);
        let items = entries.into_iter().enumerate().map(|(index, command)| {
            KeyedView::new(
                index,
                Border::new()
                    .corner_radius(CornerRadius::uniform(CARD_RADIUS))
                    .background_optional(if self.menu_focus == Some(index) { self.theme.as_ref().and_then(Theme::selection) } else { None })
                    .content(Button::new()
                    .automation_name(command.label.clone())
                    .style(ButtonStyle::Subtle)
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .horizontal_content_alignment(HorizontalAlignment::Left)
                    .height(MENU_ITEM_HEIGHT)
                    .is_enabled(command.enabled)
                    .on_click(context.message(Message::MenuCommand(command.label.clone())))
                    .content(
                        StackPanel::new()
                            .orientation(Orientation::Horizontal)
                            .spacing(ROW_GAP)
                            .children((
                                match command.icon {
                                    Some(glyph) => icon_state(glyph, ICON_SIZE, self.theme.as_ref(), command.enabled),
                                    None => StackPanel::new().width(ICON_SIZE).into(),
                                },
                                text(&command.label, self.theme.as_ref().map(Theme::foreground)),
                            )),
                    )),
            )
        });
        let backdrop = Border::new()
            .opacity(MENU_DIM)
            .background_optional(self.theme.as_ref().map(Theme::background))
            .on_pointer_pressed(context.callback(|_: PointerEventInfo| Message::CloseMenu));
        let card = Border::new()
            .width(MENU_WIDTH)
            .horizontal_alignment(HorizontalAlignment::Left)
            .vertical_alignment(VerticalAlignment::Top)
            .margin(Thickness::new(left, top, 0.0, 0.0))
            .padding(Thickness::new(MENU_PADDING, MENU_PADDING, MENU_PADDING, MENU_PADDING))
            .corner_radius(CornerRadius::uniform(MENU_RADIUS))
            .border_thickness(Thickness::new(DIVIDER, DIVIDER, DIVIDER, DIVIDER))
            .border_brush_optional(self.theme.as_ref().map(Theme::muted))
            .background_optional(self.theme.as_ref().map(Theme::surface))
            .height(height)
            .content(ScrollViewer::new().content(StackPanel::new().keyed_children(items)));
        Grid::new().children((backdrop, card)).into()
    }

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
            keys::Key::ToggleHidden => self.update(Message::ToggleHidden, context),
            keys::Key::Refresh => self.update(Message::Refresh, context),
            keys::Key::MoveUp | keys::Key::MoveDown if self.menu.is_some() => self.move_menu_focus(key == keys::Key::MoveDown),
            keys::Key::Open if self.menu.is_some() => {
                if let (Some((_, _, kind)), Some(index)) = (self.menu, self.menu_focus)
                    && let Some((label, true)) = self.menu_items(kind).into_iter().nth(index)
                {
                    self.update(Message::MenuCommand(label), context);
                }
            }
            keys::Key::MoveLeft | keys::Key::MoveRight | keys::Key::Home | keys::Key::End | keys::Key::Char(_) if self.menu.is_some() => {}
            keys::Key::MoveUp | keys::Key::MoveDown | keys::Key::MoveLeft | keys::Key::MoveRight | keys::Key::Home | keys::Key::End => self.move_selection(key, context),
            keys::Key::Char(character) => {
                let now = std::time::Instant::now();
                if now.duration_since(self.type_ahead.1) > TYPE_AHEAD_WINDOW {
                    self.type_ahead.0.clear();
                }
                self.type_ahead = (format!("{}{character}", self.type_ahead.0), now);
                let visible = self.visible_entries();
                if let Some(target) = visible.iter().position(|entry| entry.name.to_lowercase().starts_with(&self.type_ahead.0)) {
                    self.focus_entry(&visible, target, false, context);
                }
            }
            keys::Key::Back => self.update(Message::Back, context),
            keys::Key::Forward => self.update(Message::Forward, context),
            keys::Key::PrevTab => self.update(Message::SwitchTab((self.active_tab + self.tabs.len() - 1) % self.tabs.len()), context),
            keys::Key::ReopenTab => self.update(Message::ReopenTab, context),
            keys::Key::JumpTab(index) => {
                if index < self.tabs.len() {
                    self.update(Message::SwitchTab(index), context);
                }
            }
            keys::Key::Rename => self.update(Message::Command(tr_dynamic("Rename selected")), context),
            keys::Key::NewFolder => self.update(Message::Command(tr_dynamic("Create folder")), context),
            keys::Key::CopyPath => self.update(Message::CopyPath(false), context),
            keys::Key::Properties => self.update(Message::Command(tr_dynamic("Properties")), context),
            keys::Key::DeletePermanently => self.update(Message::DeletePermanently, context),
            keys::Key::FocusFilter => {
                let _ = self.filter_ref.request_focus();
            }
            keys::Key::Confirm => self.update(Message::Confirm, context),
            keys::Key::Cancel => self.update(Message::CancelPrompt, context),
            keys::Key::FocusAddress => self.update(Message::EditAddress, context),
            keys::Key::ContextMenu => {
                // shortcut: opens at a fixed spot below the toolbar, not beside the selected row; track row positions to anchor it.
                let x = self.sidebar_width + SPLITTER_WIDTH + WINDOW_MARGIN + CHECK_WIDTH;
                let message = match self.selected.iter().next() {
                    Some(path) => Message::ContextItem(path.clone(), x, MENU_ANCHOR_Y),
                    None => Message::ContextBackground(x, MENU_ANCHOR_Y),
                };
                self.update(message, context);
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

    // The field is not in the tree until the next render, so the first attempt waits for it.
    fn retry_focus_soon(&self, context: &ComponentContext<Self>) {
        let _task = context.spawn_background(|_| {
            std::thread::sleep(std::time::Duration::from_millis(150));
            Message::RetryFocus
        });
    }

    fn request_address_focus(&mut self, context: &ComponentContext<Self>) {
        self.focus_attempts += 1;
        let sender = context.sender();
        let report = move |result: Result<bool, _>| {
            let _ = sender.send(Message::Focus(matches!(result, Ok(true))));
        };
        let accepted = if self.focus_prompt { self.prompt_ref.request_focus_result(report) } else { self.address_ref.request_focus_result(report) };
        if !accepted {
            self.focus_attempts = u8::MAX;
        }
    }

    fn list_revision(&self, needle: &str, visible: usize) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.hovered_row.hash(&mut hasher);
        (
            self.load_generation,
            self.sort,
            self.descending,
            needle,
            visible,
            self.theme_version,
            self.hide_extensions,
            self.layout,
            self.group,
            (self.columns.size.to_bits(), self.columns.modified.to_bits()),
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

    fn resize_handle(&self, context: &ViewContext<Self>, column: Column, grid_column: i32) -> View {
        Border::new()
            .grid_column(grid_column)
            .width(HANDLE_HIT_WIDTH)
            .horizontal_alignment(HorizontalAlignment::Left)
            .margin(Thickness::new(-(COLUMN_GAP + HANDLE_HIT_WIDTH) / 2.0, HANDLE_INSET, 0.0, HANDLE_INSET))
            .background_optional(self.theme.as_ref().map(Theme::background))
            .capture_pointer_on_press(true)
            .on_pointer_pressed(context.callback(move |_: PointerEventInfo| Message::ResizeStart(column, shell::cursor_x())))
            .on_pointer_moved(context.callback(|info: PointerEventInfo| Message::ResizeDrag(shell::cursor_x(), info.is_left_button_pressed)))
            .on_pointer_released(context.callback(|_: PointerEventInfo| Message::ResizeEnd))
            .on_pointer_entered(context.callback(move |_: PointerEventInfo| Message::HandleHover(Some(column))))
            .on_pointer_exited(context.callback(|_: PointerEventInfo| Message::HandleHover(None)))
            .content(
                Border::new()
                    .width(HANDLE_WIDTH)
                    .horizontal_alignment(HorizontalAlignment::Center)
                    .background_optional(if self.hovered_handle == Some(column) || self.drag.is_some() { self.theme.as_ref().map(Theme::muted) } else { None }),
            )
            .into()
    }

    fn sort_button(
        &self,
        context: &ViewContext<Self>,
        key: SortKey,
        name: &str,
        column: i32,
        align: HorizontalAlignment,
    ) -> View {
        let label = name;
        let theme = self.theme.as_ref();
        let active = self.sort == key;
        let label = TextBlock::new()
            .text(label)
            .font_size(FONT_CAPTION)
            .font_weight(FontWeight::SEMI_BOLD)
            .foreground_optional(theme.map(if active { Theme::foreground } else { Theme::muted }));
        let arrow = active.then(|| icon(if self.descending { icons::Icon::CaretDown } else { icons::Icon::CaretUp }, ICON_SIZE_SMALL, theme));
        let content = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(TIGHT_GAP)
            .vertical_alignment(VerticalAlignment::Center)
            .keyed_children(std::iter::once(View::from(label)).chain(arrow).enumerate().map(|(index, view)| KeyedView::new(index, view)).collect::<Vec<_>>());
        Button::new()
            .grid_column(column)
            .min_width(0.0)
            .margin(match key {
                SortKey::Name => Thickness::new(ICON_WIDTH, 0.0, 0.0, 0.0),
                SortKey::Size => Thickness::new(0.0, 0.0, -BUTTON_INSET, 0.0),
                SortKey::Modified => Thickness::new(0.0, 0.0, ROW_TRAILING - BUTTON_INSET, 0.0),
            })
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .horizontal_content_alignment(align)
            .style(ButtonStyle::Subtle)
            .on_click(context.message(Message::Sort(key)))
            .content(content)
            .tooltip(format!("{} {}", tr("Sort by"), name))
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

    // Sits inside the filter box; its own Border carries the hover because a Button's template padding clips a small icon.
    fn subfolders_toggle(&self, context: &ViewContext<Self>) -> View {
        let theme = self.theme.as_ref();
        let fill = if self.recursive || self.hovered_subfolders { theme.and_then(Theme::selection) } else { theme.map(Theme::surface) };
        Border::new()
            .horizontal_alignment(HorizontalAlignment::Right)
            .vertical_alignment(VerticalAlignment::Center)
            .automation_name(tr("Subfolders"))
            .width(TOGGLE_SIZE)
            .height(TOGGLE_SIZE)
            .margin(Thickness::new(0.0, 0.0, TOGGLE_INSET, 0.0))
            .corner_radius(CornerRadius::uniform(CARD_RADIUS))
            .background_optional(fill)
            .on_pointer_entered(context.callback(|_: PointerEventInfo| Message::SubfoldersHover(true)))
            .on_pointer_exited(context.callback(|_: PointerEventInfo| Message::SubfoldersHover(false)))
            .on_pointer_pressed(context.callback(|_: PointerEventInfo| Message::ToggleRecursive))
            .content(Border::new().horizontal_alignment(HorizontalAlignment::Center).vertical_alignment(VerticalAlignment::Center).content(icon(icons::Icon::FolderOpen, ICON_SIZE_SMALL, theme)))
            .tooltip(tr(if self.recursive { "Subfolders included: click to search only this folder" } else { "Click to include subfolders in the list and the filter" }))
    }

    fn command_bar(&self, context: &ViewContext<Self>) -> View {
        let theme = self.theme.as_ref();
        let commands = self.commands();
        let find = |label: &str| {
            let label = tr_dynamic(label);
            commands.iter().find(|command| command.label == label)
        };
        let anchored = |kind: MenuKind, label: &str, content: View, tooltip: Option<String>| -> View {
            let button = Button::new().style(ButtonStyle::Subtle).automation_name(label).on_click(context.message(Message::OpenDropdown(kind))).content(content);
            Border::new()
                .on_pointer_entered(context.callback(|info: PointerEventInfo| Message::BarAnchor(info.window_x - info.x, info.window_y - info.y)))
                .content(match tooltip {
                    Some(hint) => button.tooltip(hint),
                    None => button.into(),
                })
                .into()
        };
        let dropdown = |glyph: icons::Icon, label: &str, kind: MenuKind| -> View {
            anchored(
                kind,
                label,
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(ROW_GAP)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((icon(glyph, ICON_SIZE, theme), text(label, theme.map(Theme::foreground)), icon(icons::Icon::CaretDown, ICON_SIZE_SMALL, theme)))
                    .into(),
                None,
            )
        };
        let tool = |label: &str| -> Option<View> {
            let command = find(label)?;
            let hint = match shortcut_of(label) {
                Some(keys) => format!("{} ({keys})", command.label),
                None => command.label.clone(),
            };
            Some(
                Button::new()
                    .style(ButtonStyle::Subtle)
                    .automation_name(command.label.clone())
                    .is_enabled(command.enabled)
                    .on_click(context.message(Message::Command(command.label.clone())))
                    .content(icon_state(command.icon?, ICON_SIZE, theme, command.enabled))
                    .tooltip(hint),
            )
        };
        let separator = || Border::new().width(DIVIDER).height(BAR_DIVIDER_HEIGHT).margin(Thickness::new(ROW_GAP, 0.0, ROW_GAP, 0.0)).background_optional(theme.map(Theme::muted)).into();
        let more = anchored(MenuKind::More, &tr("More"), icon(icons::Icon::DotsThree, ICON_SIZE, theme).into(), Some(tr("More").to_owned()));
        let mut left: Vec<View> = vec![dropdown(icons::Icon::Plus, tr("New"), MenuKind::New), separator()];
        left.extend(TOOL_LABELS.iter().filter_map(|label| tool(label)));
        left.push(separator());
        left.push(dropdown(icons::Icon::SortAscending, tr("Sort"), MenuKind::Sort));
        left.push(more);
        let details = Border::new()
            .grid_column(1)
            .corner_radius(CornerRadius::uniform(CARD_RADIUS))
            .background_optional(if self.hide_pane { None } else { theme.and_then(Theme::selection) })
            .content(
                Button::new()
                    .style(ButtonStyle::Subtle)
                    .automation_name(tr("Details"))
                    .on_click(context.message(Message::TogglePane))
                    .content(
                        StackPanel::new()
                            .orientation(Orientation::Horizontal)
                            .spacing(ROW_GAP)
                            .children((icon(icons::Icon::SidebarSimple, ICON_SIZE, theme), text(tr("Details"), theme.map(Theme::foreground)))),
                    )
                    .tooltip(tr("Preview pane")),
            );
        let right = details;
        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .orientation(Orientation::Horizontal)
                    .vertical_alignment(VerticalAlignment::Center)
                    .keyed_children(left.into_iter().enumerate().map(|(index, view)| KeyedView::new(index, view)).collect::<Vec<_>>()),
                right,
            ))
            .into()
    }

    fn pane_hidden(&self) -> bool {
        self.hide_pane || self.window_width < AUTO_HIDE_PANE_BELOW
    }

    fn chrome_width(&self) -> f64 {
        self.sidebar_width + SPLITTER_WIDTH + SCROLL_ALLOWANCE + 2.0 * WINDOW_MARGIN + 2.0 * CARD_PADDING
    }

    fn apply_sort(&mut self) {
        let (key, descending) = (self.sort, self.descending);
        self.entries.sort_by(|a, b| {
            let order = match key {
                SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortKey::Size => a.size.cmp(&b.size),
                SortKey::Modified => a.modified.cmp(&b.modified),
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

    // Pins are shared by every window, so only a pin change writes them; any other save keeps what is on disk.
    fn save_settings(&self) {
        self.write_settings(settings::load().pinned);
    }

    fn write_settings(&self, pinned: Vec<PathBuf>) {
        settings::save(&settings::Settings {
            show_hidden: self.show_hidden,
            hide_extensions: self.hide_extensions,
            sort: self.sort.name().to_owned(),
            descending: self.descending,
            language: self.language.to_owned(),
            layout: self.layout.name().to_owned(),
            group: self.group.name().to_owned(),
            tags: self.tags.clone(),
            hide_pane: self.hide_pane,
            sidebar_width: self.sidebar_width,
            terminal: self.terminal.clone(),
            tabs: self.tabs.clone(),
            active_tab: self.active_tab,
            size_width: self.columns.size,
            modified_width: self.columns.modified,
            pinned,
        });
    }

    // The active key shows its direction; the label stays a static key so it can be translated.
    fn sort_label(&self, key: SortKey, label: &'static str) -> &'static str {
        match (self.sort == key, self.descending, label) {
            (true, false, "Sort by name") => "Sort by name ▲",
            (true, true, "Sort by name") => "Sort by name ▼",
            (true, false, "Sort by size") => "Sort by size ▲",
            (true, true, "Sort by size") => "Sort by size ▼",
            (true, false, "Sort by date") => "Sort by date ▲",
            (true, true, "Sort by date") => "Sort by date ▼",
            _ => label,
        }
    }

    fn fit_list_width(&mut self) {
        let pane = if self.pane_hidden() { 0.0 } else { PANE_WIDTH };
        self.list_width = (self.window_width - self.chrome_width() - pane).max(MIN_LIST_WIDTH);
    }

    fn visible_entries(&self) -> Vec<std::sync::Arc<Entry>> {
        let needle = self.filter.to_lowercase();
        let visible = self.entries.iter().filter(|entry| self.matches(entry, &needle)).cloned().collect();
        group::arrange(visible, self.group, std::time::SystemTime::now())
    }

    fn matches(&self, entry: &Entry, needle: &str) -> bool {
        let found = match needle.strip_prefix("tag:") {
            Some(tag) => self
                .tags
                .get(&entry.path)
                .is_some_and(|tags| tags.iter().any(|candidate| candidate.to_lowercase().contains(tag.trim()))),
            None => entry.name.to_lowercase().contains(needle),
        };
        (self.show_hidden || !entry.hidden) && found
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
            if self.searching {
                self.status.push_str(&format!(" · {}", tr("Searching…")));
            } else if self.recursive && self.entries.len() >= directory::SEARCH_LIMIT {
                self.status.push_str(&format!(" · {} {}", tr("limited to the first"), directory::SEARCH_LIMIT));
            }
            self.details.clear();
            self.preview.clear();
            self.preview_image = None;
            if let [path] = self.selected.iter().collect::<Vec<_>>()[..]
                && let Some(entry) = self.entries.iter().find(|entry| &entry.path == path)
            {
                self.details = directory::properties(entry);
                if let Some(tags) = self.tags.get(&entry.path) {
                    self.details.push_str(&format!("
{}: {}", tr("Tags"), tags.join(", ")));
                }
                self.preview = if directory::is_pdf(entry) { String::new() } else { directory::preview(entry).unwrap_or_default() };
                self.preview_image = if directory::is_image(entry) {
                    Some(entry.path.clone())
                } else {
                    directory::is_pdf(entry).then(|| pdf::first_page(&entry.path)).flatten()
                };
                // the pane already shows the item's details; the status only repeats them when the pane is hidden
                if self.pane_hidden() {
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
        self.save_settings();
        self.notice = None;
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

    fn wait_for_batch(&self, generation: u64, path: PathBuf, walk: std::sync::Arc<directory::Walk>, context: &ComponentContext<Self>) {
        let _task = context.spawn_background(move |_| Message::Searched(generation, path, walk.next_batch()));
    }

    fn queue_load(&mut self, path: PathBuf, context: &ComponentContext<Self>) {
        self.load_generation += 1;
        let generation = self.load_generation;
        let requested_path = path.clone();
        let path_for_git = path.clone();
        if let Some(previous) = self.walk.take() {
            previous.cancel();
        }
        self.searching = self.recursive;
        if self.recursive {
            let walk = directory::Walk::start(path);
            self.walk = Some(walk.clone());
            self.wait_for_batch(generation, requested_path, walk, context);
        } else {
            let _task = context.spawn_background(move |_| {
                let result = directory::read(&path).map_err(|error| format!("{}: {error}", path.display()));
                Message::Loaded(generation, requested_path, result)
            });
        }
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
            if let Undo::Move(items) = &undo {
                let pairs: Vec<_> = items
                    .iter()
                    .filter_map(|(current, parent)| Some((current.clone(), parent.join(current.file_name()?))))
                    .collect();
                self.move_tags(&pairs);
            }
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

    // Pairs are (new path, original path); tags follow the item to its new path.
    fn move_tags(&mut self, pairs: &[(PathBuf, PathBuf)]) {
        let mut moved = false;
        for (new, original) in pairs {
            if let Some(tags) = self.tags.remove(original) {
                self.tags.insert(new.clone(), tags);
                moved = true;
            }
        }
        if moved {
            self.save_settings();
        }
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
                    if let Undo::Rename { from, to } = &undo {
                        self.move_tags(&[(from.clone(), to.clone())]);
                    }
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

fn select_box(entry: &Entry, is_selected: bool, shown: bool, sender: &LocalSender<Message>, busy: bool) -> CheckBox {
    let selected_path = entry.path.clone();
    CheckBox::new()
        .automation_name(format!("{} {}", tr("Select"), entry.name))
        .min_width(0.0)
        .width(CHECK_WIDTH)
        .opacity(if shown || is_selected { 1.0 } else { 0.0 })
        .is_enabled(!busy)
        .is_checked(is_selected)
        .on_is_checked_changed(
            sender.callback(move |checked| Message::Selection(selected_path.clone(), checked)),
        )
}

fn open_button(entry: &Entry, sender: &LocalSender<Message>, column: i32, content: impl Into<View>) -> View {
    let action = Message::RowClick(entry.path.clone());
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
    hovered: bool,
    theme: Option<&Theme>,
    sender: &LocalSender<Message>,
    busy: bool,
    width: f64,
    columns: Columns,
    hide_extensions: bool,
    compact: bool,
) -> View {
    let name = StackPanel::new()
        .horizontal_alignment(HorizontalAlignment::Left)
        .orientation(Orientation::Horizontal)
        .spacing(ROW_GAP)
        .children((
            icon(directory::icon(entry), ICON_SIZE, theme),
            text(&display_name(entry, hide_extensions), theme.map(Theme::foreground)),
        ));
    let check = select_box(entry, is_selected, hovered, sender, busy).grid_column(0);
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
        GridLength::Pixel(columns.size),
        GridLength::Pixel(columns.modified + ROW_TRAILING),
    ])
        .children((
            check,
            button,
            caption(&directory::format_size(entry), theme.map(Theme::muted))
                .grid_column(2)
                .horizontal_alignment(HorizontalAlignment::Right)
                .vertical_alignment(VerticalAlignment::Center),
            caption(&directory::format_modified(entry), theme.map(Theme::muted))
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
    hovered: bool,
    theme: Option<&Theme>,
    sender: &LocalSender<Message>,
    busy: bool,
    layout: Layout,
    hide_extensions: bool,
) -> View {
    let name = text(&display_name(entry, hide_extensions), theme.map(Theme::foreground));
    let icon = icon(directory::icon(entry), if layout == Layout::Grid { ICON_SIZE_LARGE } else { ICON_SIZE }, theme);
    let content: View = if layout == Layout::Grid {
        StackPanel::new().spacing(TIGHT_GAP).horizontal_alignment(HorizontalAlignment::Center).children((icon, name)).into()
    } else {
        let detail = if entry.is_directory { tr("Folder").to_owned() } else { directory::format_size(entry) };
        let lines = vec![KeyedView::new(0usize, name), KeyedView::new(1usize, caption(&detail, theme.map(Theme::muted)))];
        StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(ROW_GAP)
            .children((
                icon,
                StackPanel::new().vertical_alignment(VerticalAlignment::Center).keyed_children(lines),
            ))
            .into()
    };
    let width = if layout == Layout::Grid { GRID_CELL_WIDTH } else { TILE_CELL_WIDTH };
    let card = if is_selected { theme.and_then(Theme::selection) } else { theme.map(Theme::background) };
    Border::new()
        .width(width - 2.0 * CARD_MARGIN)
        .margin(Thickness::new(CARD_MARGIN, CARD_MARGIN, CARD_MARGIN, CARD_MARGIN))
        .corner_radius(CornerRadius::uniform(CARD_RADIUS))
        .background_optional(card)
        .content(
            Grid::new()
                .columns([GridLength::Auto, GridLength::STAR])
                .children((
                    select_box(entry, is_selected, hovered, sender, busy).grid_column(0),
                    open_button(entry, sender, 1, content),
                )),
        )
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

fn group_header(title: &str, theme: Option<&Theme>) -> View {
    text(title, theme.map(Theme::accent)).margin(Thickness::new(ICON_WIDTH, GROUP_GAP, 0.0, ROW_GAP)).into()
}

// Wraps up to `lines` lines, then ends in an ellipsis.
fn wrapped_text(value: &str, foreground: Option<Brush>, lines: i32) -> TextBlock {
    let block = TextBlock::new().text(value).font_size(FONT_CAPTION).text_wrapping(TextWrapping::Wrap).max_lines(lines).text_trimming(TextTrimming::CharacterEllipsis);
    foreground.map(|brush| block.clone().foreground(brush)).unwrap_or(block)
}

fn shortcut_of(label: &str) -> Option<&'static str> {
    match label {
        "Cut" => Some("Ctrl+X"),
        "Copy" => Some("Ctrl+C"),
        "Paste here" => Some("Ctrl+V"),
        "Delete to Recycle Bin" => Some("Del"),
        _ => None,
    }
}

// A disabled control dims its icon too, since images ignore the button's disabled state.
fn icon_state(glyph: icons::Icon, size: f64, theme: Option<&Theme>, enabled: bool) -> View {
    Border::new().opacity(if enabled { 1.0 } else { ICON_DISABLED_OPACITY }).content(icon(glyph, size, theme)).into()
}

const NEW_LABELS: [&str; 4] = ["Create folder", "Create file", "Create shortcut", "Git clone"];
const TOOL_LABELS: [&str; 7] = ["Cut", "Copy", "Copy path", "Paste here", "Rename selected", "Share", "Delete to Recycle Bin"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuKind {
    Item,
    Background,
    New,
    Sort,
    More,
}

// Duotone icon: an accent-tinted layer under a foreground outline, drawn from a themed SVG.
fn icon(icon: icons::Icon, size: f64, theme: Option<&Theme>) -> View {
    let colors = theme.and_then(Theme::icon_colors).map(|[foreground, accent, _]| [foreground, accent]);
    match iconset::file(icon, colors).and_then(|path| Image::new().source_file(path).ok()) {
        Some(image) => image.width(size).height(size).stretch(Stretch::Uniform).into(),
        None => StackPanel::new().width(size).height(size).into(),
    }
}

fn caption(value: &str, foreground: Option<Brush>) -> TextBlock {
    text(value, foreground).font_size(FONT_CAPTION)
}

fn title(value: &str, foreground: Option<Brush>) -> TextBlock {
    text(value, foreground).font_size(FONT_TITLE).font_weight(FontWeight::SEMI_BOLD)
}

fn text(value: &str, foreground: Option<Brush>) -> TextBlock {
    let block = TextBlock::new().text(value).font_size(FONT_BODY).max_lines(1).text_trimming(TextTrimming::CharacterEllipsis);
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
    shell::apply_window_icon(Theme::load().ok().flatten().and_then(|theme| theme.icon_colors()), true);
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
