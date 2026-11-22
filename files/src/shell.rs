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
