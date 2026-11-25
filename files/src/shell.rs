use std::path::{Path, PathBuf};

pub fn open(path: &Path) -> Result<(), String> {
    execute("open", &path.to_string_lossy(), None)
}

// Verbs such as "open", "properties" or "runas" (elevated, with its own UAC prompt).
#[cfg(windows)]
pub fn execute(verb: &str, file: &str, parameters: Option<&str>) -> Result<(), String> {
    use windows::{
        Win32::UI::Shell::{SEE_MASK_INVOKEIDLIST, SHELLEXECUTEINFOW, ShellExecuteExW},
        core::PCWSTR,
    };

    let wide = |text: &str| text.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let operation = wide(verb);
    let file_text = file;
    let file = wide(file);
    let parameters = parameters.map(wide);
    // como Explorer: los .bat y programas con rutas relativas corren desde la carpeta del archivo
    let directory = Path::new(file_text).parent().filter(|dir| dir.is_dir()).map(|dir| wide(&dir.to_string_lossy()));
    // Only the "properties" verb needs INVOKEIDLIST; it makes the shell parse the file as an item id list.
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: if verb == "properties" { SEE_MASK_INVOKEIDLIST } else { 0 },
        lpVerb: PCWSTR(operation.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: parameters.as_ref().map_or(PCWSTR::null(), |p| PCWSTR(p.as_ptr())),
        lpDirectory: directory.as_ref().map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr())),
        nShow: 1,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info) }.map_err(|error| error.to_string())
}

#[cfg(windows)]
pub fn rename(source: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileW(source: *const u16, target: *const u16) -> i32;
    }

    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe { MoveFileW(source.as_ptr(), target.as_ptr()) } == 0 {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
pub fn recycle(paths: &[std::path::PathBuf]) -> Result<bool, String> {
    delete(paths, false)
}

#[cfg(windows)]
// A permanent delete leaves out FOF_ALLOWUNDO, so Windows asks for its own confirmation.
pub fn delete(paths: &[std::path::PathBuf], permanent: bool) -> Result<bool, String> {
    use std::{
        ffi::c_void,
        os::windows::ffi::OsStrExt,
        path::{Component, Prefix},
    };

    #[repr(C)]
    struct FileOperation {
        window: *mut c_void,
        operation: u32,
        from: *const u16,
        to: *const u16,
        flags: u16,
        aborted: i32,
        mappings: *mut c_void,
        progress_title: *const u16,
    }

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SHFileOperationW(operation: *mut FileOperation) -> i32;
    }

    let paths = paths
        .iter()
        .map(std::path::absolute)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    if paths.iter().any(|path| {
        !path.is_absolute()
            || matches!(
                path.components().next(),
                Some(Component::Prefix(prefix))
                    if matches!(
                        prefix.kind(),
                        Prefix::Verbatim(_)
                            | Prefix::VerbatimDisk(_)
                            | Prefix::VerbatimUNC(_, _)
                            | Prefix::DeviceNS(_)
                    )
            )
    }) {
        return Err("Recycle Bin requires standard absolute paths".to_owned());
    }
    std::thread::spawn(move || {
        let mut from = Vec::new();
        for path in &paths {
            if !path.exists() {
                return Err(format!("{} no longer exists", path.display()));
            }
            from.extend(path.as_os_str().encode_wide());
            from.push(0);
        }
        from.push(0);
        let mut operation = FileOperation {
            window: std::ptr::null_mut(),
            operation: 3,
            from: from.as_ptr(),
            to: std::ptr::null(),
            flags: if permanent { 0 } else { 0x0040 },
            aborted: 0,
            mappings: std::ptr::null_mut(),
            progress_title: std::ptr::null(),
        };

        #[link(name = "ole32")]
        unsafe extern "system" {
            fn CoInitializeEx(reserved: *mut c_void, apartment: u32) -> i32;
            fn CoUninitialize();
        }

        let initialized = unsafe { CoInitializeEx(std::ptr::null_mut(), 0x2) };
        if initialized < 0 {
            return Err(format!(
                "COM initialization failed (0x{:08X})",
                initialized as u32
            ));
        }

        let result = unsafe { SHFileOperationW(&mut operation) };
        unsafe { CoUninitialize() };
        if operation.aborted != 0 || result == 0x75 {
            Ok(false)
        } else if result != 0 {
            Err(format!("Windows file operation failed (0x{result:X})"))
        } else {
            Ok(true)
        }
    })
    .join()
    .map_err(|_| "File operation failed".to_owned())?
}

#[cfg(not(windows))]
pub fn delete(_paths: &[std::path::PathBuf], _permanent: bool) -> Result<bool, String> {
    Err("Deleting is supported only on Windows".to_owned())
}

#[cfg(not(windows))]
pub fn rename(source: &Path, target: &Path) -> Result<(), String> {
    std::fs::rename(source, target).map_err(|error| error.to_string())
}

#[cfg(windows)]
pub fn transfer(paths: &[PathBuf], destination: &Path, move_items: bool) -> Result<bool, String> {
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};

    if paths.is_empty() {
        return Err("Select at least one item".to_owned());
    }

    let paths = paths
        .iter()
        .map(std::path::absolute)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    if let Some(path) = paths.iter().find(|path| !path.exists()) {
        return Err(format!("{} no longer exists", path.display()));
    }

    let destination = std::path::absolute(destination).map_err(|error| error.to_string())?;
    if !destination.is_dir() {
        return Err(format!(
            "Destination is not an accessible folder: {}",
            destination.display()
        ));
    }

    std::thread::spawn(move || {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(|error| error.to_string())?;
        let result = perform_transfer(&paths, &destination, move_items);
        unsafe { CoUninitialize() };
        result.map_err(|error| error.to_string())
    })
    .join()
    .map_err(|_| "File operation failed".to_owned())?
}

