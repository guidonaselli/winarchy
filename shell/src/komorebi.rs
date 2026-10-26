use serde_json::Value;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::sync::Mutex;
use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::Storage::FileSystem::{FILE_FLAGS_AND_ATTRIBUTES, PIPE_ACCESS_INBOUND};
use windows::Win32::System::Pipes::*;

const PIPE: &str = r"\\.\pipe\winarchy-shell-komorebi";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Default, PartialEq, Debug)]
pub struct Workspaces {
    pub occupied: Vec<bool>,
    pub focused: usize,
}

pub static CURRENT: Mutex<Option<Workspaces>> = Mutex::new(None);

pub fn parse(event: &Value) -> Option<Workspaces> {
    let monitors = event.get("state")?.get("monitors")?.get("elements")?.as_array()?;
    let primary = monitors.iter().find(|m| m["size"]["left"] == 0 && m["size"]["top"] == 0).or_else(|| monitors.first())?;
    let ws = &primary["workspaces"];
    let occupied = ws["elements"]
        .as_array()?
        .iter()
        .map(|w| {
            let has = |k: &str| w[k]["elements"].as_array().is_some_and(|a| !a.is_empty());
            has("containers") || has("floating_windows") || !w["monocle_container"].is_null()
        })
        .collect();
    Some(Workspaces { occupied, focused: ws["focused"].as_u64()? as usize })
}

// Hilo propio: un suscriptor que no lee degrada a komorebi (probe 1.4), así que nunca se lee desde la UI.
pub fn spawn(on_change: impl Fn() + Send + 'static) {
    std::thread::spawn(move || loop {
        run_once(&on_change);
        std::thread::sleep(std::time::Duration::from_secs(2));
    });
}

fn run_once(on_change: &impl Fn()) {
    let name: Vec<u16> = PIPE.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let h = CreateNamedPipeW(PCWSTR(name.as_ptr()), FILE_FLAGS_AND_ATTRIBUTES(PIPE_ACCESS_INBOUND.0), PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS, 1, 0, 1 << 20, 0, None);
        if h.is_invalid() {
            return;
        }
        let sub = std::process::Command::new("komorebic.exe").args(["subscribe-pipe", "winarchy-shell-komorebi"]).creation_flags(CREATE_NO_WINDOW).status();
        if sub.is_ok_and(|s| s.success()) && (ConnectNamedPipe(h, None).is_ok() || GetLastError() == ERROR_PIPE_CONNECTED) {
            read_events(h, on_change);
        }
        let _ = DisconnectNamedPipe(h);
        let _ = CloseHandle(h);
    }
}

unsafe fn read_events(h: HANDLE, on_change: &impl Fn()) {
    use std::os::windows::io::FromRawHandle;
    let mut file = std::mem::ManuallyDrop::new(std::fs::File::from_raw_handle(h.0));
    let mut pending = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = match file.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        pending.extend_from_slice(&buf[..n]);
        while let Some(i) = pending.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = pending.drain(..=i).collect();
            if let Some(ws) = serde_json::from_slice::<Value>(&line).ok().as_ref().and_then(parse) {
                let mut cur = CURRENT.lock().unwrap();
                if cur.as_ref() != Some(&ws) {
                    *cur = Some(ws);
                    drop(cur);
                    on_change();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_primary_monitor_occupancy_and_focus() {
        let ev = serde_json::json!({"state":{"monitors":{"elements":[
            {"size":{"left":2560,"top":0},"workspaces":{"focused":0,"elements":[{"containers":{"elements":[1]}}]}},
            {"size":{"left":0,"top":0},"workspaces":{"focused":2,"elements":[
                {"containers":{"elements":[]},"floating_windows":{"elements":[]},"monocle_container":null},
                {"containers":{"elements":[1]},"floating_windows":{"elements":[]},"monocle_container":null},
                {"containers":{"elements":[]},"floating_windows":{"elements":[]},"monocle_container":null}]}}]}}});
        let ws = parse(&ev).unwrap();
        assert_eq!(ws.focused, 2);
        assert_eq!(ws.occupied, vec![false, true, false]);
    }

    #[test]
    fn ignores_events_without_state() {
        assert!(parse(&serde_json::json!({"event":{}})).is_none());
    }
}
