use std::cell::RefCell;
use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use windows::core::{Error, Interface, HRESULT, PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::FileSystem::{
    GetFileAttributesW, FILE_ATTRIBUTE_DIRECTORY, INVALID_FILE_ATTRIBUTES,
};
use windows::Win32::System::Com::*;
use windows::Win32::System::Ole::*;
use windows::Win32::System::SystemServices::SFGAO_FOLDER;
use windows::Win32::System::Threading::CREATE_NO_WINDOW;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const RECYCLE_BIN: &str = "{645FF040-5081-101B-9F08-00AA002F954E}";

fn desktop_dirs() -> Vec<PathBuf> {
    [FOLDERID_Desktop, FOLDERID_PublicDesktop]
        .iter()
        .filter_map(|id| unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }.ok())
        .map(|p| unsafe {
            let path = PathBuf::from(p.to_string().unwrap_or_default());
            CoTaskMemFree(Some(p.0 as *const _));
            path
        })
        .collect()
}

fn hide_desktop_icon(guid: &str) -> Option<u32> {
    use windows::core::HSTRING;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let mut value = 0u32;
    let mut len = 4u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            windows::core::w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\HideDesktopIcons\NewStartPanel"),
            &HSTRING::from(guid),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut _),
            Some(&mut len),
        )
    };
    (rc.0 == 0).then_some(value)
}

/// Lo que Explorer deja ver en el escritorio: lo que hay en las carpetas Desktop y los íconos
/// del sistema que activa "Configuración de íconos del escritorio" (solo la Papelera por defecto).
fn desktop_item_visible(
    parsing_name: &str,
    desktops: &[PathBuf],
    hide_flag: impl Fn(&str) -> Option<u32>,
) -> bool {
    if let Some(guid) = parsing_name.strip_prefix("::") {
        let guid = guid.to_ascii_uppercase();
        return match hide_flag(&guid) {
            Some(hidden) => hidden == 0,
            None => guid == RECYCLE_BIN,
        };
    }
    let Some(parent) = Path::new(parsing_name).parent() else {
        return false;
    };
    desktops
        .iter()
        .any(|d| d.as_os_str().eq_ignore_ascii_case(parent.as_os_str()))
}

pub const WM_FILES_OPEN_FAILED: u32 = WM_APP + 94;

#[derive(Debug, PartialEq, Eq)]
enum DefaultCommand {
    Ignore,
    OpenInFiles(PathBuf),
    ShellDefault,
    UnsupportedSelection,
    UnsupportedVirtualFolder,
}

fn default_command(
    selection_count: u32,
    has_folder: bool,
    path: Option<PathBuf>,
) -> DefaultCommand {
    match (selection_count, has_folder) {
        (0, _) => DefaultCommand::Ignore,
        (1, true) => path
            .map(DefaultCommand::OpenInFiles)
            .unwrap_or(DefaultCommand::UnsupportedVirtualFolder),
        (count, true) if count > 1 => DefaultCommand::UnsupportedSelection,
        _ => DefaultCommand::ShellDefault,
    }
}

enum ShortcutFolder {
    Filesystem(PathBuf),
    Namespace,
}

unsafe fn shortcut_file(path: &Path) -> bool {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
    {
        return false;
    }
    let path = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let attributes = GetFileAttributesW(PCWSTR(path.as_ptr()));
    attributes != INVALID_FILE_ATTRIBUTES && attributes & FILE_ATTRIBUTE_DIRECTORY.0 == 0
}