#[cfg(windows)]
fn perform_transfer(
    paths: &[PathBuf],
    destination: &Path,
    move_items: bool,
) -> windows::core::Result<bool> {
    use windows::{
        Win32::{
            System::Com::{CLSCTX_ALL, CoCreateInstance},
            UI::Shell::{FileOperation, IFileOperation, IFileOperationProgressSink},
        },
        core::{IUnknown, PCWSTR},
    };

    let operation: IFileOperation =
        unsafe { CoCreateInstance(&FileOperation, None::<&IUnknown>, CLSCTX_ALL)? };
    let destination_item = shell_item(destination)?;
    // pasting into the folder the items came from makes "name - Copy" instead of a conflict
    if !move_items && paths.iter().any(|path| path.parent() == Some(destination)) {
        use windows::Win32::UI::Shell::{FOF_ALLOWUNDO, FOF_RENAMEONCOLLISION};
        unsafe { operation.SetOperationFlags(FOF_ALLOWUNDO | FOF_RENAMEONCOLLISION)? };
    }
    for path in paths {
        let source = shell_item(path)?;
        unsafe {
            if move_items {
                operation.MoveItem(
                    &source,
                    &destination_item,
                    PCWSTR::null(),
                    None::<&IFileOperationProgressSink>,
                )?;
            } else {
                operation.CopyItem(
                    &source,
                    &destination_item,
                    PCWSTR::null(),
                    None::<&IFileOperationProgressSink>,
                )?;
            }
        }
    }

    unsafe {
        operation.PerformOperations()?;
        Ok(!operation.GetAnyOperationsAborted()?.as_bool())
    }
}

// Creates a folder or an empty file; in a protected folder the shell operation asks for elevation, like Explorer does.
#[cfg(windows)]
pub fn create_item(path: &Path, directory: bool) -> Result<(), String> {
    let created = if directory { std::fs::create_dir(path).map(|()| ()) } else { std::fs::File::create_new(path).map(|_| ()) };
    match created {
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => new_item_elevated(path, directory).map_err(|_| error.to_string()),
        other => other.map_err(|error| error.to_string()),
    }
}

#[cfg(windows)]
fn new_item_elevated(path: &Path, directory: bool) -> windows::core::Result<()> {
    use windows::{
        Win32::{
            Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL},
            System::Com::{CLSCTX_ALL, CoCreateInstance},
            UI::Shell::{FileOperation, IFileOperation, IFileOperationProgressSink},
        },
        core::{HSTRING, IUnknown},
    };
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else { return Err(windows::core::Error::empty()) };
    let operation: IFileOperation = unsafe { CoCreateInstance(&FileOperation, None::<&IUnknown>, CLSCTX_ALL)? };
    let attributes = if directory { FILE_ATTRIBUTE_DIRECTORY } else { FILE_ATTRIBUTE_NORMAL };
    unsafe {
        operation.NewItem(&shell_item(parent)?, attributes.0, &HSTRING::from(name), None, None::<&IFileOperationProgressSink>)?;
        operation.PerformOperations()?;
        if operation.GetAnyOperationsAborted()?.as_bool() { return Err(windows::core::Error::empty()) }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn create_item(path: &Path, directory: bool) -> Result<(), String> {
    let created = if directory { std::fs::create_dir(path) } else { std::fs::File::create_new(path).map(|_| ()) };
    created.map_err(|error| error.to_string())
}

#[cfg(windows)]
fn shell_item(path: &Path) -> windows::core::Result<windows::Win32::UI::Shell::IShellItem> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        Win32::{System::Com::IBindCtx, UI::Shell::SHCreateItemFromParsingName},
        core::PCWSTR,
    };

    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe { SHCreateItemFromParsingName(PCWSTR::from_raw(path.as_ptr()), None::<&IBindCtx>) }
}

#[cfg(not(windows))]
pub fn transfer(
    _paths: &[PathBuf],
    _destination: &Path,
    _move_items: bool,
) -> Result<bool, String> {
    Err("File operations are supported only on Windows".to_owned())
}

#[cfg(not(windows))]
pub fn open(_path: &Path) -> Result<(), String> {
    Err("Opening files is supported only on Windows".to_owned())
}

#[cfg(windows)]
// A name that does not exist yet, so a shortcut never overwrites (and its undo never deletes) a user's file.
pub fn free_name(dir: &Path, stem: &str, extension: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{stem}.{extension}"));
    let mut counter = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem} ({counter}).{extension}"));
        counter += 1;
    }
    candidate
}

#[cfg(windows)]
pub fn print_to_parent_console(line: &str) {
    use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) }.is_ok() {
        println!("{line}");
    }
}

#[cfg(not(windows))]
pub fn print_to_parent_console(line: &str) {
    println!("{line}");
}

// A Start menu shortcut that carries the app's taskbar identity, so pinning it and running it group as one app.
#[cfg(windows)]
pub fn create_app_shortcut(target: &Path, link: &Path, app_id: &str) -> Result<(), String> {
    use windows::{
        Win32::{
            System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, IPersistFile},
            UI::Shell::{
                IShellLinkW, ShellLink,
                PropertiesSystem::IPropertyStore,
            },
        },
        core::{GUID, HSTRING, Interface},
    };
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let shell_link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
        shell_link.SetPath(&HSTRING::from(target.as_os_str())).map_err(|e| e.to_string())?;
        if let Some(folder) = target.parent() {
            shell_link.SetWorkingDirectory(&HSTRING::from(folder.as_os_str())).map_err(|e| e.to_string())?;
        }
        let store: IPropertyStore = shell_link.cast().map_err(|e| e.to_string())?;
        let key = windows::Win32::Foundation::PROPERTYKEY { fmtid: GUID::from_u128(0x9F4C2855_9F79_4B39_A8D0_E1D42DE1D5F3), pid: 5 };
        let value = windows::Win32::System::Com::StructuredStorage::PROPVARIANT::from(app_id);
        store.SetValue(&key, &value).map_err(|e| e.to_string())?;
        store.Commit().map_err(|e| e.to_string())?;
        shell_link.cast::<IPersistFile>().and_then(|file| file.Save(&HSTRING::from(link.as_os_str()), true)).map_err(|e| e.to_string())
    }
}

#[cfg(not(windows))]
pub fn create_app_shortcut(_target: &Path, _link: &Path, _app_id: &str) -> Result<(), String> {
    Err("Shortcuts are supported only on Windows".to_owned())
}

pub const APP_ID: &str = "Winarchy.Files";

// Groups every window and pin of the app under one taskbar identity instead of the exe path.
#[cfg(windows)]
pub fn set_app_id() {
    let _ = unsafe { windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(windows::core::w!("Winarchy.Files")) };
}

#[cfg(not(windows))]
pub fn set_app_id() {}

