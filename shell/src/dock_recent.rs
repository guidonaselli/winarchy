// Archivos recientes de una app (la lista "Recientes" de su jump list), por AppUserModelID.
use windows::core::{HSTRING, PWSTR};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Shell::Common::IObjectArray;
use windows::Win32::UI::Shell::{ApplicationDocumentLists, IApplicationDocumentLists, IShellItem, ADLT_RECENT, SIGDN_FILESYSPATH, SIGDN_NORMALDISPLAY};

pub const MAX: u32 = 5;

unsafe fn name(item: &IShellItem, kind: windows::Win32::UI::Shell::SIGDN) -> Option<String> {
    let p: PWSTR = item.GetDisplayName(kind).ok()?;
    let s = p.to_string().ok();
    CoTaskMemFree(Some(p.0 as *const _));
    s.filter(|s| !s.is_empty())
}

// (nombre a mostrar, ruta). Solo archivos reales: lo que no tiene ruta en disco (URLs, tareas) no se lista.
pub fn list(aumid: &str) -> Vec<(String, String)> {
    unsafe {
        let Ok(docs) = CoCreateInstance::<_, IApplicationDocumentLists>(&ApplicationDocumentLists, None, CLSCTX_INPROC_SERVER) else { return Vec::new() };
        if docs.SetAppID(&HSTRING::from(aumid)).is_err() {
            return Vec::new();
        }
        let Ok(arr) = docs.GetList::<IObjectArray>(ADLT_RECENT, MAX) else { return Vec::new() };
        (0..arr.GetCount().unwrap_or(0))
            .filter_map(|i| {
                let item: IShellItem = arr.GetAt(i).ok()?;
                Some((name(&item, SIGDN_NORMALDISPLAY)?, name(&item, SIGDN_FILESYSPATH)?))
            })
            .collect()
    }
}
