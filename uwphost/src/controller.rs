// Ported from nickel-uwu (commit 298f87a), Copyright (c) 2026 Steven Horton, MIT. See THIRD_PARTY_NOTICES.md.
//! Private immersive shell controller; every method is compared with its symbol before the call.

use crate::{contract::Failure, memory, symbols::Symbols};
use std::{
    ffi::c_void,
    sync::atomic::{AtomicUsize, Ordering},
};
use windows::{
    core::{IUnknown, Interface, GUID, HRESULT},
    Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
};

const BUILDER: GUID = GUID::from_u128(0xc71c41f1_ddad_42dc_a8fc_f5bfc61df957);
const BUILDER2: GUID = GUID::from_u128(0x2eb59b15_1487_40ce_916e_ef65330dd224);
const TOUCH_KEYBOARD: GUID = GUID::from_u128(0xf580a09b_ea6e_4d83_9a03_add6cb756ab3);
const BEHAVIOR_OFFSET: usize = 0x60;
const BEHAVIOR_SLOTS: usize = 12;
const SHOULD_CREATE_SLOT: usize = 8;

type ShouldCreate = unsafe extern "system" fn(*mut c_void, u32, *mut i32, *mut GUID) -> HRESULT;
static ORIGINAL_SHOULD_CREATE: AtomicUsize = AtomicUsize::new(0);

unsafe extern "system" fn without_touch_keyboard(this: *mut c_void, index: u32, should_create: *mut i32, class: *mut GUID) -> HRESULT {
    // SAFETY: install_filter() stores the symbol-verified method before publishing this vtable.
    let original: ShouldCreate = unsafe { std::mem::transmute(ORIGINAL_SHOULD_CREATE.load(Ordering::Acquire)) };
    // SAFETY: Forwards the caller's arguments; on success both outputs are written.
    unsafe {
        let result = original(this, index, should_create, class);
        if result.is_ok() && !class.is_null() && !should_create.is_null() && class.read() == TOUCH_KEYBOARD {
            should_create.write(0);
        }
        result
    }
}

pub struct Controller(
    #[expect(dead_code, reason = "held for its release on drop")] IUnknown,
);

fn layout(reason: impl Into<String>) -> Failure {
    Failure::Layout(reason.into())
}

/// Requires an initialized STA on this thread.
pub fn start(symbols: &Symbols) -> Result<Controller, Failure> {
    // SAFETY: COM is initialized by the caller; every private method address is checked against its symbol.
    unsafe {
        let builder: IUnknown = CoCreateInstance(&BUILDER, None, CLSCTX_INPROC_SERVER).map_err(|e| layout(format!("create builder: {e}")))?;
        let mut raw = std::ptr::null_mut();
        builder.query(&BUILDER2, &mut raw).ok().map_err(|e| layout(format!("query builder2: {e}")))?;
        let builder2 = IUnknown::from_raw(raw);
        let (target, address) = [&builder, &builder2]
            .into_iter()
            .find_map(|candidate| {
                let method = memory::method(candidate.as_raw() as usize, 3)?;
                (method == symbols.create_controller).then_some((candidate.as_raw(), method))
            })
            .ok_or_else(|| layout("builder vtable does not match its symbols"))?;
        let create: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT = std::mem::transmute(address);
        let mut raw = std::ptr::null_mut();
        create(target, &mut raw).ok().map_err(|e| layout(format!("create controller: {e}")))?;
        if raw.is_null() {
            return Err(layout("builder returned no controller"));
        }
        let controller = IUnknown::from_raw(raw);
        if memory::method(controller.as_raw() as usize, 3) != Some(symbols.start) {
            return Err(layout("controller vtable does not match its symbols"));
        }
        install_filter(controller.as_raw() as usize, symbols)?;
        let start: unsafe extern "system" fn(*mut c_void) -> HRESULT = std::mem::transmute(symbols.start);
        start(controller.as_raw()).ok().map_err(|e| layout(format!("start controller: {e}")))?;
        Ok(Controller(controller))
    }
}

fn install_filter(controller: usize, symbols: &Symbols) -> Result<(), Failure> {
    let behavior = memory::read_usize(controller + BEHAVIOR_OFFSET).filter(|b| *b != 0).ok_or_else(|| layout("no creation behavior"))?;
    let original = memory::read_usize(behavior).ok_or_else(|| layout("creation behavior is unreadable"))?;
    let mut copied = [0usize; BEHAVIOR_SLOTS];
    for (slot, entry) in copied.iter_mut().enumerate() {
        *entry = memory::read_usize(original + slot * size_of::<usize>()).ok_or_else(|| layout("behavior vtable is unreadable"))?;
    }
    if copied[SHOULD_CREATE_SLOT] != symbols.should_create {
        return Err(layout("creation behavior vtable does not match its symbols"));
    }
    ORIGINAL_SHOULD_CREATE.store(copied[SHOULD_CREATE_SLOT], Ordering::Release);
    copied[SHOULD_CREATE_SLOT] = without_touch_keyboard as *const () as usize;
    // leaked: the behavior may be released, or kept alive by a component, after anything we could own
    let copied: &'static [usize; BEHAVIOR_SLOTS] = Box::leak(Box::new(copied));
    // SAFETY: The behavior is a heap COM object (its vtable was just read and verified); the copy is never freed.
    unsafe { (behavior as *mut usize).write(copied.as_ptr() as usize) };
    Ok(())
}