#[cfg(windows)]
pub fn link_target(link: &Path) -> Option<PathBuf> {
    use windows::{
        Win32::{
            Storage::FileSystem::WIN32_FIND_DATAW,
            System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READ},
            UI::Shell::{IShellLinkW, ShellLink},
        },
        core::{HSTRING, Interface},
    };
    unsafe {
        let shell_link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        shell_link.cast::<IPersistFile>().ok()?.Load(&HSTRING::from(link.as_os_str()), STGM_READ).ok()?;
        let mut buffer = [0u16; 520];
        let mut data = WIN32_FIND_DATAW::default();
        shell_link.GetPath(&mut buffer, &mut data, 0).ok()?;
        let length = buffer.iter().position(|&c| c == 0)?;
        (length > 0).then(|| PathBuf::from(String::from_utf16_lossy(&buffer[..length])))
    }
}

#[cfg(not(windows))]
pub fn link_target(_link: &Path) -> Option<PathBuf> {
    None
}

#[cfg(windows)]
pub fn create_shortcut(target: &Path, link: &Path) -> Result<(), String> {
    use windows::{
        Win32::{
            System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile},
            UI::Shell::{IShellLinkW, ShellLink},
        },
        core::{Interface, HSTRING},
    };
    unsafe {
        let shell_link: IShellLinkW =
            CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
        shell_link.SetPath(&HSTRING::from(target.as_os_str())).map_err(|e| e.to_string())?;
        shell_link
            .cast::<IPersistFile>()
            .and_then(|file| file.Save(&HSTRING::from(link.as_os_str()), true))
            .map_err(|e| e.to_string())
    }
}

#[cfg(not(windows))]
pub fn create_shortcut(_target: &Path, _link: &Path) -> Result<(), String> {
    Err("Shortcuts are supported only on Windows".to_owned())
}

#[cfg(windows)]
pub fn empty_recycle_bin() -> Result<(), String> {
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SHEmptyRecycleBinW(window: *mut std::ffi::c_void, root: *const u16, flags: u32) -> i32;
    }
    // Windows shows its own confirmation unless flags ask otherwise.
    let result = unsafe { SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), 0) };
    // S_OK, or the "already empty" result Windows returns for an empty bin.
    if result == 0 || result == 0x8000_0001u32 as i32 { Ok(()) } else { Err(format!("0x{:08X}", result as u32)) }
}

// Uses the tar.exe that ships with Windows 10 and later, so no archive library is needed.
pub fn compress(items: &[PathBuf], parent: &Path, archive: &Path) -> Result<(), String> {
    let mut command = std::process::Command::new("tar.exe");
    command.args(["-a", "-c", "-f"]).arg(archive).arg("-C").arg(parent);
    for item in items {
        command.arg(item.file_name().ok_or("item has no name")?);
    }
    run_tar(&mut command)
}

pub fn extract(archive: &Path, destination: &Path) -> Result<(), String> {
    std::fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    run_tar(std::process::Command::new("tar.exe").arg("-x").arg("-f").arg(archive).arg("-C").arg(destination))
}

pub fn git(args: &[&str], dir: &Path) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    let output = std::process::Command::new("git")
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .current_dir(dir)
        .creation_flags(0x0800_0000)
        .output()
        .map_err(|error| error.to_string())?;
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).trim().to_owned();
    if output.status.success() {
        Ok(text(&output.stdout))
    } else {
        Err(text(&output.stderr))
    }
}

fn run_tar(command: &mut std::process::Command) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let output = command.creation_flags(0x0800_0000).output().map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

// Screen position in layout units: event coordinates move with the dragged handle, so they cannot drive a resize.
#[cfg(windows)]
pub fn cursor_x() -> f64 {
    use windows::Win32::{Foundation::POINT, UI::{HiDpi::GetDpiForSystem, WindowsAndMessaging::GetCursorPos}};
    let mut point = POINT::default();
    // SAFETY: `point` is a valid out pointer for the duration of the call.
    unsafe {
        let _ = GetCursorPos(&mut point);
        f64::from(point.x) * 96.0 / f64::from(GetDpiForSystem().max(96))
    }
}

#[cfg(not(windows))]
pub fn cursor_x() -> f64 {
    0.0
}

// Unpackaged apps cannot use GetForCurrentView, so the share UI is opened through the window interop factory.
#[cfg(windows)]
pub fn share(paths: &[PathBuf]) -> Result<(), String> {
    use windows::{
        ApplicationModel::DataTransfer::{DataRequestedEventArgs, DataTransferManager},
        Foundation::TypedEventHandler,
        Storage::{IStorageItem, StorageFile},
        Win32::UI::{Shell::IDataTransferManagerInterop, WindowsAndMessaging::GetForegroundWindow},
        core::{HSTRING, Interface, factory},
    };
    use windows_collections::IIterable;

    let failed = |error: windows::core::Error| error.message();
    let interop = factory::<DataTransferManager, IDataTransferManagerInterop>().map_err(failed)?;
    let window = unsafe { GetForegroundWindow() };
    let manager: DataTransferManager = unsafe { interop.GetForWindow(window) }.map_err(failed)?;
    let paths = paths.to_vec();
    manager
        .DataRequested(&TypedEventHandler::new(move |_, arguments: windows::core::Ref<DataRequestedEventArgs>| {
            if let Some(arguments) = arguments.as_ref() {
                let mut items = Vec::new();
                for path in &paths {
                    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(path.to_string_lossy().as_ref()))?.join()?;
                    items.push(Some(file.cast::<IStorageItem>()?));
                }
                let data = arguments.Request()?.Data()?;
                data.Properties()?.SetTitle(&HSTRING::from("Files"))?;
                data.SetStorageItems(&IIterable::<IStorageItem>::from(items), true)?;
            }
            Ok(())
        }))
        .map_err(failed)?;
    unsafe { interop.ShowShareUIForWindow(window) }.map_err(failed)
}

#[cfg(not(windows))]
pub fn share(_paths: &[PathBuf]) -> Result<(), String> {
    Err("sharing needs Windows".to_owned())
}

const ICON_LAYERS: &[u8] = include_bytes!("../assets/icon-layers.bin");
const ICON_SIZES: [usize; 4] = [16, 24, 32, 48];
const ICON_OPACITY: [f32; 3] = [0.55, 0.45, 1.0];
const DEFAULT_ICON_COLORS: [[u8; 3]; 3] = [[0xd4, 0xd4, 0xd8], [0x8b, 0x91, 0x9c], [0x1f, 0x21, 0x25]];