unsafe fn resolve_shortcut_folder(path: &Path) -> Option<ShortcutFolder> {
    if !shortcut_file(path) {
        return None;
    }
    let path_wide = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
    link.cast::<IPersistFile>()
        .ok()?
        .Load(PCWSTR(path_wide.as_ptr()), STGM_READ)
        .ok()?;
    let mut target_wide = vec![0u16; 32768];
    let target_path = link
        .GetPath(&mut target_wide, std::ptr::null_mut(), 0)
        .ok()
        .and_then(|_| {
            let length = target_wide.iter().position(|character| *character == 0)?;
            (length > 0).then(|| PathBuf::from(OsString::from_wide(&target_wide[..length])))
        });
    let target_item = link
        .GetIDList()
        .ok()
        .filter(|pidl| !pidl.is_null())
        .and_then(|pidl| {
            let item: Option<IShellItem> = SHCreateItemFromIDList(pidl).ok();
            CoTaskMemFree(Some(pidl.cast()));
            item
        });
    let item_is_folder = target_item
        .as_ref()
        .and_then(|item| item.GetAttributes(SFGAO_FOLDER).ok())
        .is_some_and(|attributes| attributes & SFGAO_FOLDER == SFGAO_FOLDER);
    if item_is_folder {
        if let Some(path) = target_item
            .as_ref()
            .and_then(|item| item.GetDisplayName(SIGDN_FILESYSPATH).ok())
            .map(TaskMemString)
            .and_then(|path| path.0.to_string().ok())
            .map(PathBuf::from)
            .or_else(|| target_path.clone())
        {
            let wide = path
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>();
            let attributes = GetFileAttributesW(PCWSTR(wide.as_ptr()));
            if attributes != INVALID_FILE_ATTRIBUTES && attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0
            {
                return Some(ShortcutFolder::Filesystem(path));
            }
            return None;
        }
        return Some(ShortcutFolder::Namespace);
    }
    target_path.and_then(|path| {
        let wide = path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let attributes = GetFileAttributesW(PCWSTR(wide.as_ptr()));
        (attributes != INVALID_FILE_ATTRIBUTES && attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0)
            .then_some(ShortcutFolder::Filesystem(path))
    })
}

#[cfg(test)]
unsafe fn folder_shortcut_target(path: &Path) -> Option<PathBuf> {
    match resolve_shortcut_folder(path)? {
        ShortcutFolder::Filesystem(path) => Some(path),
        ShortcutFolder::Namespace => None,
    }
}

unsafe fn shell_item_folder_path(
    item: &IShellItem,
) -> windows::core::Result<(bool, Option<PathBuf>)> {
    let is_folder = item.GetAttributes(SFGAO_FOLDER)? & SFGAO_FOLDER == SFGAO_FOLDER;
    let path = item
        .GetDisplayName(SIGDN_FILESYSPATH)
        .ok()
        .map(TaskMemString)
        .and_then(|path| path.0.to_string().ok())
        .map(PathBuf::from);
    if let Some(path) = path.as_ref() {
        match resolve_shortcut_folder(path) {
            Some(ShortcutFolder::Filesystem(target)) => return Ok((true, Some(target))),
            Some(ShortcutFolder::Namespace) => return Ok((true, None)),
            None if shortcut_file(path) => return Ok((false, None)),
            None => {}
        }
    }
    Ok((is_folder, if is_folder { path } else { None }))
}

fn accelerator_consumed(result: HRESULT) -> bool {
    result == S_OK
}

#[windows::core::implement(IShellBrowser, ICommDlgBrowser)]
struct Browser {
    hwnd: HWND,
    active: Rc<RefCell<Option<IShellView>>>,
    root: PathBuf,
}

impl windows::Win32::System::Ole::IOleWindow_Impl for Browser_Impl {
    fn GetWindow(&self) -> windows::core::Result<HWND> {
        Ok(self.hwnd)
    }

    fn ContextSensitiveHelp(&self, _: BOOL) -> windows::core::Result<()> {
        Err(Error::from_hresult(E_NOTIMPL))
    }
}

impl ICommDlgBrowser_Impl for Browser_Impl {
    fn OnDefaultCommand(&self, view: Option<&IShellView>) -> windows::core::Result<()> {
        unsafe {
            let view = view.ok_or_else(|| Error::from_hresult(E_POINTER))?;
            let folder: IFolderView = view.cast()?;
            let selection: IShellItemArray = folder.Items(SVGIO_SELECTION)?;
            let count = selection.GetCount()?;
            let mut has_folder = false;
            let mut path = None;
            for index in 0..count {
                let item = selection.GetItemAt(index)?;
                let (is_folder, item_path) = shell_item_folder_path(&item)?;
                has_folder |= is_folder;
                if count == 1 {
                    path = item_path;
                }
            }
            match default_command(count, has_folder, path) {
                DefaultCommand::Ignore => Ok(()),
                DefaultCommand::OpenInFiles(path) => launch_files(&self.root, &path, self.hwnd)
                    .or_else(|error| {
                        show_message(
                            self.hwnd,
                            &format!("Could not open this folder in Winarchy Files: {error}"),
                            "Winarchy",
                        );
                        Err(Error::from_hresult(E_FAIL))
                    }),
                DefaultCommand::ShellDefault => Err(Error::from_hresult(S_FALSE)),
                DefaultCommand::UnsupportedSelection => {
                    show_message(self.hwnd, unsupported_selection_message(), "Winarchy");
                    Ok(())
                }
                DefaultCommand::UnsupportedVirtualFolder => {
                    show_message(self.hwnd, unsupported_folder_message(), "Winarchy");
                    Ok(())
                }
            }
        }
    }

