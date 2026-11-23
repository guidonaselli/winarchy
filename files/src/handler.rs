// Makes Files the program Windows starts for folders (Win+R, `start`, shortcuts to folders) by writing per-user class
// overrides. Machine-wide keys are never touched, and the previous state is saved so it can be put back exactly.

const CLASSES: [&str; 3] = ["Folder", "Directory", "Drive"];
const USER_CLASSES: &str = r"Software\Classes";
const BACKUP: &str = r"Software\Winarchy\Files\HandlerBackup";
// stored for a value that did not exist, so restoring can tell it apart from an empty one
const ABSENT: &str = "\u{0}absent";

#[cfg(windows)]
mod registry {
    use windows::{
        Win32::{
            Foundation::ERROR_SUCCESS,
            System::Registry::{
                HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteKeyW, RegDeleteTreeW,
                RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
            },
        },
        core::PCWSTR,
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    pub fn set(key: &str, name: &str, value: &str) -> Result<(), String> {
        unsafe {
            let mut handle = HKEY::default();
            RegCreateKeyExW(HKEY_CURRENT_USER, PCWSTR(wide(key).as_ptr()), None, PCWSTR::null(), REG_OPTION_NON_VOLATILE, KEY_WRITE, None, &mut handle, None)
                .ok()
                .map_err(|error| format!("{key}: {error}"))?;
            let data: Vec<u8> = wide(value).iter().flat_map(|unit| unit.to_le_bytes()).collect();
            let result = RegSetValueExW(handle, PCWSTR(wide(name).as_ptr()), None, REG_SZ, Some(&data)).ok().map_err(|error| format!("{key}: {error}"));
            let _ = RegCloseKey(handle);
            result
        }
    }

    pub fn get(key: &str, name: &str) -> Option<String> {
        unsafe {
            let mut handle = HKEY::default();
            if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(wide(key).as_ptr()), None, KEY_READ, &mut handle) != ERROR_SUCCESS {
                return None;
            }
            let mut size = 0u32;
            let name = wide(name);
            let probe = RegQueryValueExW(handle, PCWSTR(name.as_ptr()), None, None, None, Some(&mut size));
            let mut buffer = vec![0u8; size as usize];
            let found = probe == ERROR_SUCCESS
                && RegQueryValueExW(handle, PCWSTR(name.as_ptr()), None, None, Some(buffer.as_mut_ptr()), Some(&mut size)) == ERROR_SUCCESS;
            let _ = RegCloseKey(handle);
            found.then(|| {
                let units: Vec<u16> = buffer[..size as usize].chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect();
                String::from_utf16_lossy(&units).trim_end_matches('\0').to_owned()
            })
        }
    }

    pub fn delete_tree(key: &str) {
        unsafe {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(wide(key).as_ptr()));
        }
    }

    pub fn delete_default(key: &str) {
        unsafe {
            let mut handle = HKEY::default();
            if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(wide(key).as_ptr()), None, KEY_WRITE, &mut handle) == ERROR_SUCCESS {
                let _ = RegDeleteValueW(handle, PCWSTR::null());
                let _ = RegCloseKey(handle);
            }
        }
    }

    // Removes the key only when it has no subkeys, so entries added by other programs survive.
    pub fn delete_if_empty(key: &str) {
        unsafe {
            let _ = RegDeleteKeyW(HKEY_CURRENT_USER, PCWSTR(wide(key).as_ptr()));
        }
    }
}

#[cfg(not(windows))]
mod registry {
    pub fn set(_: &str, _: &str, _: &str) -> Result<(), String> {
        Err("The folder handler is supported only on Windows".to_owned())
    }
    pub fn get(_: &str, _: &str) -> Option<String> {
        None
    }
    pub fn delete_tree(_: &str) {}
    pub fn delete_default(_: &str) {}
    pub fn delete_if_empty(_: &str) {}
}

fn command_line(exe: &std::path::Path) -> String {
    format!("\"{}\" \"%1\"", exe.display())
}