// The baked 48px layers carry about a fifth of padding per side; this crop (origin and side) trims it so the icon fills its slot like its neighbours.
const ICON_CROP: (f32, f32, f32) = (8.0, 6.0, 36.0);
const ICON_SOURCE: usize = 48;
const ICON_SUPERSAMPLE: usize = 3;

// BGRA pixels of the icon: back folder, accent panel and front outline tinted and stacked, cropped from the largest bake and box-filtered to `size`.
fn icon_pixels(size: usize, tints: [[u8; 3]; 3]) -> Vec<u8> {
    let source_area = ICON_SOURCE * ICON_SOURCE;
    let start: usize = ICON_SIZES.iter().take_while(|&&candidate| candidate != ICON_SOURCE).map(|candidate| 3 * candidate * candidate).sum();
    let (origin_x, origin_y, side) = ICON_CROP;
    let step = side / (size * ICON_SUPERSAMPLE) as f32;
    let sample = |layer: usize, x: f32, y: f32| {
        let column = (x as usize).min(ICON_SOURCE - 1);
        let row = (y as usize).min(ICON_SOURCE - 1);
        f32::from(ICON_LAYERS[start + layer * source_area + row * ICON_SOURCE + column]) / 255.0
    };
    let mut pixels = vec![0u8; size * size * 4];
    for pixel in 0..size * size {
        let (column, row) = (pixel % size, pixel / size);
        let (mut color, mut alpha) = ([0f32; 3], 0f32);
        for (layer, tint) in tints.iter().enumerate() {
            let mut sum = 0f32;
            for dy in 0..ICON_SUPERSAMPLE {
                for dx in 0..ICON_SUPERSAMPLE {
                    let x = origin_x + ((column * ICON_SUPERSAMPLE + dx) as f32 + 0.5) * step;
                    let y = origin_y + ((row * ICON_SUPERSAMPLE + dy) as f32 + 0.5) * step;
                    sum += sample(layer, x, y);
                }
            }
            let coverage = sum / (ICON_SUPERSAMPLE * ICON_SUPERSAMPLE) as f32 * ICON_OPACITY[layer];
            let out = coverage + alpha * (1.0 - coverage);
            if out > 0.0 {
                for channel in 0..3 {
                    color[channel] = (f32::from(tint[channel]) * coverage + color[channel] * alpha * (1.0 - coverage)) / out;
                }
            }
            alpha = out;
        }
        pixels[pixel * 4..pixel * 4 + 4].copy_from_slice(&[color[2] as u8, color[1] as u8, color[0] as u8, (alpha * 255.0) as u8]);
    }
    pixels
}

// The title bar is tinted with the accent, so its icon is drawn in the background colour; the taskbar
// keeps the foreground outline with the accent panel.
// shortcut: the title bar assumes an accent-tinted frame, revisit if Winarchy stops tinting it.
#[cfg(windows)]
pub fn apply_window_icon(colors: Option<[[u8; 3]; 3]>, wait_for_window: bool) {
    use windows::Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        System::Threading::GetCurrentProcessId,
        UI::WindowsAndMessaging::{
            CreateIcon, DestroyIcon, EnumWindows, GetSystemMetrics, GetWindowThreadProcessId, ICON_BIG, ICON_SMALL, IsWindowVisible,
            SM_CXICON, SM_CXSMICON, SendMessageW, WM_SETICON,
        },
    };
    use windows::core::BOOL;

    let [foreground, accent, background] = colors.unwrap_or(DEFAULT_ICON_COLORS);
    let variants = [(ICON_BIG, SM_CXICON, [foreground, accent, foreground]), (ICON_SMALL, SM_CXSMICON, [background, background, background])];

    unsafe extern "system" fn set(window: HWND, data: LPARAM) -> BOOL {
        let mut process = 0;
        // SAFETY: plain Win32 queries on a window handle supplied by EnumWindows; `data` points at `variants` below
        unsafe {
            GetWindowThreadProcessId(window, Some(&mut process));
            if process != GetCurrentProcessId() || !IsWindowVisible(window).as_bool() {
                return true.into();
            }
            let variants = &*(data.0 as *const [(u32, windows::Win32::UI::WindowsAndMessaging::SYSTEM_METRICS_INDEX, [[u8; 3]; 3]); 2]);
            for (kind, metric, tints) in variants {
                let wanted = GetSystemMetrics(*metric) as usize;
                let size = ICON_SIZES.into_iter().find(|&candidate| candidate >= wanted).unwrap_or(48);
                let pixels = icon_pixels(size, *tints);
                let mask = vec![0u8; (size + 15) / 16 * 2 * size];
                if let Ok(icon) = CreateIcon(None, size as i32, size as i32, 1, 32, mask.as_ptr(), pixels.as_ptr()) {
                    let previous = SendMessageW(window, WM_SETICON, Some(WPARAM(*kind as usize)), Some(LPARAM(icon.0 as isize)));
                    if previous.0 != 0 {
                        let _ = DestroyIcon(windows::Win32::UI::WindowsAndMessaging::HICON(previous.0 as *mut _));
                    }
                }
            }
        }
        false.into()
    }

    let run = move || {
        let data = LPARAM(&variants as *const _ as isize);
        // SAFETY: `set` matches WNDENUMPROC and `variants` outlives the call
        unsafe { EnumWindows(Some(set), data) }.is_ok()
    };
    if wait_for_window {
        std::thread::spawn(move || {
            for _ in 0..50 {
                if !run() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        });
    } else {
        run();
    }
}

#[cfg(not(windows))]
pub fn apply_window_icon(_colors: Option<[[u8; 3]; 3]>, _wait_for_window: bool) {}

