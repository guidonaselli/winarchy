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
        if !valid {
            return Err("theme contains an invalid color".to_owned());
        }
        Ok(Some(theme.with_readable_text()))
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

    // Text colors come from a palette made for the bar, not for body text: nudge each toward the
    // foreground until it reaches 4.5:1 against every surface it is drawn on.
    fn with_readable_text(mut self) -> Self {
        let grounds: Vec<&str> = [Some(self.background.as_str()), self.surface.as_deref(), self.selection.as_deref()]
            .into_iter()
            .flatten()
            .collect();
        let foreground = self.foreground.clone();
        let adjust = |text: &str| readable(text, &foreground, &grounds);
        self.muted = adjust(&self.muted);
        self.accent = adjust(&self.accent);
        self.urgent = adjust(&self.urgent);
        self
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

fn rgb(value: &str) -> Option<[f64; 3]> {
    let value = value.strip_prefix('#').unwrap_or(value);
    let channel = |start: usize| u8::from_str_radix(value.get(start..start + 2)?, 16).ok().map(f64::from);
    Some([channel(0)?, channel(2)?, channel(4)?])
}

fn luminance([r, g, b]: [f64; 3]) -> f64 {
    let linear = |c: f64| {
        let c = c / 255.0;
        if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

fn contrast(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

const MIN_TEXT_CONTRAST: f64 = 4.5;

fn readable(text: &str, foreground: &str, grounds: &[&str]) -> String {
    let (Some(text_rgb), Some(target)) = (rgb(text), rgb(foreground)) else { return text.to_owned() };
    let grounds: Vec<[f64; 3]> = grounds.iter().filter_map(|g| rgb(g)).collect();
    for step in 0..=20 {
        let mix = f64::from(step) / 20.0;
        let candidate = [0, 1, 2].map(|i| text_rgb[i] + (target[i] - text_rgb[i]) * mix);
        if grounds.iter().all(|ground| contrast(candidate, *ground) >= MIN_TEXT_CONTRAST) {
            return format!("#{:02x}{:02x}{:02x}", candidate[0].round() as u8, candidate[1].round() as u8, candidate[2].round() as u8);
        }
    }
    foreground.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn muted_text_reaches_readable_contrast_on_every_surface() {
        let grounds = ["#e6e9ef", "#ccd0da", "#eff1f5"];
        let fixed = readable("#6c6f85", "#4c4f69", &grounds);
        for ground in grounds {
            assert!(contrast(rgb(&fixed).unwrap(), rgb(ground).unwrap()) >= MIN_TEXT_CONTRAST, "{fixed} on {ground}");
        }
    }

    #[test]
    fn already_readable_text_is_left_alone() {
        assert_eq!(readable("#4c4f69", "#4c4f69", &["#ffffff"]), "#4c4f69");
    }
}