    fn OnStateChange(&self, _: Option<&IShellView>, _: u32) -> windows::core::Result<()> {
        Ok(())
    }

    fn IncludeObject(
        &self,
        _: Option<&IShellView>,
        pidl: *const windows::Win32::UI::Shell::Common::ITEMIDLIST,
    ) -> windows::core::Result<()> {
        // sin Explorer nadie filtra el espacio de nombres: salen Home, Galería, Documentos, etc.
        let name = unsafe { SHGetNameFromIDList(pidl, SIGDN_DESKTOPABSOLUTEPARSING) }
            .ok()
            .map(|n| unsafe {
                let text = n.to_string().unwrap_or_default();
                CoTaskMemFree(Some(n.0 as *const _));
                text
            });
        match name {
            Some(name) if !desktop_item_visible(&name, &desktop_dirs(), hide_desktop_icon) => {
                Err(S_FALSE.into())
            }
            _ => Ok(()),
        }
    }
}

impl IShellBrowser_Impl for Browser_Impl {
    fn InsertMenusSB(
        &self,
        _: HMENU,
        widths: *mut OLEMENUGROUPWIDTHS,
    ) -> windows::core::Result<()> {
        if !widths.is_null() {
            unsafe { *widths = Default::default() };
        }
        Ok(())
    }

    fn SetMenuSB(&self, _: HMENU, _: isize, _: HWND) -> windows::core::Result<()> {
        Ok(())
    }

    fn RemoveMenusSB(&self, _: HMENU) -> windows::core::Result<()> {
        Ok(())
    }

    fn SetStatusTextSB(&self, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }

    fn EnableModelessSB(&self, _: BOOL) -> windows::core::Result<()> {
        Ok(())
    }

    fn TranslateAcceleratorSB(&self, _: *const MSG, _: u16) -> windows::core::Result<()> {
        Err(Error::from_hresult(S_FALSE))
    }

    fn BrowseObject(
        &self,
        pidl: *const windows::Win32::UI::Shell::Common::ITEMIDLIST,
        flags: u32,
    ) -> windows::core::Result<()> {
        unsafe {
            if pidl.is_null()
                || flags & (SBSP_PARENT | SBSP_NAVIGATEBACK | SBSP_NAVIGATEFORWARD) != 0
            {
                show_message(self.hwnd, unsupported_folder_message(), "Winarchy");
                return Ok(());
            }
            let item: IShellItem = if flags & SBSP_RELATIVE != 0 {
                SHCreateItemWithParent(None, &SHGetDesktopFolder()?, pidl)?
            } else {
                SHCreateItemFromIDList(pidl)?
            };
            let (is_folder, path) = shell_item_folder_path(&item)?;
            if !is_folder {
                show_message(self.hwnd, unsupported_folder_message(), "Winarchy");
                return Ok(());
            }
            let Some(path) = path else {
                show_message(self.hwnd, unsupported_folder_message(), "Winarchy");
                return Ok(());
            };
            if let Err(error) = launch_files(&self.root, &path, self.hwnd) {
                show_message(
                    self.hwnd,
                    &format!("Could not open this folder in Winarchy Files: {error}"),
                    "Winarchy",
                );
            }
            Ok(())
        }
    }

    fn GetViewStateStream(&self, _: u32) -> windows::core::Result<IStream> {
        Err(Error::from_hresult(E_NOTIMPL))
    }

    fn GetControlWindow(&self, _: u32) -> windows::core::Result<HWND> {
        Err(Error::from_hresult(E_NOTIMPL))
    }

    fn SendControlMsg(
        &self,
        _: u32,
        _: u32,
        _: WPARAM,
        _: LPARAM,
        _: *mut LRESULT,
    ) -> windows::core::Result<()> {
        Err(Error::from_hresult(E_NOTIMPL))
    }

    fn QueryActiveShellView(&self) -> windows::core::Result<IShellView> {
        self.active
            .borrow()
            .clone()
            .ok_or_else(|| Error::from_hresult(E_FAIL))
    }

    fn OnViewWindowActive(&self, _: Option<&IShellView>) -> windows::core::Result<()> {
        Ok(())
    }