// komorebi draws each window's border as a click-catching window stacked just below it, but a window it tiles right after
// it appears can end up under that border, and then every click lands on the border until something reorders the stack.
// So while the window is visible, if what sits over its centre is a komorebi border, raise the window above it (without activating).
#[cfg(windows)]
pub fn stay_above_tiling_border() {
    use windows::Win32::{
        Foundation::{HWND, LPARAM, POINT, RECT},
        System::Threading::GetCurrentProcessId,
        UI::WindowsAndMessaging::{
            EnumWindows, GA_ROOT, GetAncestor, GetClassNameW, GetWindowRect, GetWindowThreadProcessId, HWND_NOTOPMOST, HWND_TOPMOST, IsIconic, IsWindowVisible, SWP_NOACTIVATE,
            SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, WindowFromPoint,
        },
    };
    use windows::core::BOOL;
    const POLL: std::time::Duration = std::time::Duration::from_millis(400);

    unsafe extern "system" fn find(window: HWND, data: LPARAM) -> BOOL {
        let mut process = 0;
        // SAFETY: plain Win32 queries on a window handle supplied by EnumWindows; `data` points at an HWND slot
        unsafe {
            GetWindowThreadProcessId(window, Some(&mut process));
            if process == GetCurrentProcessId() && IsWindowVisible(window).as_bool() && GetWindowRect(window, &mut RECT::default()).is_ok() {
                *(data.0 as *mut HWND) = window;
                return false.into();
            }
        }
        true.into()
    }

    std::thread::spawn(|| loop {
        std::thread::sleep(POLL);
        unsafe {
            let mut window = HWND::default();
            let _ = EnumWindows(Some(find), LPARAM(&mut window as *mut HWND as isize));
            if window.0.is_null() || IsIconic(window).as_bool() {
                continue;
            }
            let mut rect = RECT::default();
            if GetWindowRect(window, &mut rect).is_err() {
                continue;
            }
            let covering = GetAncestor(WindowFromPoint(POINT { x: (rect.left + rect.right) / 2, y: (rect.top + rect.bottom) / 2 }), GA_ROOT);
            if covering == window || covering.0.is_null() {
                continue;
            }
            let mut name = [0u16; 32];
            let length = GetClassNameW(covering, &mut name) as usize;
            if String::from_utf16_lossy(&name[..length]).starts_with("komoborder") {
                // HWND_TOP alone leaves the window under the border (verified live); a topmost round trip does reorder it
                let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
                let _ = SetWindowPos(window, Some(HWND_TOPMOST), 0, 0, 0, 0, flags);
                let _ = SetWindowPos(window, Some(HWND_NOTOPMOST), 0, 0, 0, 0, flags);
            }
        }
    });
}

#[cfg(not(windows))]
pub fn stay_above_tiling_border() {}

// The classic Explorer menu, including third-party entries; shown at the pointer and run when one is chosen.
#[cfg(windows)]
pub fn native_menu(paths: &[PathBuf], background: bool) -> Result<bool, String> {
    use windows::{
        Win32::{
            Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
            UI::{
                Shell::{
                    BHID_SFUIObject, CMF_NORMAL, CMINVOKECOMMANDINFO, DefSubclassProc, IContextMenu, IContextMenu2, IShellFolder,
                    SHBindToObject, SHCreateShellItemArrayFromIDLists, SHParseDisplayName, SetWindowSubclass, RemoveWindowSubclass,
                },
                WindowsAndMessaging::{
                    CreatePopupMenu, DestroyMenu, GetCursorPos, GetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu,
                    WM_DRAWITEM, WM_INITMENUPOPUP, WM_MEASUREITEM, WM_MENUCHAR,
                },
            },
        },
        core::{Interface, PCSTR, PCWSTR},
    };

    // Submenus such as "Open with" and "Send to" fill themselves in through these menu messages.
    unsafe extern "system" fn forward(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, data: usize) -> LRESULT {
        if matches!(message, WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR) {
            let raw = data as *mut std::ffi::c_void;
            if let Some(menu) = unsafe { IContextMenu2::from_raw_borrowed(&raw) }
                && unsafe { menu.HandleMenuMsg(message, wparam, lparam) }.is_ok()
            {
                return LRESULT(0);
            }
        }
        unsafe { DefSubclassProc(window, message, wparam, lparam) }
    }

    if paths.is_empty() {
        return Ok(false);
    }
    unsafe {
        let mut identifiers = Vec::new();
        for path in paths {
            let wide: Vec<u16> = path.as_os_str().to_string_lossy().encode_utf16().chain(Some(0)).collect();
            let mut identifier = std::ptr::null_mut();
            SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut identifier, 0, None).map_err(|error| error.to_string())?;
            identifiers.push(identifier as *const _);
        }
        let window = GetForegroundWindow();
        let menu: IContextMenu = if background {
            // the menu of the empty area of the folder: New, Paste, Properties and the extensions registered for it
            let folder: IShellFolder = SHBindToObject(None::<&IShellFolder>, identifiers[0], None).map_err(|error| error.to_string())?;
            folder.CreateViewObject(window).map_err(|error| error.to_string())?
        } else {
            let items = SHCreateShellItemArrayFromIDLists(&identifiers).map_err(|error| error.to_string())?;
            items.BindToHandler(None, &BHID_SFUIObject).map_err(|error| error.to_string())?
        };
        let popup = CreatePopupMenu().map_err(|error| error.to_string())?;
        let outcome = (|| -> Result<bool, String> {
            menu.QueryContextMenu(popup, 0, 1, 0x7FFF, CMF_NORMAL).ok().map_err(|error| error.to_string())?;
            let second = menu.cast::<IContextMenu2>().ok();
            if let Some(second) = &second {
                let _ = SetWindowSubclass(window, Some(forward), 1, second.as_raw() as usize);
            }
            let mut point = POINT::default();
            let _ = GetCursorPos(&mut point);
            let chosen = TrackPopupMenu(popup, TPM_RETURNCMD | TPM_RIGHTBUTTON, point.x, point.y, None, window, None);
            if second.is_some() {
                let _ = RemoveWindowSubclass(window, Some(forward), 1);
            }
            if chosen.0 == 0 {
                return Ok(false);
            }
            let info = CMINVOKECOMMANDINFO {
                cbSize: std::mem::size_of::<CMINVOKECOMMANDINFO>() as u32,
                fMask: 0,
                hwnd: window,
                lpVerb: PCSTR((chosen.0 - 1) as usize as *const u8),
                nShow: 1,
                ..Default::default()
            };
            menu.InvokeCommand(&info).map_err(|error| error.to_string())?;
            Ok(true)
        })();
        let _ = DestroyMenu(popup);
        outcome
    }
}

#[cfg(not(windows))]
pub fn native_menu(_paths: &[PathBuf], _background: bool) -> Result<bool, String> {
    Err("not supported on this platform".to_owned())
}

