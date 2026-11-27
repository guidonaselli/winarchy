// Ported from nickel-uwu (commit 298f87a), Copyright (c) 2026 Steven Horton, MIT. See THIRD_PARTY_NOTICES.md.
//! Removes the shell cloak of an application view.

use crate::{memory, symbols::Symbols};
use std::ffi::c_void;
use windows::{
    core::{IUnknown, Interface, GUID, HRESULT},
    Win32::System::Com::{CoCreateInstance, IServiceProvider, CLSCTX_LOCAL_SERVER},
};

const IMMERSIVE_SHELL: GUID = GUID::from_u128(0xc2f03a33_21f5_47fa_b4bb_156362a2f239);
const VIEW_COLLECTION: GUID = GUID::from_u128(0x1841c6d7_4f9d_42c0_af41_8747538f10e5);
const LOOKUP_SLOT: usize = 8;
const SET_CLOAK_SLOT: usize = 12;
const CLOAK_SHELL: u32 = 1;

/// Runs on the controller's STA, where both interfaces are direct in-process pointers.
pub fn uncloak(app_id: &str, symbols: &Symbols) -> Result<(), String> {
    // SAFETY: Both private methods are compared with their symbols before the call; returned references are released.
    unsafe {
        let shell: IServiceProvider = CoCreateInstance(&IMMERSIVE_SHELL, None, CLSCTX_LOCAL_SERVER).map_err(|e| format!("immersive shell: {e}"))?;
        let mut raw = std::ptr::null_mut();
        (shell.vtable().QueryService)(shell.as_raw(), &VIEW_COLLECTION, &VIEW_COLLECTION, &mut raw).ok().map_err(|e| format!("view collection: {e}"))?;
        if raw.is_null() {
            return Err("no view collection".into());
        }
        let collection = IUnknown::from_raw(raw);
        let lookup = memory::method(collection.as_raw() as usize, LOOKUP_SLOT)
            .filter(|method| symbols.view_lookups.contains(method))
            .ok_or("view lookup does not match its symbols")?;
        let lookup: unsafe extern "system" fn(*mut c_void, *const u16, *mut *mut c_void) -> HRESULT = std::mem::transmute(lookup);
        let app_id: Vec<u16> = app_id.encode_utf16().chain(Some(0)).collect();
        let mut raw = std::ptr::null_mut();
        lookup(collection.as_raw(), app_id.as_ptr(), &mut raw).ok().map_err(|e| format!("view lookup: {e}"))?;
        if raw.is_null() {
            return Err("no view for the app".into());
        }
        let view = IUnknown::from_raw(raw);
        if memory::method(view.as_raw() as usize, SET_CLOAK_SLOT) != Some(symbols.set_cloak) {
            return Err("SetCloak does not match its symbols".into());
        }
        let set_cloak: unsafe extern "system" fn(*mut c_void, u32, i32) -> HRESULT = std::mem::transmute(symbols.set_cloak);
        set_cloak(view.as_raw(), CLOAK_SHELL, 0).ok().map_err(|e| format!("uncloak: {e}"))
    }
}
