// Ported from nickel-uwu (commit 298f87a), Copyright (c) 2026 Steven Horton, MIT. See THIRD_PARTY_NOTICES.md.
//! Redirects the immersive fallback window to the desktop band, only inside this process.

use crate::{
    contract::Failure,
    memory,
    symbols::{system_module, Symbols},
};
use std::{
    ffi::c_void,
    sync::atomic::{AtomicUsize, Ordering},
};
use windows::{
    core::s,
    Win32::System::{
        LibraryLoader::GetProcAddress,
        Memory::{VirtualProtect, PAGE_PROTECTION_FLAGS, PAGE_READWRITE},
    },
};

const IMMERSIVE_BACKGROUND_BAND: u32 = 12;
const DESKTOP_BAND: u32 = 1;

type CreateWindowInBand = unsafe extern "system" fn(
    u32,
    *const u16,
    *const u16,
    u32,
    i32,
    i32,
    i32,
    i32,
    *mut c_void,
    *mut c_void,
    *mut c_void,
    *mut c_void,
    u32,
) -> *mut c_void;

static ORIGINAL: AtomicUsize = AtomicUsize::new(0);

#[allow(clippy::too_many_arguments)]
unsafe extern "system" fn redirected(
    ex_style: u32,
    class: *const u16,
    name: *const u16,
    style: u32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    parent: *mut c_void,
    menu: *mut c_void,
    instance: *mut c_void,
    parameter: *mut c_void,
    band: u32,
) -> *mut c_void {
    // SAFETY: install() stores the validated user32 export before publishing this hook.
    let original: CreateWindowInBand = unsafe { std::mem::transmute(ORIGINAL.load(Ordering::Acquire)) };
    let band = if band == IMMERSIVE_BACKGROUND_BAND { DESKTOP_BAND } else { band };
    // SAFETY: Same signature; only the requested band changes.
    unsafe { original(ex_style, class, name, style, x, y, width, height, parent, menu, instance, parameter, band) }
}

pub struct BandRedirect {
    slot: usize,
    previous: usize,
}

impl BandRedirect {
    pub fn install(symbols: &Symbols) -> Result<Self, Failure> {
        let user32 = system_module("user32.dll")?;
        // SAFETY: user32 stays loaded for the process lifetime.
        let export = unsafe { GetProcAddress(user32, s!("CreateWindowInBand")) }
            .map(|export| export as usize)
            .ok_or_else(|| Failure::Layout("CreateWindowInBand is missing".into()))?;
        let slot = symbols.import_slot;
        let previous = memory::read_usize(slot).ok_or_else(|| Failure::Layout("import slot is unreadable".into()))?;
        if previous != export && previous != symbols.delay_thunk {
            return Err(Failure::Layout("import slot holds neither the export nor the delay thunk".into()));
        }
        ORIGINAL.store(export, Ordering::Release);
        write_slot(slot, redirected as *const () as usize).map_err(Failure::Layout)?;
        Ok(Self { slot, previous })
    }
}

impl Drop for BandRedirect {
    fn drop(&mut self) {
        let _ = write_slot(self.slot, self.previous);
    }
}

fn write_slot(slot: usize, value: usize) -> Result<(), String> {
    let mut protection = PAGE_PROTECTION_FLAGS(0);
    // SAFETY: The slot was read successfully and is an import slot of a module loaded in this process;
    // its protection is restored right after the aligned store.
    unsafe {
        VirtualProtect(slot as *const c_void, size_of::<usize>(), PAGE_READWRITE, &mut protection).map_err(|e| format!("unprotect import slot: {e}"))?;
        (slot as *mut usize).write_volatile(value);
        let mut ignored = PAGE_PROTECTION_FLAGS(0);
        VirtualProtect(slot as *const c_void, size_of::<usize>(), protection, &mut ignored).map_err(|e| format!("reprotect import slot: {e}"))
    }
}