// Ejects a removable drive the way Explorer does: through the drive's own shell "eject" verb.
#[cfg(windows)]
pub fn eject(path: &Path) -> Result<(), String> {
    use windows::{
        Win32::UI::{
            Shell::{BHID_SFUIObject, CMF_NORMAL, CMINVOKECOMMANDINFO, IContextMenu, SHCreateShellItemArrayFromIDLists, SHParseDisplayName},
            WindowsAndMessaging::{CreatePopupMenu, DestroyMenu, GetForegroundWindow},
        },
        core::{PCSTR, PCWSTR},
    };

    unsafe {
        let wide: Vec<u16> = path.as_os_str().to_string_lossy().encode_utf16().chain(Some(0)).collect();
        let mut identifier = std::ptr::null_mut();
        SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut identifier, 0, None).map_err(|error| error.to_string())?;
        let items = SHCreateShellItemArrayFromIDLists(&[identifier as *const _]).map_err(|error| error.to_string())?;
        let menu: IContextMenu = items.BindToHandler(None, &BHID_SFUIObject).map_err(|error| error.to_string())?;
        let popup = CreatePopupMenu().map_err(|error| error.to_string())?;
        let outcome = (|| -> Result<(), String> {
            menu.QueryContextMenu(popup, 0, 1, 0x7FFF, CMF_NORMAL).ok().map_err(|error| error.to_string())?;
            let info = CMINVOKECOMMANDINFO {
                cbSize: std::mem::size_of::<CMINVOKECOMMANDINFO>() as u32,
                hwnd: GetForegroundWindow(),
                lpVerb: PCSTR(b"eject\0".as_ptr()),
                nShow: 1,
                ..Default::default()
            };
            menu.InvokeCommand(&info).map_err(|error| error.to_string())
        })();
        let _ = DestroyMenu(popup);
        outcome
    }
}

#[cfg(not(windows))]
pub fn eject(_path: &Path) -> Result<(), String> {
    Err("not supported on this platform".to_owned())
}

// A small click-through pill that follows the pointer while something is dragged, so the object seems to travel with it.
#[cfg(windows)]
struct Ghost {
    window: windows::Win32::Foundation::HWND,
    bitmap: windows::Win32::Graphics::Gdi::HBITMAP,
    context: windows::Win32::Graphics::Gdi::HDC,
}

#[cfg(windows)]
impl Ghost {
    const OPACITY: u8 = 225;
    const PADDING: i32 = 10;
    const OFFSET: i32 = 14;

    fn new(label: &str, foreground: [u8; 3], fill: [u8; 3]) -> Option<Self> {
        use windows::{
            Win32::{
                Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM},
                Graphics::Gdi::{
                    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, CreateFontW, CreateRoundRectRgn, DIB_RGB_COLORS, DT_NOPREFIX,
                    DT_SINGLELINE, DT_VCENTER, DrawTextW, GetDC, GetTextExtentPoint32W, ReleaseDC, SelectObject, SetBkMode, SetWindowRgn, SetTextColor, TRANSPARENT,
                },
                System::LibraryLoader::GetModuleHandleW,
                UI::{
                    HiDpi::GetDpiForSystem,
                    WindowsAndMessaging::{
                        CreateWindowExW, DefWindowProcW, RegisterClassW, ULW_ALPHA, UpdateLayeredWindow, WNDCLASSW,
                        WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
                    },
                },
            },
            core::w,
        };
        use windows::Win32::Graphics::Gdi::BLENDFUNCTION;

        unsafe extern "system" fn procedure(window: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
            unsafe { DefWindowProcW(window, message, w, l) }
        }

        unsafe {
            let instance = GetModuleHandleW(None).ok()?;
            let class = WNDCLASSW { lpfnWndProc: Some(procedure), hInstance: instance.into(), lpszClassName: w!("WinarchyDragGhost"), ..Default::default() };
            RegisterClassW(&class);
            let scale = GetDpiForSystem() as i32;
            let screen = GetDC(None);
            let context = CreateCompatibleDC(Some(screen));
            let font = CreateFontW(-(15 * scale / 96), 0, 0, 0, 600, 0, 0, 0, Default::default(), Default::default(), Default::default(), Default::default(), 0, w!("Segoe UI Variable Text"));
            let previous = SelectObject(context, font.into());
            let text: Vec<u16> = label.encode_utf16().collect();
            let mut extent = SIZE::default();
            let _ = GetTextExtentPoint32W(context, &text, &mut extent);
            let padding = Self::PADDING * scale / 96;
            let (width, height) = (extent.cx + 2 * padding, extent.cy + padding);
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: width, biHeight: -height, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let bitmap = CreateDIBSection(Some(context), &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
            SelectObject(context, bitmap.into());
            let pixels = std::slice::from_raw_parts_mut(bits as *mut u8, (width * height * 4) as usize);
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.copy_from_slice(&[fill[2], fill[1], fill[0], 255]);
            }
            SelectObject(context, font.into());
            SetBkMode(context, TRANSPARENT);
            SetTextColor(context, COLORREF(u32::from(foreground[0]) | u32::from(foreground[1]) << 8 | u32::from(foreground[2]) << 16));
            let mut rect = windows::Win32::Foundation::RECT { left: 0, top: 0, right: width, bottom: height };
            let mut line = text.clone();
            DrawTextW(context, &mut line, &mut rect, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | windows::Win32::Graphics::Gdi::DT_CENTER);
            SelectObject(context, previous);
            let window = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                w!("WinarchyDragGhost"), None, WS_POPUP, 0, 0, width, height, None, None, Some(instance.into()), None,
            ).ok()?;
            let radius = height;
            SetWindowRgn(window, Some(CreateRoundRectRgn(0, 0, width + 1, height + 1, radius, radius)), false);
            let blend = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: Self::OPACITY, AlphaFormat: 0 };
            let (origin, size) = (POINT::default(), SIZE { cx: width, cy: height });
            let _ = UpdateLayeredWindow(window, Some(screen), None, Some(&size), Some(context), Some(&origin), COLORREF(0), Some(&blend), ULW_ALPHA);
            ReleaseDC(None, screen);
            Some(Self { window, bitmap, context })
        }
    }

    fn follow(&self, x: i32, y: i32) {
        use windows::Win32::UI::WindowsAndMessaging::{HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOSIZE, SWP_SHOWWINDOW, SetWindowPos};
        let offset = Self::OFFSET;
        let _ = unsafe { SetWindowPos(self.window, Some(HWND_TOPMOST), x + offset, y + offset, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW) };
    }
}

