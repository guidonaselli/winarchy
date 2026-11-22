use std::path::{Path, PathBuf};

#[cfg(windows)]
pub fn open(path: &Path) -> Result<(), String> {
    use std::{ffi::c_void, os::windows::ffi::OsStrExt};

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(
            window: *mut c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show_command: i32,
        ) -> isize;
    }

    let operation: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
    let file: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        )
    };
    if result > 32 {
        Ok(())
    } else {
        Err(std::io::Error::from_raw_os_error(result as i32).to_string())
    }
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
            flags: 0x0040,
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
pub fn recycle(_paths: &[std::path::PathBuf]) -> Result<bool, String> {
    Err("Recycle Bin is supported only on Windows".to_owned())
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
