// Ported from nickel-uwu (commit 298f87a), Copyright (c) 2026 Steven Horton, MIT. See THIRD_PARTY_NOTICES.md.
//! Exact-module public symbols, resolved all at once before anything is registered or called.

use crate::{contract::Failure, download};
use object::{Object, ObjectSegment};
use pdb::{FallibleIterator, SymbolData};
use std::{
    collections::HashMap,
    ffi::c_void,
    path::{Path, PathBuf},
    time::Duration,
};
use windows::{
    core::{w, HSTRING},
    Win32::{
        Foundation::HMODULE,
        Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO},
        System::LibraryLoader::{LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32},
    },
};

pub const TWINUI: &str = "twinui.pcshell.dll";
const SERVICE_PROVIDER: &str = "Windows.ImmersiveShell.ServiceProvider.dll";
const MAX_PDB_BYTES: u64 = 200 * 1024 * 1024;
const RETRY_DELAYS: [u64; 3] = [0, 5, 20];

const TWINUI_NAMES: [&str; 14] = [
    "?CreateImmersiveShellController@CImmersiveShellBuilder@@UEAAJPEAPEAUIImmersiveShellController@@@Z",
    "?ShouldCreateComponent@CImmersiveShellCreationBehavior@@UEAAJIPEAHPEAU_GUID@@@Z",
    "__imp_CreateWindowInBand",
    "__imp_load_CreateWindowInBand",
    "?WindowDiscoveredFromShellHook@UwpWindowWrapperBase@@UEAAXPEAUHWND__@@@Z",
    "?VisibilityChanged@UwpWindowWrapperBase@@UEAAXW4EventPhase@@W4Visibility@@@Z",
    "?HandlePresentationReadinessChange@UwpWindowWrapperBase@@UEAAX_K@Z",
    "??_7UwpWindowEventDispatcher@@6B@",
    "??_7UwpWindowEventDispatcher@@6BIWeakReferenceSource@@@",
    "??_7UwpWindowEventDispatcher@@6BINtUserViewWrapperCollection@@@",
    "??_7UwpWindowWrapper@@6B?$ChainInterfaces@UIUwpWindowWrapperInternal@@UIWindowWrapper@ViewManagerInterop@Shell@Internal@Windows@@VNil@Details@WRL@Microsoft@@V789Microsoft@@V789Microsoft@@V789Microsoft@@V789Microsoft@@V789Microsoft@@V789Microsoft@@V789Microsoft@@@WRL@Microsoft@@@",
    "?GetViewForAppUserModelId@ApplicationViewCollectionBase@@UEAAJPEBGPEAPEAUIApplicationView@@@Z",
    "?GetViewForAppUserModelId@CApplicationViewManager@@UEAAJPEBGPEAPEAUIApplicationView@@@Z",
    "?SetCloak@CApplicationViewBase@@UEAAJW4APPLICATION_VIEW_CLOAK_TYPE@@H@Z",
];
const SERVICE_PROVIDER_NAMES: [&str; 1] = ["?Start@CImmersiveShellController@@UEAAJXZ"];

/// Absolute addresses inside the modules loaded in this process.
pub struct Symbols {
    pub create_controller: usize,
    pub should_create: usize,
    pub import_slot: usize,
    pub delay_thunk: usize,
    pub discovery: usize,
    pub visibility: usize,
    pub readiness: usize,
    pub dispatcher_vtable: usize,
    pub dispatcher_weak_vtable: usize,
    pub dispatcher_collection_vtable: usize,
    pub wrapper_vtable: usize,
    pub view_lookups: [usize; 2],
    pub set_cloak: usize,
    pub start: usize,
}

pub fn system_module(name: &str) -> Result<HMODULE, Failure> {
    // SAFETY: Loading is restricted to System32; the module stays loaded for the process lifetime.
    unsafe { LoadLibraryExW(&HSTRING::from(name), None, LOAD_LIBRARY_SEARCH_SYSTEM32) }.map_err(|e| Failure::Layout(format!("load {name}: {e}")))
}

fn system32(name: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    PathBuf::from(root).join("System32").join(name)
}

pub fn file_version(name: &str) -> Option<String> {
    let path = HSTRING::from(system32(name).as_os_str());
    // SAFETY: The buffer has the size the API reported and outlives the pointer VerQueryValueW returns into it.
    unsafe {
        let size = GetFileVersionInfoSizeW(&path, None);
        let mut data = vec![0u8; size as usize];
        if size == 0 || GetFileVersionInfoW(&path, None, size, data.as_mut_ptr().cast()).is_err() {
            return None;
        }
        let mut info: *mut c_void = std::ptr::null_mut();
        let mut length = 0;
        if !VerQueryValueW(data.as_ptr().cast(), w!("\\"), &mut info, &mut length).as_bool() || (length as usize) < size_of::<VS_FIXEDFILEINFO>() {
            return None;
        }
        let info = info.cast::<VS_FIXEDFILEINFO>().read_unaligned();
        Some(format!("{}.{}.{}.{}", info.dwFileVersionMS >> 16, info.dwFileVersionMS & 0xffff, info.dwFileVersionLS >> 16, info.dwFileVersionLS & 0xffff))
    }
}