#[cfg(windows)]
impl Drop for Ghost {
    fn drop(&mut self) {
        use windows::Win32::Graphics::Gdi::{DeleteDC, DeleteObject};
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.window);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.context);
        }
    }
}

// Blocks while the left button is held; true once the pointer leaves the system drag rectangle, false if it is released first.
#[cfg(windows)]
pub fn drag_started() -> bool {
    use windows::Win32::{
        Foundation::POINT,
        UI::{
            Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON},
            WindowsAndMessaging::{GetCursorPos, GetSystemMetrics, SM_CXDRAG, SM_CYDRAG},
        },
    };
    unsafe {
        let mut start = POINT::default();
        let _ = GetCursorPos(&mut start);
        let (limit_x, limit_y) = (GetSystemMetrics(SM_CXDRAG), GetSystemMetrics(SM_CYDRAG));
        while GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0 {
            let mut now = POINT::default();
            let _ = GetCursorPos(&mut now);
            if (now.x - start.x).abs() >= limit_x || (now.y - start.y).abs() >= limit_y {
                return true;
            }
            std::thread::sleep(crate::style::DRAG_POLL);
        }
        false
    }
}

// Shows the hand cursor and a label that follows the pointer until the left button is released; returns where it was released and whether Ctrl was down just before.
#[cfg(windows)]
pub fn follow_drag(label: &str, colors: Option<[[u8; 3]; 3]>) -> ((i32, i32), bool) {
    use windows::Win32::{
        Foundation::POINT,
        UI::{
            Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON},
            WindowsAndMessaging::{
                CopyIcon, DispatchMessageW, GetCursorPos, HCURSOR, HICON, IDC_HAND, LoadCursorW, MSG, OCR_NORMAL, PM_REMOVE, PeekMessageW, SPI_SETCURSORS,
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetSystemCursor, SystemParametersInfoW,
            },
        },
    };
    let [_, accent, background] = colors.unwrap_or(DEFAULT_ICON_COLORS);
    let ghost = Ghost::new(label, background, accent);
    let mut point = POINT::default();
    let mut ctrl = false;
    unsafe {
        // the system arrow becomes the hand while dragging; the user's cursor scheme is reloaded on release
        if let Ok(hand) = LoadCursorW(None, IDC_HAND).and_then(|cursor| CopyIcon(HICON(cursor.0))) {
            let _ = SetSystemCursor(HCURSOR(hand.0), OCR_NORMAL);
        }
        while GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0 {
            let _ = GetCursorPos(&mut point);
            ctrl = crate::keys::ctrl_down();
            if let Some(ghost) = &ghost {
                ghost.follow(point.x, point.y);
            }
            // the ghost window belongs to this thread, so its messages must keep flowing
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = DispatchMessageW(&message);
            }
            std::thread::sleep(crate::style::DRAG_POLL);
        }
        let _ = SystemParametersInfoW(SPI_SETCURSORS, 0, None, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
        let _ = GetCursorPos(&mut point);
    }
    ((point.x, point.y), ctrl)
}

#[cfg(not(windows))]
pub fn drag_started() -> bool {
    false
}

#[cfg(not(windows))]
pub fn follow_drag(_label: &str, _colors: Option<[[u8; 3]; 3]>) -> ((i32, i32), bool) {
    ((0, 0), false)
}

// Starts a real XAML drag of `paths` from the element the latest pointer event came from, so other apps can receive the files.
#[cfg(windows)]
pub fn xaml_drag(paths: &[PathBuf]) -> Result<(), String> {
    use crate::xaml_drag::{DragStartingEventArgs, PointerRoutedEventArgs, UIElement};
    use windows::{
        ApplicationModel::DataTransfer::DataPackageOperation,
        Foundation::TypedEventHandler,
        Storage::{IStorageItem, StorageFile, StorageFolder},
        core::{HSTRING, IUnknown, Interface},
    };
    use windows_collections::IIterable;

    let failed = |error: windows::core::Error| error.message();
    let (element, arguments) = windows_reactor::take_last_pointer().ok_or("no pointer press to drag from")?;
    let (element, arguments) = unsafe { (IUnknown::from_raw(element), IUnknown::from_raw(arguments)) };
    let element: UIElement = element.cast().map_err(failed)?;
    let arguments: PointerRoutedEventArgs = arguments.cast().map_err(failed)?;
    let point = arguments.GetCurrentPoint(&element).map_err(failed)?;
    let paths = paths.to_vec();
    element
        .DragStarting(&TypedEventHandler::new(move |_, arguments: windows::core::Ref<DragStartingEventArgs>| {
            if let Some(arguments) = arguments.as_ref() {
                arguments.SetAllowedOperations(DataPackageOperation::Copy | DataPackageOperation::Move | DataPackageOperation::Link)?;
                let mut items = Vec::new();
                for path in &paths {
                    let name = HSTRING::from(path.to_string_lossy().as_ref());
                    items.push(Some(if path.is_dir() {
                        StorageFolder::GetFolderFromPathAsync(&name)?.join()?.cast::<IStorageItem>()?
                    } else {
                        StorageFile::GetFileFromPathAsync(&name)?.join()?.cast::<IStorageItem>()?
                    }));
                }
                arguments.Data()?.SetStorageItems(&IIterable::<IStorageItem>::from(items), true)?;
            }
            Ok(())
        }))
        .map_err(failed)?;
    element.StartDragAsync(&point).map_err(failed)?;
    Ok(())
}

#[cfg(not(windows))]
pub fn xaml_drag(_paths: &[PathBuf]) -> Result<(), String> {
    Err("not supported on this platform".to_owned())
}

// The caption buttons of the extended title bar keep the system colours unless told otherwise; tints them with the theme. False until the title bar exists.
pub fn style_caption_buttons(colors: [[u8; 3]; 3]) -> bool {
    #[cfg(windows)]
    return windows_reactor::style_caption_buttons(colors[0]);
    #[cfg(not(windows))]
    return true;
}

