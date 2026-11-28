// Una app en modo de ahorro a la vista no llega a ejecutar con el equipo cargado y queda "no responde".
use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM};
use windows::Win32::System::Threading::{
    GetPriorityClass, GetProcessInformation, OpenProcess, ProcessPowerThrottling, SetPriorityClass, SetProcessInformation, IDLE_PRIORITY_CLASS,
    NORMAL_PRIORITY_CLASS, PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetForegroundWindow, GetWindowThreadProcessId, IsHungAppWindow, IsWindowVisible};

const EVERY: std::time::Duration = std::time::Duration::from_secs(2);

pub fn spawn(root: std::path::PathBuf) {
    std::thread::spawn(move || loop {
        std::thread::sleep(EVERY);
        unsafe {
            let foreground = GetForegroundWindow();
            let mut windows: Vec<(HWND, bool)> = Vec::new();
            let _ = EnumWindows(Some(collect_hung), LPARAM(&mut windows as *mut _ as isize));
            windows.push((foreground, false));
            for (window, hung) in windows {
                let mut pid = 0;
                GetWindowThreadProcessId(window, Some(&mut pid));
                if pid != 0 && rescue(pid, hung, window == foreground) {
                    crate::boot_log(&root, &format!("rescued from idle priority: pid={pid} hung={hung} {}", crate::dock::exe_of(window)));
                }
            }
        }
    });
}

unsafe extern "system" fn collect_hung(window: HWND, out: LPARAM) -> BOOL {
    if IsWindowVisible(window).as_bool() && IsHungAppWindow(window).as_bool() {
        (*(out.0 as *mut Vec<(HWND, bool)>)).push((window, true));
    }
    true.into()
}

// Solo se rescata el modo de ahorro (Idle + EcoQoS): un Idle sin EcoQoS lo eligió alguien.
unsafe fn rescue(pid: u32, hung: bool, foreground: bool) -> bool {
    // la escritura se pide recién con el modo confirmado: un juego con anti-cheat al frente solo recibe la consulta
    if !(hung || foreground) || !saving(pid) {
        return false;
    }
    let Ok(process) = OpenProcess(PROCESS_SET_INFORMATION, false, pid) else { return false };
    let rescued = SetPriorityClass(process, NORMAL_PRIORITY_CLASS).is_ok();
    if rescued {
        // máscaras en cero: Windows vuelve a decidir la velocidad de ejecución
        let reset = PROCESS_POWER_THROTTLING_STATE { Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION, ..Default::default() };
        let _ = SetProcessInformation(process, ProcessPowerThrottling, &reset as *const _ as *const _, std::mem::size_of_val(&reset) as u32);
    }
    let _ = CloseHandle(process);
    rescued
}

unsafe fn saving(pid: u32) -> bool {
    let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
    let mut state = PROCESS_POWER_THROTTLING_STATE { Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION, ..Default::default() };
    let size = std::mem::size_of_val(&state) as u32;
    let saving = GetPriorityClass(process) == IDLE_PRIORITY_CLASS.0
        && GetProcessInformation(process, ProcessPowerThrottling, &mut state as *mut _ as *mut _, size).is_ok()
        && state.ControlMask & state.StateMask & PROCESS_POWER_THROTTLING_EXECUTION_SPEED != 0;
    let _ = CloseHandle(process);
    saving
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::process::CommandExt;
    use windows::Win32::Foundation::HANDLE;

    #[test]
    fn only_a_visible_process_in_power_saving_gets_its_priority_back() {
        let mut child = std::process::Command::new("cmd.exe")
            .args(["/c", "pause"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .spawn()
            .unwrap();
        let (pid, process) = (child.id(), HANDLE(child.as_raw_handle()));
        unsafe {
            assert!(!rescue(pid, true, true), "a normal process is left alone");
            SetPriorityClass(process, IDLE_PRIORITY_CLASS).unwrap();
            assert!(!rescue(pid, false, false), "background work stays idle");
            assert!(!rescue(pid, false, true), "idle without throttling was chosen by someone");
            assert!(!rescue(pid, true, false), "also when hung");
            let eco = PROCESS_POWER_THROTTLING_STATE { Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION, ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED, StateMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED };
            let throttle = || SetProcessInformation(process, ProcessPowerThrottling, &eco as *const _ as *const _, std::mem::size_of_val(&eco) as u32).unwrap();
            throttle();
            assert!(!rescue(pid, false, false), "background power saving is left alone");
            for (hung, foreground) in [(false, true), (true, false)] {
                SetPriorityClass(process, IDLE_PRIORITY_CLASS).unwrap();
                throttle();
                assert!(rescue(pid, hung, foreground));
                assert_eq!(GetPriorityClass(process), NORMAL_PRIORITY_CLASS.0);
            }
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}