#[derive(Debug)]
struct Identity {
    pdb: String,
    key: String,
    age: u32,
    image_size: u64,
}

fn identity(path: &Path) -> Result<Identity, String> {
    let image = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let pe = object::File::parse(image.as_slice()).map_err(|e| format!("parse {}: {e}", path.display()))?;
    let record = pe.pdb_info().map_err(|e| format!("CodeView record: {e}"))?.ok_or("no CodeView record")?;
    let pdb = std::str::from_utf8(record.path()).map_err(|_| "PDB path is not UTF-8")?;
    let pdb = Path::new(pdb).file_name().ok_or("no PDB file name")?.to_string_lossy().into_owned();
    let base = pe.relative_address_base();
    let image_size = pe.segments().map(|s| s.address() - base + s.size()).max().ok_or("no sections")?;
    Ok(Identity { pdb, key: symbol_key(record.guid(), record.age()), age: record.age(), image_size })
}

fn symbol_key(guid: [u8; 16], age: u32) -> String {
    format!(
        "{:08X}{:04X}{:04X}{}{:X}",
        u32::from_le_bytes([guid[0], guid[1], guid[2], guid[3]]),
        u16::from_le_bytes([guid[4], guid[5]]),
        u16::from_le_bytes([guid[6], guid[7]]),
        guid[8..].iter().map(|byte| format!("{byte:02X}")).collect::<String>(),
        age,
    )
}

fn check_identity(guid: &str, age: u32, wanted: &Identity) -> Result<(), String> {
    let guid = guid.replace('-', "").to_uppercase();
    if wanted.key.get(..32) != Some(guid.as_str()) || age < wanted.age {
        return Err(format!("PDB identity does not match {}", wanted.key));
    }
    Ok(())
}

fn resolve_pdb(path: &Path, wanted: &Identity, names: &[&str]) -> Result<HashMap<String, usize>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut pdb = pdb::PDB::open(file).map_err(|e| format!("parse PDB: {e}"))?;
    let info = pdb.pdb_information().map_err(|e| format!("PDB identity: {e}"))?;
    check_identity(&info.guid.to_string(), info.age, wanted)?;
    let map = pdb.address_map().map_err(|e| format!("address map: {e}"))?;
    let table = pdb.global_symbols().map_err(|e| format!("symbols: {e}"))?;
    let mut found = HashMap::new();
    let mut symbols = table.iter();
    while let Some(symbol) = symbols.next().map_err(|e| format!("symbols: {e}"))? {
        // records of kinds the parser does not know are not an error
        let (offset, name) = match symbol.parse() {
            Ok(SymbolData::Public(data)) => (data.offset, data.name),
            Ok(SymbolData::Procedure(data)) => (data.offset, data.name),
            Ok(SymbolData::Data(data)) => (data.offset, data.name),
            _ => continue,
        };
        let Some(wanted) = names.iter().find(|n| n.as_bytes() == name.as_bytes()) else { continue };
        if let Some(rva) = offset.to_rva(&map) {
            found.insert(wanted.to_string(), rva.0 as usize);
        }
        if found.len() == names.len() {
            break;
        }
    }
    Ok(found)
}

/// Cached RVAs, valid only for this file version, complete and inside the image.
fn read_manifest(text: &str, version: &str, names: &[&str], image_size: u64) -> Option<HashMap<String, usize>> {
    let mut lines = text.lines();
    if lines.next()? != version {
        return None;
    }
    let mut found = HashMap::new();
    for line in lines {
        let (name, value) = line.split_once('\t')?;
        let rva = usize::from_str_radix(value, 16).ok()?;
        if rva == 0 || rva as u64 >= image_size {
            return None;
        }
        found.insert(name.to_owned(), rva);
    }
    names.iter().all(|name| found.contains_key(*name)).then_some(found)
}

fn manifest_text(version: &str, names: &[&str], rvas: &HashMap<String, usize>) -> String {
    let rows: String = names.iter().map(|name| format!("{name}\t{:x}\n", rvas[*name])).collect();
    format!("{version}\n{rows}")
}

fn download_pdb(identity: &Identity, destination: &Path, notify: &dyn Fn(&str)) -> Result<(), String> {
    let path = format!("/download/symbols/{0}/{1}/{0}", identity.pdb, identity.key);
    let source = download::Source { host: "msdl.microsoft.com", port: 443, secure: true, path: &path };
    let limits = download::Limits { stall_ms: 30_000, max_bytes: MAX_PDB_BYTES };
    let mut last = String::new();
    for delay in RETRY_DELAYS {
        std::thread::sleep(Duration::from_secs(delay));
        notify(&identity.pdb);
        match download::fetch(&source, destination, &limits) {
            Ok(()) => return Ok(()),
            Err(error) => last = error,
        }
    }
    Err(format!("download {}: {last}", identity.pdb))
}

