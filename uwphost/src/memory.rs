//! Reads of this process that turn a wrong private offset into an error instead of an access violation.

use std::ffi::c_void;
use windows::Win32::System::{Diagnostics::Debug::ReadProcessMemory, Threading::GetCurrentProcess};

pub fn read_into(address: usize, buffer: &mut [u8]) -> usize {
    let mut read = 0;
    // SAFETY: The kernel validates the source range; the buffer is writable for its whole length.
    let _ = unsafe { ReadProcessMemory(GetCurrentProcess(), address as *const c_void, buffer.as_mut_ptr().cast(), buffer.len(), Some(&mut read)) };
    read
}

pub fn read_usize(address: usize) -> Option<usize> {
    let mut bytes = [0u8; size_of::<usize>()];
    (read_into(address, &mut bytes) == bytes.len()).then(|| usize::from_le_bytes(bytes))
}

pub fn method(object: usize, slot: usize) -> Option<usize> {
    read_usize(read_usize(object)? + slot * size_of::<usize>())
}

pub fn starts_with(address: usize, expected: &[u8]) -> bool {
    let mut actual = vec![0u8; expected.len()];
    read_into(address, &mut actual) == actual.len() && actual == expected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wild_address_is_an_error_not_a_crash() {
        assert_eq!(read_usize(0x10), None);
        assert_eq!(method(0x10, 3), None);
        assert!(!starts_with(0, &[0x48]));
        let value = 7usize;
        assert_eq!(read_usize(&value as *const usize as usize), Some(7));
    }
}
