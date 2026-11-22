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
    let file = wide(file);
    let parameters = parameters.map(wide);
    // Only the "properties" verb needs INVOKEIDLIST; it makes the shell parse the file as an item id list.
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: if verb == "properties" { SEE_MASK_INVOKEIDLIST } else { 0 },
        lpVerb: PCWSTR(operation.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: parameters.as_ref().map_or(PCWSTR::null(), |p| PCWSTR(p.as_ptr())),
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
const DEFAULT_ICON_COLORS: [[u8; 3]; 3] = [[0xe6, 0xd9, 0xdb], [0xf3, 0x8d, 0x70], [0x2c, 0x25, 0x25]];

// BGRA pixels of the icon: back folder, accent panel and front outline tinted and stacked.
fn icon_pixels(size: usize, tints: [[u8; 3]; 3]) -> Vec<u8> {
    let area = size * size;
    let start: usize = ICON_SIZES.iter().take_while(|&&candidate| candidate != size).map(|candidate| 3 * candidate * candidate).sum();
    let mut pixels = vec![0u8; area * 4];
    for pixel in 0..area {
        let (mut color, mut alpha) = ([0f32; 3], 0f32);
        for (layer, tint) in tints.iter().enumerate() {
            let coverage = f32::from(ICON_LAYERS[start + layer * area + pixel]) / 255.0 * ICON_OPACITY[layer];
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

// The classic Explorer menu, including third-party entries; shown at the pointer and run when one is chosen.
#[cfg(windows)]
pub fn native_menu(paths: &[PathBuf]) -> Result<bool, String> {
    use windows::{
        Win32::{
            Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
            UI::{
                Shell::{
                    BHID_SFUIObject, CMF_NORMAL, CMINVOKECOMMANDINFO, DefSubclassProc, IContextMenu, IContextMenu2,
                    SHCreateShellItemArrayFromIDLists, SHParseDisplayName, SetWindowSubclass, RemoveWindowSubclass,
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
        let items = SHCreateShellItemArrayFromIDLists(&identifiers).map_err(|error| error.to_string())?;
        let menu: IContextMenu = items.BindToHandler(None, &BHID_SFUIObject).map_err(|error| error.to_string())?;
        let popup = CreatePopupMenu().map_err(|error| error.to_string())?;
        let window = GetForegroundWindow();
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
pub fn native_menu(_paths: &[PathBuf]) -> Result<bool, String> {
    Err("not supported on this platform".to_owned())
}