fn module_symbols(
    cache: &Path,
    module: &str,
    names: &[&str],
    notify: &dyn Fn(&str),
    log: &dyn Fn(&str),
) -> Result<Vec<usize>, Failure> {
    let base = system_module(module)?.0 as usize;
    let version = file_version(module).ok_or_else(|| Failure::Symbols(format!("{module} has no file version")))?;
    let identity = identity(&system32(module)).map_err(Failure::Symbols)?;
    log(&format!("phase=module name={module} version={version} pdb={} key={}", identity.pdb, identity.key));
    let directory = cache.join(&identity.pdb).join(&identity.key);
    let manifest = directory.join("symbols.txt");
    let cached = std::fs::read_to_string(&manifest).ok().and_then(|text| read_manifest(&text, &version, names, identity.image_size));
    let rvas = match cached {
        Some(rvas) => rvas,
        None => {
            std::fs::create_dir_all(&directory).map_err(|e| Failure::Symbols(format!("create symbol cache: {e}")))?;
            let pdb = directory.join(&identity.pdb);
            if !pdb.is_file() {
                download_pdb(&identity, &pdb, notify).map_err(Failure::Symbols)?;
            }
            let rvas = resolve_pdb(&pdb, &identity, names).map_err(|error| {
                let _ = std::fs::remove_file(&pdb);
                Failure::Symbols(error)
            })?;
            if let Some(missing) = names.iter().find(|name| !rvas.contains_key(**name)) {
                return Err(Failure::Layout(format!("{module} has no symbol {missing}")));
            }
            // a manifest cut short would be trusted forever: write it whole or not at all
            let partial = manifest.with_extension("tmp");
            std::fs::write(&partial, manifest_text(&version, names, &rvas))
                .and_then(|()| std::fs::rename(&partial, &manifest))
                .map_err(|e| Failure::Symbols(format!("write manifest: {e}")))?;
            rvas
        }
    };
    Ok(names.iter().map(|name| base + rvas[*name]).collect())
}

pub fn load(root: &Path, notify: &dyn Fn(&str), log: &dyn Fn(&str)) -> Result<Symbols, Failure> {
    let cache = root.join("state").join("symbols");
    let twinui = module_symbols(&cache, TWINUI, &TWINUI_NAMES, notify, log)?;
    let provider = module_symbols(&cache, SERVICE_PROVIDER, &SERVICE_PROVIDER_NAMES, notify, log)?;
    Ok(Symbols {
        create_controller: twinui[0],
        should_create: twinui[1],
        import_slot: twinui[2],
        delay_thunk: twinui[3],
        discovery: twinui[4],
        visibility: twinui[5],
        readiness: twinui[6],
        dispatcher_vtable: twinui[7],
        dispatcher_weak_vtable: twinui[8],
        dispatcher_collection_vtable: twinui[9],
        wrapper_vtable: twinui[10],
        view_lookups: [twinui[11], twinui[12]],
        set_cloak: twinui[13],
        start: provider[0],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wanted() -> Identity {
        Identity { pdb: "a.pdb".into(), key: "09A465FCB1668CB766F6031877BFD6832".into(), age: 2, image_size: 0x1000 }
    }

    #[test]
    fn formats_the_symbol_store_key() {
        let guid = [0xfc, 0x65, 0xa4, 0x09, 0x66, 0xb1, 0xb7, 0x8c, 0x66, 0xf6, 0x03, 0x18, 0x77, 0xbf, 0xd6, 0x83];
        assert_eq!(symbol_key(guid, 1), "09A465FCB1668CB766F6031877BFD6831");
    }

    #[test]
    fn rejects_another_guid_or_an_older_age() {
        let wanted = wanted();
        check_identity("09a465fc-b166-8cb7-66f6-031877bfd683", 2, &wanted).unwrap();
        check_identity("09a465fc-b166-8cb7-66f6-031877bfd683", 3, &wanted).unwrap();
        assert!(check_identity("09a465fc-b166-8cb7-66f6-031877bfd683", 1, &wanted).is_err());
        assert!(check_identity("19a465fc-b166-8cb7-66f6-031877bfd683", 2, &wanted).is_err());
    }

    #[test]
    fn a_truncated_pdb_is_an_error() {
        let path = std::env::temp_dir().join(format!("uwphost-cut-{}.pdb", std::process::id()));
        std::fs::write(&path, b"Microsoft C/C++ MSF 7.00\r\n").unwrap();
        assert!(resolve_pdb(&path, &wanted(), &["a"]).is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn a_manifest_is_trusted_only_complete_current_and_in_range() {
        let names = ["a", "b"];
        let good = manifest_text("1.0", &names, &HashMap::from([("a".into(), 0x10), ("b".into(), 0x20)]));
        assert_eq!(read_manifest(&good, "1.0", &names, 0x1000).unwrap()["b"], 0x20);
        assert!(read_manifest(&good, "1.1", &names, 0x1000).is_none());
        assert!(read_manifest(&good, "1.0", &["a", "b", "c"], 0x1000).is_none());
        assert!(read_manifest(&good, "1.0", &names, 0x20).is_none());
        assert!(read_manifest("1.0\na\tzz\n", "1.0", &["a"], 0x1000).is_none());
        assert!(read_manifest("1.0\na\t0\n", "1.0", &["a"], 0x1000).is_none());
    }
}
