// Soltar archivos sobre una celda del dock los abre con esa app, como la taskbar de Windows. El lanzamiento corre en un hilo aparte: el dock no debe esperar a la app.
use windows::core::{implement, Interface, Result, HSTRING, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Storage::FileSystem::WIN32_FIND_DATAW;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, IDataObject, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, DVASPECT_CONTENT, FORMATETC, STGM_READ, TYMED_HGLOBAL};
use windows::Win32::System::Ole::*;
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::{DragQueryFileW, IShellLinkW, ShellLink, HDROP};

// Anclas lanzables como `exe archivo`: un exe o un .lnk sin argumentos propios. UWP y Squirrel (Update.exe --processStart) no aceptan el archivo así.
pub fn droppable(open: &str, args: &str) -> bool {
    let o = open.to_ascii_lowercase();
    args.is_empty() && (o.ends_with(".exe") || o.ends_with(".lnk"))
}

pub fn drop_args(base: &str, paths: &[String]) -> String {
    let files = paths.iter().map(|p| format!("\"{}\"", p.replace('"', ""))).collect::<Vec<_>>().join(" ");
    if base.is_empty() { files } else { format!("{base} {files}") }
}

unsafe fn paths(data: &IDataObject) -> Vec<String> {
    let fmt = FORMATETC { cfFormat: CF_HDROP.0, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex: -1, tymed: TYMED_HGLOBAL.0 as u32 };
    let Ok(mut med) = data.GetData(&fmt) else { return Vec::new() };
    let hdrop = HDROP(med.u.hGlobal.0);
    let n = DragQueryFileW(hdrop, u32::MAX, None);
    let out = (0..n)
        .filter_map(|i| {
            let len = DragQueryFileW(hdrop, i, None) as usize;
            let mut buf = vec![0u16; len + 1];
            (DragQueryFileW(hdrop, i, Some(&mut buf)) > 0).then(|| String::from_utf16_lossy(&buf[..len]))
        })
        .collect();
    ReleaseStgMedium(&mut med);
    out
}

// (exe, argumentos propios) de un .lnk; un exe se devuelve tal cual.
unsafe fn resolve(open: &str) -> Option<(String, String)> {
    if !open.to_ascii_lowercase().ends_with(".lnk") {
        return Some((open.to_string(), String::new()));
    }
    let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
    link.cast::<IPersistFile>().ok()?.Load(PCWSTR(HSTRING::from(open).as_ptr()), STGM_READ).ok()?;
    let mut exe = [0u16; 520];
    link.GetPath(&mut exe, std::ptr::null_mut::<WIN32_FIND_DATAW>(), 0).ok()?;
    let mut args = [0u16; 1024];
    link.GetArguments(&mut args).ok()?;
    let s = |b: &[u16]| String::from_utf16_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]);
    let exe = s(&exe);
    (!exe.is_empty()).then(|| (exe, s(&args)))
}

fn open_with(open: String, files: Vec<String>) {
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if let Some((exe, own)) = resolve(&open) {
            crate::dock_menu::launch(&exe, &drop_args(&own, &files));
        }
    });
}

#[implement(IDropTarget)]
struct Target {
    owner: isize,
}

impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(&self, _: Option<&IDataObject>, keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        self.DragOver(keys, pt, effect)
    }

    fn DragOver(&self, _: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        let ok = crate::dock::drop_launch(self.owner, pt.x, pt.y).is_some();
        unsafe { *effect = if ok { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        Ok(())
    }

    fn DragLeave(&self) -> Result<()> {
        Ok(())
    }

    fn Drop(&self, data: Option<&IDataObject>, _: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        unsafe { *effect = DROPEFFECT_NONE };
        let (Some(data), Some((open, _))) = (data, crate::dock::drop_launch(self.owner, pt.x, pt.y)) else { return Ok(()) };
        let files = unsafe { paths(data) };
        if !files.is_empty() {
            open_with(open, files);
            unsafe { *effect = DROPEFFECT_COPY };
        }
        Ok(())
    }
}

pub unsafe fn register(hwnd: HWND) {
    let _ = OleInitialize(None);
    let target: IDropTarget = Target { owner: hwnd.0 as isize }.into();
    let _ = RegisterDragDrop(hwnd, &target);
}

pub unsafe fn revoke(hwnd: HWND) {
    let _ = RevokeDragDrop(hwnd);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_executables_and_shortcuts_accept_files() {
        assert!(droppable(r"C:\Apps\Code.EXE", ""));
        assert!(droppable(r"C:\TaskBar\Helium.lnk", ""));
        assert!(!droppable(r"shell:AppsFolder\X!App", ""));
        assert!(!droppable(r"C:\Discord\Update.exe", "--processStart Discord.exe"));
    }

    #[test]
    fn dropped_paths_follow_the_anchors_own_arguments_and_lose_quotes() {
        assert_eq!(drop_args("--flag", &[r"C:\a b\c.txt".into(), r#"d"e"#.into()]), r#"--flag "C:\a b\c.txt" "de""#);
        assert_eq!(drop_args("", &[r"C:\x.txt".into()]), r#""C:\x.txt""#);
    }
}