// Expands %VARS% and resolves `shell:` names to a folder path; anything else is taken as typed.
// shortcut: `shell:` locations that are not file system folders (Recycle Bin, This PC) fall back to the typed text.
#[cfg(windows)]
pub fn resolve_address(text: &str) -> PathBuf {
    use windows::{
        Win32::{
            Foundation::MAX_PATH,
            System::Com::CoTaskMemFree,
            UI::Shell::{Common::ITEMIDLIST, SHGetPathFromIDListW, SHParseDisplayName},
        },
        core::HSTRING,
    };
    let expanded = expand_variables(text.trim().trim_matches('"'));
    if expanded.len() > 6 && expanded[..6].eq_ignore_ascii_case("shell:") {
        unsafe {
            let mut list: *mut ITEMIDLIST = std::ptr::null_mut();
            if SHParseDisplayName(&HSTRING::from(expanded.as_str()), None, &mut list, 0, None).is_ok() {
                let mut buffer = [0u16; MAX_PATH as usize];
                let found = SHGetPathFromIDListW(list, &mut buffer).as_bool();
                CoTaskMemFree(Some(list.cast()));
                if found {
                    return PathBuf::from(String::from_utf16_lossy(&buffer[..buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len())]));
                }
            }
        }
    }
    PathBuf::from(expanded)
}

#[cfg(windows)]
fn expand_variables(text: &str) -> String {
    use windows::{Win32::System::Environment::ExpandEnvironmentStringsW, core::HSTRING};
    let source = HSTRING::from(text);
    let needed = unsafe { ExpandEnvironmentStringsW(&source, None) } as usize;
    if needed == 0 {
        return text.to_owned();
    }
    let mut buffer = vec![0u16; needed];
    let written = unsafe { ExpandEnvironmentStringsW(&source, Some(&mut buffer)) } as usize;
    String::from_utf16_lossy(&buffer[..written.saturating_sub(1)])
}

#[cfg(not(windows))]
pub fn resolve_address(text: &str) -> PathBuf {
    PathBuf::from(text)
}


#[cfg(all(test, windows))]
mod address_tests {
    use super::resolve_address;

    #[test]
    fn expands_variables_and_known_folders() {
        unsafe { let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED); }
        let root = std::env::var("SystemRoot").unwrap();
        assert_eq!(resolve_address(r"%SystemRoot%\System32"), std::path::PathBuf::from(format!(r"{root}\System32")));
        assert!(resolve_address("shell:Downloads").ends_with("Downloads"));
        assert_eq!(resolve_address(r"C:\Temp"), std::path::PathBuf::from(r"C:\Temp"));
        assert_eq!(resolve_address(r#"  "C:\Temp"  "#), std::path::PathBuf::from(r"C:\Temp"));
    }
}

#[cfg(all(test, windows))]
mod long_path_tests {
    use super::{create_item, transfer};

    #[test]
    fn copies_and_creates_beyond_260_characters() {
        let base = std::env::temp_dir().join(format!("wf-long-{}", std::process::id()));
        let mut deep = base.clone();
        for _ in 0..12 {
            deep.push("d".repeat(30));
        }
        std::fs::create_dir_all(&deep).unwrap();
        assert!(deep.as_os_str().len() > 260);
        let source = base.join("origen.txt");
        std::fs::write(&source, "x").unwrap();
        let copied = transfer(&[source], &deep, false);
        let created = create_item(&deep.join("nuevo.txt"), false);
        let seen = (deep.join("origen.txt").exists(), deep.join("nuevo.txt").exists());
        let _ = std::fs::remove_dir_all(&base);
        assert!(copied.is_ok(), "copy: {copied:?}");
        assert!(created.is_ok(), "create: {created:?}");
        assert_eq!(seen, (true, true));
    }
}

#[cfg(all(test, windows))]
mod elevated_tests {
    use super::create_item;
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};

    // writes under Program Files, so it only passes where elevation is silent or the prompt is accepted
    #[test]
    #[ignore]
    fn creates_in_protected_folder_through_elevation() {
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let folder = std::path::Path::new(r"C:\Program Files").join(format!("winarchy-uac-{}", std::process::id()));
        let result = create_item(&folder, true);
        assert!(result.is_ok(), "create: {result:?}");
        assert!(folder.is_dir());
        println!("{}", folder.display());
    }
}

// Programs registered for the file's type, as (display name, executable); packaged apps without an executable path are left out.
#[cfg(windows)]
pub fn open_with_handlers(path: &std::path::Path) -> Vec<(String, std::path::PathBuf)> {
    use windows::{
        Win32::{
            System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree},
            UI::Shell::{ASSOC_FILTER_RECOMMENDED, IAssocHandler, SHAssocEnumHandlers},
        },
        core::{HSTRING, PWSTR},
    };
    let Some(extension) = path.extension() else { return Vec::new() };
    let read = |text: PWSTR| {
        let value = unsafe { text.to_string().ok() };
        unsafe { CoTaskMemFree(Some(text.0.cast())) };
        value
    };
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let Ok(handlers) = SHAssocEnumHandlers(&HSTRING::from(format!(".{}", extension.to_string_lossy())), ASSOC_FILTER_RECOMMENDED) else { return Vec::new() };
        let mut found = Vec::new();
        loop {
            let mut batch: [Option<IAssocHandler>; 1] = [None];
            let mut fetched = 0u32;
            if handlers.Next(&mut batch, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            let Some(handler) = batch[0].take() else { break };
            let (Ok(name), Ok(label)) = (handler.GetName(), handler.GetUIName()) else { continue };
            if let (Some(exe), Some(label)) = (read(name), read(label))
                && std::path::Path::new(&exe).is_file()
            {
                found.push((label, std::path::PathBuf::from(exe)));
            }
        }
        found
    }
}

#[cfg(not(windows))]
pub fn open_with_handlers(_path: &std::path::Path) -> Vec<(String, std::path::PathBuf)> {
    Vec::new()
}

// Screen position in physical pixels.
#[cfg(windows)]
pub fn cursor_pos() -> (i32, i32) {
    use windows::Win32::{Foundation::POINT, UI::WindowsAndMessaging::GetCursorPos};
    let mut point = POINT::default();
    // SAFETY: `point` is a valid out pointer for the duration of the call.
    unsafe {
        let _ = GetCursorPos(&mut point);
    }
    (point.x, point.y)
}

#[cfg(not(windows))]
pub fn cursor_pos() -> (i32, i32) {
    (0, 0)
}

// Physical pixels per layout unit on the primary display.
#[cfg(windows)]
pub fn dip_scale() -> f64 {
    f64::from(unsafe { windows::Win32::UI::HiDpi::GetDpiForSystem() }.max(96)) / 96.0
}

#[cfg(not(windows))]
pub fn dip_scale() -> f64 {
    1.0
}
