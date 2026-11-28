use std::time::Duration;
use windows::{
    Win32::{
        Foundation::{POINT, RECT},
        Graphics::Gdi::ClientToScreen,
        System::{
            Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
            Variant::VARIANT,
        },
        UI::{
            Accessibility::{
                CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationScrollPattern, TreeScope_Descendants,
                UIA_IsScrollPatternAvailablePropertyId, UIA_NamePropertyId, UIA_ScrollPatternId,
            },
            WindowsAndMessaging::{GetForegroundWindow},
            HiDpi::GetDpiForWindow,
        },
    },
    core::{BSTR, Result},
};

const MARGIN: i32 = 4;
const NO_SCROLL: f64 = -1.0;

// The list is virtualized and the toolkit exposes no scroll offset, so the item is found by name through
// UI Automation and the list's scroll pattern moves it into view; `fraction` is a first guess when it is not realized yet.
pub fn reveal(name: &str, fraction: f64) {
    let _ = try_reveal(name, fraction);
}

fn area(rect: &RECT) -> i64 {
    i64::from(rect.right - rect.left) * i64::from(rect.bottom - rect.top)
}

fn list_scroller(automation: &IUIAutomation, root: &IUIAutomationElement) -> Result<Option<(IUIAutomationElement, RECT)>> {
    unsafe {
        let scrollables = root.FindAll(TreeScope_Descendants, &automation.CreatePropertyCondition(UIA_IsScrollPatternAvailablePropertyId, &VARIANT::from(true))?)?;
        let mut scroller: Option<(IUIAutomationElement, RECT)> = None;
        for index in 0..scrollables.Length()? {
            let element = scrollables.GetElement(index)?;
            let rect = element.CurrentBoundingRectangle()?;
            if scroller.as_ref().is_none_or(|(_, best)| area(&rect) > area(best)) {
                scroller = Some((element, rect));
            }
        }
        Ok(scroller)
    }
}

fn try_reveal(name: &str, fraction: f64) -> Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        let root = automation.ElementFromHandle(GetForegroundWindow())?;
        let scroller = list_scroller(&automation, &root)?;
        let Some((scroller, view)) = scroller else { return Ok(()) };
        let pattern: IUIAutomationScrollPattern = scroller.GetCurrentPatternAs(UIA_ScrollPatternId)?;
        let by_name = automation.CreatePropertyCondition(UIA_NamePropertyId, &VARIANT::from(BSTR::from(name)))?;
        for attempt in 0..5 {
            let found = root.FindAll(TreeScope_Descendants, &by_name)?;
            let mut item = None;
            for index in 0..found.Length()? {
                let rect = found.GetElement(index)?.CurrentBoundingRectangle()?;
                // the sidebar can hold an entry with the same name; only the list's column counts
                let middle = (rect.left + rect.right) / 2;
                if middle >= view.left && middle <= view.right {
                    item = Some(rect);
                    break;
                }
            }
            let Some(rect) = item else {
                if attempt < 4 {
                    pattern.SetScrollPercent(NO_SCROLL, fraction.clamp(0.0, 1.0) * 100.0)?;
                    std::thread::sleep(Duration::from_millis(150));
                    continue;
                }
                return Ok(());
            };
            let delta = if rect.top < view.top + MARGIN {
                rect.top - view.top - MARGIN
            } else if rect.bottom > view.bottom - MARGIN {
                rect.bottom - view.bottom + MARGIN
            } else {
                return Ok(());
            };
            let visible = pattern.CurrentVerticalViewSize()? / 100.0;
            let height = f64::from(view.bottom - view.top);
            let range = height / visible.max(0.001) - height;
            if range > 0.0 {
                let percent = pattern.CurrentVerticalScrollPercent()? + f64::from(delta) / range * 100.0;
                pattern.SetScrollPercent(NO_SCROLL, percent.clamp(0.0, 100.0))?;
                // the extent is an estimate until the rows are realized, so the next pass confirms the result
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            return Ok(());
        }
    }
    Ok(())
}

