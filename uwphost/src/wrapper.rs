// Ported from nickel-uwu (commit 298f87a), Copyright (c) 2026 Steven Horton, MIT. See THIRD_PARTY_NOTICES.md.
//! Finds the private view wrapper of a frame and advances it to presentation.

use crate::{contract::Failure, memory, symbols::Symbols};
use std::{cell::Cell, ffi::c_void};
use windows::{
    core::{IUnknown, Interface},
    Win32::System::{
        Memory::{VirtualQuery, MEMORY_BASIC_INFORMATION, MEM_COMMIT, MEM_PRIVATE, PAGE_EXECUTE_READWRITE, PAGE_GUARD, PAGE_READWRITE, PAGE_WRITECOPY},
    },
};

const MAX_SCAN_BYTES: usize = 256 * 1024 * 1024;
const CHUNK_BYTES: usize = 64 * 1024;
const PRESENTATION_READY_EVENT: u64 = 0x26;

// first bytes of each private method in the builds measured; any change fails closed
const DISCOVERY_PROLOGUE: [u8; 10] = [0x48, 0x89, 0x5c, 0x24, 0x10, 0x57, 0x48, 0x83, 0xec, 0x20];
const VISIBILITY_PROLOGUE: [u8; 10] = [0x48, 0x89, 0x5c, 0x24, 0x10, 0x48, 0x89, 0x74, 0x24, 0x18];
const READINESS_PROLOGUES: [[u8; 10]; 2] = [
    [0x48, 0x89, 0x5c, 0x24, 0x10, 0x57, 0x48, 0x83, 0xec, 0x20],
    // build 26300
    [0x48, 0x89, 0x5c, 0x24, 0x10, 0x55, 0x56, 0x57, 0x48, 0x83],
];

#[derive(Clone, Copy, Debug)]
pub struct Wrapper {
    pub interface: usize,
    pub client: usize,
    pub wait_flags: u32,
}

pub fn verify_prologues(symbols: &Symbols) -> Result<(), Failure> {
    let checks = [
        ("window discovery", memory::starts_with(symbols.discovery, &DISCOVERY_PROLOGUE)),
        ("visibility", memory::starts_with(symbols.visibility, &VISIBILITY_PROLOGUE)),
        ("readiness", READINESS_PROLOGUES.iter().any(|p| memory::starts_with(symbols.readiness, p))),
    ];
    match checks.iter().find(|(_, matches)| !matches) {
        Some((name, _)) => Err(Failure::Layout(format!("unexpected {name} prologue"))),
        None => Ok(()),
    }
}

thread_local! {
    static LAST_DISPATCHER: Cell<usize> = const { Cell::new(0) };
}

pub fn find(frame: usize, symbols: &Symbols) -> Result<Wrapper, String> {
    // every use revalidates: the controller can replace its dispatcher
    let inspect = |dispatcher: usize| {
        if memory::read_usize(dispatcher) != Some(symbols.dispatcher_vtable)
            || memory::read_usize(dispatcher + 8) != Some(symbols.dispatcher_weak_vtable)
            || memory::read_usize(dispatcher + 0xd8) != Some(symbols.dispatcher_collection_vtable)
        {
            return None;
        }
        in_dispatcher(dispatcher, frame, symbols.wrapper_vtable, memory::read_usize)
    };
    if let Some(wrapper) = Some(LAST_DISPATCHER.get()).filter(|d| *d != 0).and_then(inspect) {
        return Ok(wrapper);
    }
    let mut cursor = 0usize;
    let mut scanned = 0usize;
    let mut buffer = vec![0u8; CHUNK_BYTES];
    while scanned < MAX_SCAN_BYTES {
        let mut info = MEMORY_BASIC_INFORMATION::default();
        // SAFETY: VirtualQuery writes a structure of the supplied size for any candidate address.
        if unsafe { VirtualQuery(Some(cursor as *const c_void), &mut info, size_of::<MEMORY_BASIC_INFORMATION>()) } == 0 {
            break;
        }
        let base = info.BaseAddress as usize;
        let Some(end) = base.checked_add(info.RegionSize).filter(|end| *end > cursor) else { break };
        cursor = end;
        let writable = [PAGE_READWRITE, PAGE_WRITECOPY, PAGE_EXECUTE_READWRITE].iter().any(|flag| info.Protect.0 & flag.0 != 0);
        if info.State != MEM_COMMIT || info.Type != MEM_PRIVATE || info.Protect.0 & PAGE_GUARD.0 != 0 || !writable {
            continue;
        }
        let mut address = base;
        while address < end && scanned < MAX_SCAN_BYTES {
            let length = (end - address).min(CHUNK_BYTES);
            let read = memory::read_into(address, &mut buffer[..length]);
            scanned += read;
            for (index, word) in buffer[..read].as_chunks::<{ size_of::<usize>() }>().0.iter().enumerate() {
                if *word != symbols.dispatcher_vtable.to_le_bytes() {
                    continue;
                }
                let candidate = address + index * size_of::<usize>();
                // old dispatchers coexist with the one that owns a new app, so keep looking on a miss
                if let Some(wrapper) = inspect(candidate) {
                    LAST_DISPATCHER.set(candidate);
                    return Ok(wrapper);
                }
            }
            address += length;
        }
    }
    Err(format!("no wrapper owns the frame after scanning {scanned} bytes"))
}