    fn SetToolbarItems(
        &self,
        _: *const windows::Win32::UI::Controls::TBBUTTON,
        _: u32,
        _: u32,
    ) -> windows::core::Result<()> {
        Ok(())
    }
}

struct TaskMemString(PWSTR);

impl Drop for TaskMemString {
    fn drop(&mut self) {
        unsafe { CoTaskMemFree(Some(self.0 .0.cast())) }
    }
}

struct DesktopView {
    _desktop: IShellFolder,
    view: IShellView,
    active: Rc<RefCell<Option<IShellView>>>,
    _browser: IShellBrowser,
    hwnd: HWND,
    created: bool,
}

impl DesktopView {
    unsafe fn create(parent: HWND, size: RECT, root: PathBuf) -> windows::core::Result<Self> {
        let desktop = SHGetDesktopFolder()?;
        let view: IShellView = desktop.CreateViewObject(parent)?;
        let active = Rc::new(RefCell::new(Some(view.clone())));
        let browser: IShellBrowser = Browser {
            hwnd: parent,
            active: active.clone(),
            root,
        }
        .into();
        let settings = FOLDERSETTINGS {
            ViewMode: FVM_ICON.0 as u32,
            fFlags: (FWF_DESKTOP | FWF_TRANSPARENT).0 as u32,
        };
        let child = view.CreateViewWindow(None, &settings, &browser, &size)?;
        let desktop_view = Self {
            _desktop: desktop,
            view,
            active,
            _browser: browser,
            hwnd: child,
            created: true,
        };
        desktop_view
            .view
            .UIActivate(SVUIA_ACTIVATE_NOFOCUS.0 as u32)?;
        let width = size.right - size.left;
        let height = size.bottom - size.top;
        SetWindowPos(
            desktop_view.hwnd,
            HWND::default(),
            0,
            0,
            width,
            height,
            SWP_NOACTIVATE | SWP_NOZORDER | SWP_SHOWWINDOW,
        )?;
        Ok(desktop_view)
    }

    unsafe fn handle_menu_message(
        view: &IShellView,
        message: u32,
        wp: WPARAM,
        lp: LPARAM,
    ) -> Option<LRESULT> {
        if !matches!(
            message,
            WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR
        ) {
            return None;
        }
        let context: IContextMenu = view.GetItemObject(SVGIO_SELECTION).ok()?;
        if let Ok(context) = context.cast::<IContextMenu3>() {
            let mut result = LRESULT(0);
            if context
                .HandleMenuMsg2(message, wp, lp, Some(&mut result))
                .is_ok()
            {
                return Some(result);
            }
        }
        if let Ok(context) = context.cast::<IContextMenu2>() {
            if context.HandleMenuMsg(message, wp, lp).is_ok() {
                return Some(LRESULT(0));
            }
        }
        None
    }

    unsafe fn shutdown(&mut self) {
        self.active.borrow_mut().take();
        if self.created {
            let _ = self.view.UIActivate(SVUIA_DEACTIVATE.0 as u32);
            let _ = self.view.DestroyViewWindow();
            self.created = false;
        }
    }
}

impl Drop for DesktopView {
    fn drop(&mut self) {
        unsafe { self.shutdown() };
    }
}

thread_local! {
    static VIEW: RefCell<Option<DesktopView>> = const { RefCell::new(None) };
}

pub fn init(parent: HWND, size: RECT, root: PathBuf) -> windows::core::Result<()> {
    let view = unsafe { DesktopView::create(parent, size, root)? };
    let previous = VIEW.with(|slot| slot.borrow_mut().replace(view));
    drop(previous);
    Ok(())
}

pub fn resize(size: RECT) {
    let hwnd = VIEW.with(|slot| slot.borrow().as_ref().map(|view| view.hwnd));
    if let Some(hwnd) = hwnd {
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                HWND::default(),
                0,
                0,
                size.right - size.left,
                size.bottom - size.top,
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_SHOWWINDOW,
            );
        }
    }
}

pub fn invalidate() {
    let hwnd = VIEW.with(|slot| slot.borrow().as_ref().map(|view| view.hwnd));
    if let Some(hwnd) = hwnd {
        unsafe {
            let _ = InvalidateRect(hwnd, None, true);
        }
    }
}

