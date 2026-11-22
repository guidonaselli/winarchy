use serde::Deserialize;
use windows_reactor::{Brush, Color, ThemeBrush, WindowTheme};

#[derive(Clone, Deserialize)]
pub struct Theme {
    background: String,
    foreground: String,
    accent: String,
    urgent: String,
    muted: String,
    #[serde(default)]
    mode: Option<ThemeMode>,
    #[serde(default)]
    surface: Option<String>,
    #[serde(default)]
    selection: Option<String>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ThemeMode {
    Dark,
    Light,
}

impl Theme {
    pub fn window_theme(&self) -> WindowTheme {
        match self.mode {
            Some(ThemeMode::Dark) => WindowTheme::Dark,
            Some(ThemeMode::Light) => WindowTheme::Light,
            None => WindowTheme::System,
        }
    }

    fn path() -> Option<std::path::PathBuf> {
        if let Some(path) = std::env::var_os("WINARCHY_FILES_THEME") {
            Some(path.into())
        } else {
            Some(
                std::env::current_exe()
                    .ok()?
                    .parent()?
                    .join("config/files/theme.json"),
            )
        }
    }

    pub fn load() -> Result<Option<Self>, String> {
        let Some(path) = Self::path() else {
            return Ok(None);
        };
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        let theme: Self = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        let valid = [
            theme.background.as_str(),
            theme.foreground.as_str(),
            theme.accent.as_str(),
            theme.urgent.as_str(),
            theme.muted.as_str(),
        ]
        .into_iter()
        .all(|value| color(value).is_some())
            && [theme.surface.as_deref(), theme.selection.as_deref()]
                .into_iter()
                .flatten()
                .all(|value| color(value).is_some());
        valid
            .then_some(theme)
            .ok_or_else(|| "theme contains an invalid color".to_owned())
            .map(Some)
    }

    #[cfg(windows)]
    pub fn start_watching() -> Result<usize, String> {
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            Win32::Storage::FileSystem::{
                FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE,
                FindFirstChangeNotificationW,
            },
            core::PCWSTR,
        };

        let directory = Self::path()
            .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
            .ok_or_else(|| "theme path has no parent directory".to_owned())?;
        let wide: Vec<_> = directory
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe {
            FindFirstChangeNotificationW(
                PCWSTR(wide.as_ptr()),
                false,
                FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_LAST_WRITE,
            )
        }
        .map_err(|error| error.to_string())?;
        Ok(handle.0 as usize)
    }

    #[cfg(windows)]
    pub fn wait_for_change(
        handle: usize,
        cancel: windows_reactor::CancellationToken,
    ) -> Result<usize, String> {
        use windows::Win32::{
            Foundation::{HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
            Storage::FileSystem::FindCloseChangeNotification,
            System::Threading::WaitForSingleObject,
        };

        let handle = HANDLE(handle as *mut _);
        loop {
            if cancel.is_cancelled() {
                let _ = unsafe { FindCloseChangeNotification(handle) };
                return Err("theme watch cancelled".to_owned());
            }
            match unsafe { WaitForSingleObject(handle, 1000) } {
                WAIT_OBJECT_0 => return Ok(handle.0 as usize),
                WAIT_TIMEOUT => {}
                _ => {
                    let _ = unsafe { FindCloseChangeNotification(handle) };
                    return Err("WaitForSingleObject failed".to_owned());
                }
            }
        }
    }

    #[cfg(windows)]
    pub fn rearm_watcher(handle: usize) -> Result<(), String> {
        use windows::Win32::Storage::FileSystem::FindNextChangeNotification;

        unsafe { FindNextChangeNotification(windows::Win32::Foundation::HANDLE(handle as *mut _)) }
            .map_err(|error| error.to_string())
    }

    #[cfg(windows)]
    pub fn close_watcher(handle: usize) {
        use windows::Win32::Storage::FileSystem::FindCloseChangeNotification;

        let _ = unsafe {
            FindCloseChangeNotification(windows::Win32::Foundation::HANDLE(handle as *mut _))
        };
    }

    pub fn background(&self) -> Brush {
        brush(&self.background, ThemeBrush::SolidBackground)
    }
    pub fn foreground(&self) -> Brush {
        brush(&self.foreground, ThemeBrush::PrimaryText)
    }
    pub fn surface(&self) -> Brush {
        self.surface.as_deref().map_or_else(
            || self.background(),
            |value| brush(value, ThemeBrush::SolidBackground),
        )
    }
    pub fn selection(&self) -> Option<Brush> {
        self.selection.as_deref().and_then(color).map(Brush::Solid)
    }
    pub fn muted(&self) -> Brush {
        brush(&self.muted, ThemeBrush::PrimaryText)
    }
    pub fn accent(&self) -> Brush {
        brush(&self.accent, ThemeBrush::Accent)
    }
    pub fn urgent(&self) -> Brush {
        brush(&self.urgent, ThemeBrush::SystemCritical)
    }
}

fn brush(value: &str, fallback: ThemeBrush) -> Brush {
    color(value)
        .map(Brush::Solid)
        .unwrap_or(Brush::Theme(fallback))
}

fn color(value: &str) -> Option<Color> {
    let value = value.strip_prefix('#').unwrap_or(value);
    if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |start| u8::from_str_radix(&value[start..start + 2], 16).ok();
    Some(Color::rgb(channel(0)?, channel(2)?, channel(4)?))
}