pub fn register(exe: &std::path::Path) -> Result<(), String> {
    register_at(USER_CLASSES, BACKUP, exe)
}

pub fn unregister() -> Result<(), String> {
    unregister_at(USER_CLASSES, BACKUP)
}

pub fn is_registered(exe: &std::path::Path) -> bool {
    CLASSES.iter().all(|class| registry::get(&format!(r"{USER_CLASSES}\{class}\shell\open\command"), "").as_deref() == Some(command_line(exe).as_str()))
}

fn register_at(classes: &str, backup: &str, exe: &std::path::Path) -> Result<(), String> {
    for class in CLASSES {
        let shell = format!(r"{classes}\{class}\shell");
        let command = format!(r"{shell}\open\command");
        let saved = format!(r"{backup}\{class}");
        // a second registration must not replace the saved original with our own entries
        if registry::get(&saved, "shell").is_none() {
            // `shell` goes last: its presence marks a complete backup
            registry::set(&saved, "command", &registry::get(&command, "").unwrap_or_else(|| ABSENT.to_owned()))?;
            registry::set(&saved, "shell", &registry::get(&shell, "").unwrap_or_else(|| ABSENT.to_owned()))?;
        }
        registry::set(&shell, "", "open")?;
        registry::set(&command, "", &command_line(exe))?;
    }
    Ok(())
}

fn unregister_at(classes: &str, backup: &str) -> Result<(), String> {
    for class in CLASSES {
        let shell = format!(r"{classes}\{class}\shell");
        let command = format!(r"{shell}\open\command");
        let saved = format!(r"{backup}\{class}");
        let (Some(shell_default), Some(command_default)) = (registry::get(&saved, "shell"), registry::get(&saved, "command")) else {
            continue;
        };
        if command_default == ABSENT {
            registry::delete_tree(&format!(r"{shell}\open"));
        } else {
            registry::set(&command, "", &command_default)?;
        }
        if shell_default == ABSENT {
            registry::delete_default(&shell);
            registry::delete_if_empty(&shell);
        } else {
            registry::set(&shell, "", &shell_default)?;
        }
        registry::delete_tree(&saved);
    }
    registry::delete_if_empty(backup);
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    const TEST_CLASSES: &str = r"Software\Winarchy\Test\Classes";
    const TEST_BACKUP: &str = r"Software\Winarchy\Test\Backup";

    #[test]
    fn register_then_unregister_restores_the_previous_state() {
        let exe = std::path::Path::new(r"C:\Apps\Files\files.exe");
        registry::delete_tree(r"Software\Winarchy\Test");
        registry::set(&format!(r"{TEST_CLASSES}\Directory\shell\open\command"), "", r#""C:\Other\other.exe" "%1""#).unwrap();

        register_at(TEST_CLASSES, TEST_BACKUP, exe).unwrap();
        register_at(TEST_CLASSES, TEST_BACKUP, exe).unwrap();
        assert_eq!(registry::get(&format!(r"{TEST_CLASSES}\Folder\shell\open\command"), "").as_deref(), Some(r#""C:\Apps\Files\files.exe" "%1""#));
        assert_eq!(registry::get(&format!(r"{TEST_CLASSES}\Drive\shell"), "").as_deref(), Some("open"));

        unregister_at(TEST_CLASSES, TEST_BACKUP).unwrap();
        assert_eq!(registry::get(&format!(r"{TEST_CLASSES}\Directory\shell\open\command"), "").as_deref(), Some(r#""C:\Other\other.exe" "%1""#));
        assert_eq!(registry::get(&format!(r"{TEST_CLASSES}\Folder\shell\open\command"), ""), None);
        assert_eq!(registry::get(&format!(r"{TEST_CLASSES}\Drive\shell"), ""), None);
        assert_eq!(registry::get(&format!(r"{TEST_BACKUP}\Folder"), "shell"), None);
        registry::delete_tree(r"Software\Winarchy\Test");
    }
}