fn in_dispatcher(dispatcher: usize, frame: usize, vtable: usize, read: impl Fn(usize) -> Option<usize>) -> Option<Wrapper> {
    let begin = read(dispatcher.checked_add(0x180)?)?;
    let end = read(dispatcher.checked_add(0x188)?)?;
    let bytes = end.checked_sub(begin)?;
    if bytes % size_of::<usize>() != 0 || bytes > 1024 * size_of::<usize>() {
        return None;
    }
    (begin..end).step_by(size_of::<usize>()).find_map(|entry| {
        let wrapper = read(entry)?;
        if read(wrapper) != Some(vtable) || read(wrapper.checked_add(0x78)?) != Some(frame) {
            return None;
        }
        Some(Wrapper {
            interface: wrapper.checked_add(0x20)?,
            client: read(wrapper.checked_add(0x158)?)?,
            wait_flags: read(wrapper.checked_add(0x160)?)? as u32,
        })
    })
}

/// Which private events a view that is not ready yet still needs.
#[derive(Debug, PartialEq)]
pub struct Needed {
    pub discovery: bool,
    pub visibility: bool,
    pub readiness: bool,
}

pub fn needed(wrapper: &Wrapper, attached: bool) -> Needed {
    let unowned = wrapper.client == 0;
    Needed {
        discovery: unowned,
        visibility: unowned || wrapper.wait_flags & 1 != 0 || !attached,
        readiness: unowned || wrapper.wait_flags & 2 != 0 || !attached,
    }
}

/// Runs on the controller's STA. `core` must be a live CoreWindow of the frame's app.
pub fn advance(wrapper: &Wrapper, core: usize, attached: bool, symbols: &Symbols) -> Result<(), String> {
    let base = wrapper.interface.checked_sub(0x20).ok_or("wrapper pointer is too small")?;
    // SAFETY: `find` verified the wrapper's vtable; the reference is held across reentrant calls.
    let raw = base as *mut c_void;
    let _alive = unsafe { IUnknown::from_raw_borrowed(&raw) }.ok_or("null wrapper")?.clone();
    let needed = needed(wrapper, attached);
    let owns = || {
        (memory::read_usize(base + 0x158) == Some(core)).then_some(()).ok_or_else(|| "wrapper does not own this CoreWindow".to_string())
    };
    // SAFETY: Addresses come from the loaded module's symbols with verified prologues, on the live
    // wrapper subobject; the later two require the wrapper to already own the window.
    unsafe {
        if needed.discovery {
            let discover: unsafe extern "system" fn(usize, usize) = std::mem::transmute(symbols.discovery);
            discover(base, core);
        }
        if needed.visibility {
            owns()?;
            let visibility: unsafe extern "system" fn(usize, u32, u32) = std::mem::transmute(symbols.visibility);
            visibility(base, 1, 1);
        }
        if needed.readiness {
            owns()?;
            let readiness: unsafe extern "system" fn(usize, u64) = std::mem::transmute(symbols.readiness);
            readiness(base, PRESENTATION_READY_EVENT);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn memory_map() -> HashMap<usize, usize> {
        HashMap::from([
            (0x1180, 0x3000),
            (0x1188, 0x3008),
            (0x3000, 0x4000),
            (0x4000, 0x7777),
            (0x4078, 11),
            (0x4158, 12),
            (0x4160, 0),
            (0x2180, 0x5000),
            (0x2188, 0x5008),
            (0x5000, 0x6000),
            (0x6000, 0x7777),
            (0x6078, 21),
            (0x6158, 0),
            (0x6160, 17),
        ])
    }

    fn find_in(memory: &HashMap<usize, usize>, dispatcher: usize) -> Option<Wrapper> {
        in_dispatcher(dispatcher, 21, 0x7777, |address| memory.get(&address).copied())
    }

    #[test]
    fn an_older_dispatcher_does_not_hide_the_new_wrapper() {
        let memory = memory_map();
        let found = [0x1000, 0x2000].into_iter().find_map(|d| find_in(&memory, d)).unwrap();
        assert_eq!((found.interface, found.client, found.wait_flags), (0x6020, 0, 17));
    }

    #[test]
    fn an_invalid_collection_is_skipped() {
        for end in [0x3000, 0x2fff, 0x3001, 0x9000] {
            let mut memory = memory_map();
            memory.insert(0x1188, end);
            assert!(find_in(&memory, 0x1000).is_none());
            assert!(find_in(&memory, 0x2000).is_some());
        }
    }

    #[test]
    fn unreadable_readiness_is_not_ready() {
        let mut memory = memory_map();
        memory.remove(&0x6160);
        assert!(find_in(&memory, 0x2000).is_none());
    }

    #[test]
    fn only_the_missing_events_are_sent() {
        let wrapper = |client, wait_flags| Wrapper { interface: 0, client, wait_flags };
        assert_eq!(needed(&wrapper(0, 0), false), Needed { discovery: true, visibility: true, readiness: true });
        assert_eq!(needed(&wrapper(1, 1), true), Needed { discovery: false, visibility: true, readiness: false });
        assert_eq!(needed(&wrapper(1, 2), true), Needed { discovery: false, visibility: false, readiness: true });
        assert_eq!(needed(&wrapper(1, 0), true), Needed { discovery: false, visibility: false, readiness: false });
    }
}