// Top-left of the named list row in window DIPs, so a keyboard-opened menu can sit beside it.
pub fn anchor(name: &str) -> Option<(f64, f64)> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let window = GetForegroundWindow();
        let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let root = automation.ElementFromHandle(window).ok()?;
        let found = root.FindAll(TreeScope_Descendants, &automation.CreatePropertyCondition(UIA_NamePropertyId, &VARIANT::from(BSTR::from(name))).ok()?).ok()?;
        let (_, view) = list_scroller(&automation, &root).ok()??;
        let mut best: Option<RECT> = None;
        // the sidebar and the preview can show the same name; only the list's column counts
        for index in 0..found.Length().ok()? {
            let rect = found.GetElement(index).ok()?.CurrentBoundingRectangle().ok()?;
            let middle = (rect.left + rect.right) / 2;
            if middle >= view.left && middle <= view.right {
                best = Some(rect);
                break;
            }
        }
        let rect = best?;
        let mut origin = POINT::default();
        let _ = ClientToScreen(window, &mut origin);
        let scale = f64::from(GetDpiForWindow(window)) / 96.0;
        Some((f64::from(rect.left + 40 - origin.x) / scale, f64::from(rect.bottom - origin.y) / scale))
    }
}

// What lies under a screen point: its position in window DIPs, the accessible name of the element there and how far
// down (0..1) the point sits within the largest element of that name, i.e. the whole row rather than its label.
pub fn locate(x: i32, y: i32) -> Option<(f64, f64, String, f64)> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let window = GetForegroundWindow();
        let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let name = automation.ElementFromPoint(POINT { x, y }).ok()?.CurrentName().ok()?.to_string();
        let mut fraction = 0.5;
        if let Ok(root) = automation.ElementFromHandle(window)
            && let Ok(condition) = automation.CreatePropertyCondition(UIA_NamePropertyId, &VARIANT::from(BSTR::from(name.as_str())))
            && let Ok(found) = root.FindAll(TreeScope_Descendants, &condition)
        {
            let row = (0..found.Length().unwrap_or(0))
                .filter_map(|index| found.GetElement(index).ok()?.CurrentBoundingRectangle().ok())
                .filter(|rect| rect.left <= x && x < rect.right && rect.top <= y && y < rect.bottom)
                .max_by_key(area);
            if let Some(rect) = row {
                fraction = f64::from(y - rect.top) / f64::from((rect.bottom - rect.top).max(1));
            }
        }
        let mut origin = POINT::default();
        let _ = ClientToScreen(window, &mut origin);
        let scale = f64::from(GetDpiForWindow(window)) / 96.0;
        Some((f64::from(x - origin.x) / scale, f64::from(y - origin.y) / scale, name, fraction))
    }
}

// Every list row as (name, screen rect `(left, top, right, bottom)`); with `wide` a row spans the whole list width.
pub fn row_rects(wide: bool) -> Vec<(String, (i32, i32, i32, i32))> {
    use windows::Win32::UI::Accessibility::{UIA_ButtonControlTypeId, UIA_ControlTypePropertyId};
    (|| -> Result<Vec<(String, (i32, i32, i32, i32))>> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
            let root = automation.ElementFromHandle(GetForegroundWindow())?;
            let Some((_, view)) = list_scroller(&automation, &root)? else { return Ok(Vec::new()) };
            let buttons = root.FindAll(TreeScope_Descendants, &automation.CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(UIA_ButtonControlTypeId.0))?)?;
            let mut rows = Vec::new();
            for index in 0..buttons.Length()? {
                let element = buttons.GetElement(index)?;
                let row = element.CurrentBoundingRectangle()?;
                let middle = (row.left + row.right) / 2;
                if middle < view.left || middle > view.right || row.bottom <= view.top || row.top >= view.bottom {
                    continue;
                }
                let (left, right) = if wide { (view.left, view.right) } else { (row.left, row.right) };
                rows.push((element.CurrentName()?.to_string(), (left, row.top, right, row.bottom)));
            }
            Ok(rows)
        }
    })()
    .unwrap_or_default()
}