pub fn translate_accelerator(message: &MSG) -> bool {
    let view = VIEW.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|view| (view.view.clone(), view.hwnd))
    });
    let Some((view, hwnd)) = view else {
        return false;
    };
    if message.hwnd != hwnd && !unsafe { IsChild(hwnd, message.hwnd).as_bool() } {
        return false;
    }
    if matches!(message.message, WM_LBUTTONDOWN | WM_RBUTTONDOWN) {
        unsafe { take_focus(hwnd, &view) };
    }
    let result = unsafe { (view.vtable().TranslateAccelerator)(view.as_raw(), message) };
    accelerator_consumed(result)
}

// Suprimir, F2 y Ctrl+C van a la vista solo si la ventana del escritorio es la activa.
unsafe fn take_focus(hwnd: HWND, view: &IShellView) {
    if GetForegroundWindow() == hwnd {
        return;
    }
    let _ = SetForegroundWindow(hwnd);
    let _ = view.UIActivate(SVUIA_ACTIVATE_FOCUS.0 as u32);
}

pub fn handle_menu_message(message: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
    let view = VIEW.with(|slot| slot.borrow().as_ref().map(|view| view.view.clone()))?;
    unsafe { DesktopView::handle_menu_message(&view, message, wp, lp) }
}

pub fn shutdown() {
    let view = VIEW.with(|slot| slot.borrow_mut().take());
    if let Some(mut view) = view {
        unsafe { view.shutdown() };
    }
}

fn launch_files(
    root: &std::path::Path,
    path: &std::path::Path,
    hwnd: HWND,
) -> windows::core::Result<()> {
    // ASFW_ANY: el que se pone al frente es una instancia de Files que ya corre o un nieto de pwsh, sin PID conocido; el permiso caduca con el próximo input del usuario.
    let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(u32::MAX) };
    let mut command = Command::new("pwsh.exe");
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(root.join("bin").join("winarchy.ps1"))
        .arg("files")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW.0);
    let hwnd = hwnd.0 as isize;
    std::thread::Builder::new()
        .name("winarchy-files-launch".into())
        .spawn(move || {
            let exit_code = command.status().ok().and_then(|status| status.code());
            if process_exit_failed(exit_code) {
                let result = exit_code.map(|code| code as u32 as usize + 1).unwrap_or(0);
                unsafe {
                    let _ = PostMessageW(
                        HWND(hwnd as *mut _),
                        WM_FILES_OPEN_FAILED,
                        WPARAM(result),
                        LPARAM(0),
                    );
                }
            }
        })
        .map(|_| ())
        .map_err(|error| Error::new(E_FAIL, error.to_string()))
}

fn process_exit_failed(exit_code: Option<i32>) -> bool {
    exit_code != Some(0)
}

pub fn report_files_open_failure(hwnd: HWND, result: WPARAM) {
    let message = match (crate::i18n::lang(), result.0) {
        (crate::i18n::Lang::Es, 0) => {
            "No se pudo iniciar Winarchy Files. Comprueba PowerShell 7 y la instalación de Files."
                .to_string()
        }
        (crate::i18n::Lang::Es, code) => {
            format!(
                "Winarchy Files no pudo abrir la carpeta (código {}).",
                code - 1
            )
        }
        (_, 0) => "Winarchy Files could not start. Check PowerShell 7 and the Files installation."
            .to_string(),
        (_, code) => format!(
            "Winarchy Files could not open the folder (exit code {}).",
            code - 1
        ),
    };
    show_message(hwnd, &message, "Winarchy");
}

fn show_message(hwnd: HWND, text: &str, title: &str) {
    let text: Vec<u16> = std::ffi::OsStr::new(text)
        .encode_wide()
        .chain(Some(0))
        .collect();
    let title: Vec<u16> = std::ffi::OsStr::new(title)
        .encode_wide()
        .chain(Some(0))
        .collect();
    unsafe {
        let _ = MessageBoxW(
            hwnd,
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONWARNING,
        );
    }
}

fn unsupported_selection_message() -> &'static str {
    if crate::i18n::lang() == crate::i18n::Lang::Es {
        "Abre una sola carpeta a la vez."
    } else {
        "Open one folder at a time."
    }
}

fn unsupported_folder_message() -> &'static str {
    if crate::i18n::lang() == crate::i18n::Lang::Es {
        "Esta ubicación no se puede abrir con Winarchy Files."
    } else {
        "Winarchy Files cannot open this location."
    }
}

#[cfg(test)]
#[path = "desktop_view_tests.rs"]
mod tests;
