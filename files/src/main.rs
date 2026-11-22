#![windows_subsystem = "windows"]

mod directory;
mod i18n;
mod clipboard;
mod keys;
mod settings;
mod shell;
mod theme;

use directory::Entry;
use std::{
    collections::HashSet,
    fs,
    path::{Component as PathComponent, Path, PathBuf},
};
use theme::Theme;
use i18n::tr;
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

fn count_word(count: usize, one: &'static str, many: &'static str) -> &'static str {
    tr(if count == 1 { one } else { many })
}

// Only operations that can be reversed without guessing: names that did not exist before.
enum Undo {
    RemoveDir(PathBuf),
    MakeDir(PathBuf),
    Rename { from: PathBuf, to: PathBuf },
    Recycle(Vec<PathBuf>),
    Move(Vec<(PathBuf, PathBuf)>),
}

impl Undo {
    // Recycled copies cannot be brought back from the bin, so that step has no redo.
    fn inverse(&self) -> Option<Undo> {
        Some(match self {
            Undo::RemoveDir(path) => Undo::MakeDir(path.clone()),
            Undo::MakeDir(path) => Undo::RemoveDir(path.clone()),
            Undo::Rename { from, to } => Undo::Rename {
                from: to.clone(),
                to: from.clone(),
            },
            Undo::Recycle(_) => return None,
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
            Undo::Rename { from, to } => shell::rename(&from, &to).map(|()| true).map_err(|e| e.to_string()),
            Undo::Recycle(paths) => shell::recycle(&paths),
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
        created.push((target, path.parent()?.to_path_buf()));
    }
    if created.is_empty() {
        None
    } else if cut {
        Some(Undo::Move(created))
    } else {
        Some(Undo::Recycle(created.into_iter().map(|(target, _)| target).collect()))
    }
}

#[derive(Clone)]
enum Message {
    Dropped(DroppedData),
    NewWindow,
    ToggleRecursive,
    ToggleHidden,
    CycleLanguage,
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
    RenameName(String),
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
    filter: String,
    clipboard: Vec<PathBuf>,
    clipboard_cut: bool,
    pending_undo: Option<Undo>,
    undo: Vec<Undo>,
    redo: Vec<Undo>,
    pending_inverse: Option<Undo>,
    redoing: bool,
    details: String,
    preview: String,
    preview_image: Option<PathBuf>,
    show_hidden: bool,
    language: &'static str,
    recursive: bool,
    sort: SortKey,
    descending: bool,
    new_folder_name: String,
    rename_name: String,
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
            filter: String::new(),
            clipboard: Vec::new(),
            clipboard_cut: false,
            pending_undo: None,
            undo: Vec::new(),
            redo: Vec::new(),
            pending_inverse: None,
            redoing: false,
            details: String::new(),
            preview: String::new(),
            preview_image: None,
            show_hidden: saved.show_hidden,
            language: i18n::choice(),
            recursive: false,
            sort: SortKey::from_name(&saved.sort),
            descending: saved.descending,
            new_folder_name: String::new(),
            rename_name: String::new(),
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
            places: directory::places(),
            list_width: 800.0,
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
            Message::CycleLanguage => {
                let choices = i18n::choices();
                let next = choices.iter().position(|c| *c == self.language).map_or(0, |i| (i + 1) % choices.len());
                i18n::set_choice(choices[next]);
                self.language = i18n::choice();
                self.save_settings();
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
            Message::Resize(size) => self.list_width = (size.width - SIDEBAR_AND_SCROLL).max(320.0),
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
            Message::RenameName(name) => self.rename_name = name,
            Message::RenameSelected => self.rename_selected(context),
            Message::DeleteSelected => self.delete_selected(context),
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
        let rename_name_changed = context.callback(Message::RenameName);
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
        let source = VirtualSource::new(
            revision,
            visible.len(),
            |index| index,
            move |index| {
                let entry = &visible[index];
                row(
                    entry,
                    selected.contains(&entry.path),
                    theme.as_ref(),
                    &sender,
                    busy,
                    width,
                )
            },
        );

        let toolbar = StackPanel::new().spacing(8.0).children((
            Grid::new()
                .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
                .column_spacing(6.0)
                .children((
                    Button::new()
                        .grid_column(0)
                        .on_click(context.message(Message::Parent))
                        .content(tr("Up")),
                    TextBox::new()
                        .element_ref(&self.address_ref)
                        .automation_name(tr("Address"))
                        .text(&self.address)
                        .grid_column(1)
                        .on_text_changed(address_changed),
                    Button::new()
                        .grid_column(2)
                        .on_click(context.message(Message::Go))
                        .content(tr("Go")),
                )),
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto])
                .column_spacing(6.0)
                .children((
                    TextBox::new()
                        .text(&self.new_folder_name)
                        .placeholder_text(tr("New folder name"))
                        .grid_column(0)
                        .on_text_changed(folder_name_changed),
                    Button::new()
                        .grid_column(1)
                        .is_enabled(!self.deleting && !self.transferring)
                        .on_click(context.message(Message::CreateFolder))
                        .content(tr("Create folder")),
                )),
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto, GridLength::Auto])
                .column_spacing(6.0)
                .children((
                    TextBox::new()
                        .text(&self.rename_name)
                        .placeholder_text(tr("New name for one selected item"))
                        .grid_column(0)
                        .on_text_changed(rename_name_changed),
                    Button::new()
                        .grid_column(1)
                        .is_enabled(
                            self.selected.len() == 1 && !self.deleting && !self.transferring,
                        )
                        .on_click(context.message(Message::RenameSelected))
                        .content(tr("Rename selected")),
                    Button::new()
                        .grid_column(2)
                        .is_enabled(
                            !self.selected.is_empty() && !self.deleting && !self.transferring,
                        )
                        .on_click(context.message(Message::DeleteSelected))
                        .content(tr("Delete to Recycle Bin")),
                )),
            Grid::new()
                .columns([GridLength::Auto, GridLength::Auto, GridLength::Auto])
                .column_spacing(6.0)
                .children((
                    Button::new()
                        .grid_column(0)
                        .is_enabled(!self.selected.is_empty())
                        .on_click(context.message(Message::Clip(false)))
                        .content(tr("Copy")),
                    Button::new()
                        .grid_column(1)
                        .is_enabled(!self.selected.is_empty())
                        .on_click(context.message(Message::Clip(true)))
                        .content(tr("Cut")),
                    Button::new()
                        .grid_column(2)
                        .is_enabled(
                            clipboard::has_files() && !self.deleting && !self.transferring,
                        )
                        .on_click(context.message(Message::Paste))
                        .content(tr("Paste here")),
                )),
            text(tr("Destination folder"), self.theme.as_ref().map(Theme::muted)),
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto, GridLength::Auto])
                .column_spacing(6.0)
                .children((
                    TextBox::new()
                        .text(&self.transfer_destination)
                        .automation_name(tr("Destination folder path"))
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
                    GridLength::STAR,
                    GridLength::Auto,
                    GridLength::Auto,
                    GridLength::Auto,
                    GridLength::Auto,
                    GridLength::Auto,
                    GridLength::Auto,
                ])
                .column_spacing(6.0)
                .children((
                    self.sort_button(context, SortKey::Name, tr("Name"), 0),
                    self.sort_button(context, SortKey::Size, tr("Size"), 1),
                    self.sort_button(context, SortKey::Modified, tr("Modified"), 2),
                    Button::new()
                        .grid_column(3)
                        .automation_name(tr("Show hidden items"))
                        .on_click(context.message(Message::ToggleHidden))
                        .content(tr(if self.show_hidden { "Hidden: shown" } else { "Hidden: off" })),
                    Button::new()
                        .grid_column(4)
                        .automation_name(tr("Search subfolders"))
                        .on_click(context.message(Message::ToggleRecursive))
                        .content(tr(if self.recursive { "Subfolders: on" } else { "Subfolders: off" })),
                    Button::new()
                        .grid_column(5)
                        .automation_name(tr("New window"))
                        .on_click(context.message(Message::NewWindow))
                        .content(tr("New window")),
                    Button::new()
                        .grid_column(6)
                        .automation_name(tr("Change language"))
                        .on_click(context.message(Message::CycleLanguage))
                        .content(tr(match self.language {
                            "es" => "Language: Spanish",
                            "en" => "Language: English",
                            _ => "Language: auto",
                        })),
                )),
            text(
                &self.path.to_string_lossy(),
                self.theme.as_ref().map(Theme::muted),
            ),
            text(&self.details, self.theme.as_ref().map(Theme::muted)),
            text(&self.preview, self.theme.as_ref().map(Theme::muted)),
            match self
                .preview_image
                .as_ref()
                .and_then(|path| Image::new().source_file(path).ok())
            {
                Some(image) => image
                    .stretch(Stretch::Uniform)
                    .horizontal_alignment(HorizontalAlignment::Left)
                    .height(160.0),
                None => Image::new().height(0.0),
            },
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
                    .content(StackPanel::new().spacing(2.0).keyed_children(sidebar_items)),)),
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
            keys::Key::Redo => self.update(Message::Redo, context),
            keys::Key::Delete => self.update(Message::DeleteSelected, context),
            keys::Key::Up => self.update(Message::Parent, context),
            keys::Key::Refresh => self.update(Message::Refresh, context),
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
    ) -> View {
        let arrow = match (self.sort == key, self.descending) {
            (false, _) => "",
            (true, false) => " ▲",
            (true, true) => " ▼",
        };
        Button::new()
            .grid_column(column)
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
            sort: self.sort.name().to_owned(),
            descending: self.descending,
            language: self.language.to_owned(),
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
                format!("{shown} {} · {} {}", count_word(shown, "item", "items"), self.selected.len(), count_word(self.selected.len(), "selected one", "selected"))
            } else {
                format!(
                    "{shown} {} {} {} · {} {}",
                    tr("of"),
                    self.entries.len(),
                    tr("items"),
                    self.selected.len(),
                    count_word(self.selected.len(), "selected one", "selected")
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
                self.preview = directory::preview(entry).unwrap_or_default();
                self.preview_image = directory::is_image(entry).then(|| entry.path.clone());
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
        self.path = path.clone();
        self.recursive = false;
        self.address = path.to_string_lossy().into_owned();
        self.new_folder_name.clear();
        self.rename_name.clear();
        self.selected.clear();
        self.entries.clear();
        self.status = tr("Loading folder…").to_owned();
        self.has_error = false;
        self.queue_load(path, context);
    }

    fn queue_load(&mut self, path: PathBuf, context: &ComponentContext<Self>) {
        self.load_generation += 1;
        let generation = self.load_generation;
        let requested_path = path.clone();
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
    }

    fn delete_selected(&mut self, context: &ComponentContext<Self>) {
        if self.deleting || self.transferring || self.selected.is_empty() {
            return;
        }
        let paths: Vec<_> = self.selected.iter().cloned().collect();
        let parent = self.path.clone();
        self.deleting = true;
        self.status = tr("Deleting to Recycle Bin…").to_owned();
        self.has_error = false;
        let _task = context.spawn_background(move |_| {
            let result = shell::recycle(&paths);
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
                self.rename_name.clear();
                self.entries.clear();
                self.status = tr("Loading folder…").to_owned();
                self.has_error = false;
                self.queue_load(parent, context);
            }
            Err(error) => {
                self.pending_undo = None;
                self.rename_name = name;
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
        let name = self.rename_name.trim();
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
            self.rename_name.clear();
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

const SIDEBAR_AND_SCROLL: f64 = 100.0;

fn row(
    entry: &Entry,
    is_selected: bool,
    theme: Option<&Theme>,
    sender: &LocalSender<Message>,
    busy: bool,
    width: f64,
) -> View {
    let path = entry.path.clone();
    let selected_path = path.clone();
    let action = if entry.is_directory {
        Message::OpenFolder(path)
    } else {
        Message::OpenFile(path)
    };
    Grid::new()
        .width(width)
        .background_optional(if is_selected {
            theme.and_then(Theme::selection)
        } else {
            None
        })
        .columns([
            GridLength::Auto,
            GridLength::STAR,
            GridLength::Auto,
            GridLength::Auto,
        ])
        .column_spacing(12.0)
        .children((
            CheckBox::new()
                .automation_name(format!("{} {}", tr("Select"), entry.name))
                .grid_column(0)
                .is_enabled(!busy)
                .is_checked(is_selected)
                .on_is_checked_changed(
                    sender.callback(move |checked| {
                        Message::Selection(selected_path.clone(), checked)
                    }),
                ),
            Button::new()
                .grid_column(1)
                .horizontal_content_alignment(HorizontalAlignment::Left)
                .style(ButtonStyle::Subtle)
                .on_click(sender.message(action))
                .content(text(&entry.name, theme.map(Theme::foreground))),
            text(&directory::format_size(entry), theme.map(Theme::muted)).grid_column(2),
            text(&directory::format_modified(entry), theme.map(Theme::muted))
                .grid_column(3)
                .margin(Thickness::new(0.0, 0.0, 16.0, 0.0)),
        ))
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
