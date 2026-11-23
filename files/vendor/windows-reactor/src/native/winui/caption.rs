#![allow(non_snake_case, non_upper_case_globals, non_camel_case_types, dead_code, clippy::all, unused_qualifications)]

use super::bindings::Color;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RectInt32 {
    pub X: i32,
    pub Y: i32,
    pub Width: i32,
    pub Height: i32,
}

#[repr(transparent)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppWindowTitleBar(windows_core::IUnknown);
windows_core::imp::interface_hierarchy!(
    AppWindowTitleBar,
    windows_core::IUnknown,
    windows_core::IInspectable
);
impl AppWindowTitleBar {
    pub fn SetBackgroundColor<P0>(&self, value: P0) -> windows_core::Result<()>
    where
        P0: windows_core::Param<windows_reference::IReference<Color>>,
    {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).SetBackgroundColor)(
                windows_core::Interface::as_raw(this),
                value.param().abi(),
            )
            .ok()
        }
    }
    pub fn SetButtonBackgroundColor<P0>(&self, value: P0) -> windows_core::Result<()>
    where
        P0: windows_core::Param<windows_reference::IReference<Color>>,
    {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).SetButtonBackgroundColor)(
                windows_core::Interface::as_raw(this),
                value.param().abi(),
            )
            .ok()
        }
    }
    pub fn SetButtonForegroundColor<P0>(&self, value: P0) -> windows_core::Result<()>
    where
        P0: windows_core::Param<windows_reference::IReference<Color>>,
    {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).SetButtonForegroundColor)(
                windows_core::Interface::as_raw(this),
                value.param().abi(),
            )
            .ok()
        }
    }
    pub fn SetButtonHoverBackgroundColor<P0>(&self, value: P0) -> windows_core::Result<()>
    where
        P0: windows_core::Param<windows_reference::IReference<Color>>,
    {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).SetButtonHoverBackgroundColor)(
                windows_core::Interface::as_raw(this),
                value.param().abi(),
            )
            .ok()
        }
    }
    pub fn SetButtonHoverForegroundColor<P0>(&self, value: P0) -> windows_core::Result<()>
    where
        P0: windows_core::Param<windows_reference::IReference<Color>>,
    {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).SetButtonHoverForegroundColor)(
                windows_core::Interface::as_raw(this),
                value.param().abi(),
            )
            .ok()
        }
    }
    pub fn SetButtonInactiveBackgroundColor<P0>(&self, value: P0) -> windows_core::Result<()>
    where
        P0: windows_core::Param<windows_reference::IReference<Color>>,
    {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).SetButtonInactiveBackgroundColor)(
                windows_core::Interface::as_raw(this),
                value.param().abi(),
            )
            .ok()
        }
    }
    pub fn SetButtonInactiveForegroundColor<P0>(&self, value: P0) -> windows_core::Result<()>
    where
        P0: windows_core::Param<windows_reference::IReference<Color>>,
    {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).SetButtonInactiveForegroundColor)(
                windows_core::Interface::as_raw(this),
                value.param().abi(),
            )
            .ok()
        }
    }
    pub fn SetButtonPressedBackgroundColor<P0>(&self, value: P0) -> windows_core::Result<()>
    where
        P0: windows_core::Param<windows_reference::IReference<Color>>,
    {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).SetButtonPressedBackgroundColor)(
                windows_core::Interface::as_raw(this),
                value.param().abi(),
            )
            .ok()
        }
    }
    pub fn SetButtonPressedForegroundColor<P0>(&self, value: P0) -> windows_core::Result<()>
    where
        P0: windows_core::Param<windows_reference::IReference<Color>>,
    {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).SetButtonPressedForegroundColor)(
                windows_core::Interface::as_raw(this),
                value.param().abi(),
            )
            .ok()
        }
    }
    pub fn ExtendsContentIntoTitleBar(&self) -> windows_core::Result<bool> {
        let this = self;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).ExtendsContentIntoTitleBar)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn SetExtendsContentIntoTitleBar(&self, value: bool) -> windows_core::Result<()> {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).SetExtendsContentIntoTitleBar)(
                windows_core::Interface::as_raw(this),
                value,
            )
            .ok()
        }
    }
    pub fn SetForegroundColor<P0>(&self, value: P0) -> windows_core::Result<()>
    where
        P0: windows_core::Param<windows_reference::IReference<Color>>,
    {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).SetForegroundColor)(
                windows_core::Interface::as_raw(this),
                value.param().abi(),
            )
            .ok()
        }
    }
    pub fn Height(&self) -> windows_core::Result<i32> {
        let this = self;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).Height)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn SetInactiveBackgroundColor<P0>(&self, value: P0) -> windows_core::Result<()>
    where
        P0: windows_core::Param<windows_reference::IReference<Color>>,
    {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).SetInactiveBackgroundColor)(
                windows_core::Interface::as_raw(this),
                value.param().abi(),
            )
            .ok()
        }
    }
    pub fn SetInactiveForegroundColor<P0>(&self, value: P0) -> windows_core::Result<()>
    where
        P0: windows_core::Param<windows_reference::IReference<Color>>,
    {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).SetInactiveForegroundColor)(
                windows_core::Interface::as_raw(this),
                value.param().abi(),
            )
            .ok()
        }
    }
    pub fn LeftInset(&self) -> windows_core::Result<i32> {
        let this = self;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).LeftInset)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn RightInset(&self) -> windows_core::Result<i32> {
        let this = self;
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).RightInset)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub fn ResetToDefault(&self) -> windows_core::Result<()> {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).ResetToDefault)(windows_core::Interface::as_raw(
                this,
            ))
            .ok()
        }
    }
    pub fn SetDragRectangles(
        &self,
        value: &[RectInt32],
    ) -> windows_core::Result<()> {
        let this = self;
        unsafe {
            (windows_core::Interface::vtable(this).SetDragRectangles)(
                windows_core::Interface::as_raw(this),
                value.len().try_into().unwrap(),
                value.as_ptr(),
            )
            .ok()
        }
    }
    pub fn IsCustomizationSupported() -> windows_core::Result<bool> {
        Self::IAppWindowTitleBarStatics(|this| unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(this).IsCustomizationSupported)(
                windows_core::Interface::as_raw(this),
                &mut result__,
            )
            .map(|| result__)
        })
    }
    fn IAppWindowTitleBarStatics<
        R,
        F: FnOnce(&IAppWindowTitleBarStatics) -> windows_core::Result<R>,
    >(
        callback: F,
    ) -> windows_core::Result<R> {
        static SHARED: windows_core::imp::FactoryCache<
            AppWindowTitleBar,
            IAppWindowTitleBarStatics,
        > = windows_core::imp::FactoryCache::new();
        SHARED.call(callback)
    }
}
impl windows_core::RuntimeType for AppWindowTitleBar {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_class::<Self, IAppWindowTitleBar>();
}
unsafe impl windows_core::Interface for AppWindowTitleBar {
    type Vtable = <IAppWindowTitleBar as windows_core::Interface>::Vtable;
    const IID: windows_core::GUID = <IAppWindowTitleBar as windows_core::Interface>::IID;
}
impl windows_core::RuntimeName for AppWindowTitleBar {
    const NAME: &'static str = "Microsoft.UI.Windowing.AppWindowTitleBar";
}
unsafe impl Send for AppWindowTitleBar {}
unsafe impl Sync for AppWindowTitleBar {}
windows_core::imp::define_interface!(
    IAppWindowTitleBar,
    IAppWindowTitleBar_Vtbl,
    0x5574efa2_c91c_5700_a363_539c71a7aaf4
);
impl windows_core::RuntimeType for IAppWindowTitleBar {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
}
#[repr(C)]
#[doc(hidden)]
pub struct IAppWindowTitleBar_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub BackgroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub SetBackgroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ButtonBackgroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub SetButtonBackgroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ButtonForegroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub SetButtonForegroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ButtonHoverBackgroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub SetButtonHoverBackgroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ButtonHoverForegroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub SetButtonHoverForegroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ButtonInactiveBackgroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub SetButtonInactiveBackgroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ButtonInactiveForegroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub SetButtonInactiveForegroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ButtonPressedBackgroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub SetButtonPressedBackgroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ButtonPressedForegroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub SetButtonPressedForegroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub ExtendsContentIntoTitleBar:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut bool) -> windows_core::HRESULT,
    pub SetExtendsContentIntoTitleBar:
        unsafe extern "system" fn(*mut core::ffi::c_void, bool) -> windows_core::HRESULT,
    pub ForegroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub SetForegroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub Height:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut i32) -> windows_core::HRESULT,
    IconShowOptions: usize,
    SetIconShowOptions: usize,
    pub InactiveBackgroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub SetInactiveBackgroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub InactiveForegroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub SetInactiveForegroundColor: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
    pub LeftInset:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut i32) -> windows_core::HRESULT,
    pub RightInset:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut i32) -> windows_core::HRESULT,
    pub ResetToDefault: unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
    pub SetDragRectangles: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        u32,
        *const RectInt32,
    ) -> windows_core::HRESULT,
}
windows_core::imp::define_interface!(
    IAppWindowTitleBar2,
    IAppWindowTitleBar2_Vtbl,
    0x86faed38_748a_5b4b_9ccf_3ba0496c9041
);
impl windows_core::RuntimeType for IAppWindowTitleBar2 {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
}
#[repr(C)]
#[doc(hidden)]
pub struct IAppWindowTitleBar2_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    PreferredHeightOption: usize,
    SetPreferredHeightOption: usize,
}
windows_core::imp::define_interface!(
    IAppWindowTitleBar3,
    IAppWindowTitleBar3_Vtbl,
    0x07146e74_0410_5597_aba7_1af276d2ae07
);
impl windows_core::RuntimeType for IAppWindowTitleBar3 {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
}
#[repr(C)]
#[doc(hidden)]
pub struct IAppWindowTitleBar3_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    PreferredTheme: usize,
    SetPreferredTheme: usize,
}
windows_core::imp::define_interface!(
    IAppWindowTitleBarStatics,
    IAppWindowTitleBarStatics_Vtbl,
    0x9e1da52e_8b15_54d6_a886_f7b9f9d930b2
);
impl windows_core::RuntimeType for IAppWindowTitleBarStatics {
    const SIGNATURE: windows_core::imp::ConstBuffer =
        windows_core::imp::ConstBuffer::for_interface::<Self>();
}
#[repr(C)]
#[doc(hidden)]
pub struct IAppWindowTitleBarStatics_Vtbl {
    pub base__: windows_core::IInspectable_Vtbl,
    pub IsCustomizationSupported:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut bool) -> windows_core::HRESULT,
}

/// Tints the caption buttons of the extended title bar, which keep the system colours otherwise; false until the title bar exists.
pub fn style_caption_buttons(foreground: [u8; 3]) -> bool {
    use windows_core::{IUnknown, Interface};
    let Some(raw) = crate::pointer_tap::take_title_bar() else { return false };
    let Ok(title_bar) = unsafe { IUnknown::from_raw(raw) }.cast::<AppWindowTitleBar>() else { return false };
    let color = |[r, g, b]: [u8; 3], a: u8| windows_reference::IReference::from(Color { a, r, g, b });
    let [r, g, b] = foreground;
    let _ = title_bar.SetButtonForegroundColor(&color([r, g, b], 255));
    let _ = title_bar.SetButtonInactiveForegroundColor(&color([r, g, b], 160));
    let _ = title_bar.SetButtonHoverForegroundColor(&color([r, g, b], 255));
    let _ = title_bar.SetButtonBackgroundColor(&color([0, 0, 0], 0));
    let _ = title_bar.SetButtonInactiveBackgroundColor(&color([0, 0, 0], 0));
    true
}
