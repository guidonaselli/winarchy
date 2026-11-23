use std::cell::RefCell;
use windows_core::{IUnknown, Interface};

thread_local! {
    static TITLE_BAR: RefCell<Option<IUnknown>> = const { RefCell::new(None) };
    static LAST: RefCell<Option<(IUnknown, IUnknown)>> = const { RefCell::new(None) };
}

pub(crate) fn remember(element: &impl Interface, args: &impl Interface) {
    if let (Ok(element), Ok(args)) = (element.cast::<IUnknown>(), args.cast::<IUnknown>()) {
        LAST.with(|last| *last.borrow_mut() = Some((element, args)));
    }
}

/// Raw, owned COM pointers (element, pointer-routed event args) of the latest Border pointer press or move;
/// the caller releases them. Raw because applications bind WinUI with their own copy of windows-core.
pub fn take_last_pointer() -> Option<(*mut core::ffi::c_void, *mut core::ffi::c_void)> {
    LAST.with(|last| last.borrow_mut().take()).map(|(element, args)| (element.into_raw(), args.into_raw()))
}

pub(crate) fn remember_title_bar(title_bar: &impl Interface) {
    if let Ok(title_bar) = title_bar.cast::<IUnknown>() {
        TITLE_BAR.with(|slot| *slot.borrow_mut() = Some(title_bar));
    }
}

/// Raw, owned (extra reference) COM pointer to the window's AppWindowTitleBar once it was extended into the content; the caller releases it.
pub(crate) fn take_title_bar() -> Option<*mut core::ffi::c_void> {
    TITLE_BAR.with(|slot| slot.borrow().clone()).map(IUnknown::into_raw)
}
