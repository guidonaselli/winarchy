use std::sync::mpsc::Sender;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::Storage::FileSystem::{ReadFile, FILE_FLAGS_AND_ATTRIBUTES, PIPE_ACCESS_INBOUND};
use windows::Win32::System::Pipes::*;

pub const NAME: PCWSTR = w!("\\\\.\\pipe\\winarchy-assistant");

#[derive(Debug, PartialEq)]
pub enum Msg {
    Down,
    Up,
    Quit,
}

pub fn parse(line: &str) -> Option<Msg> {
    match line.trim() {
        "ptt.down" => Some(Msg::Down),
        "ptt.up" => Some(Msg::Up),
        "quit" => Some(Msg::Quit),
        _ => None,
    }
}

/// Una línea por mensaje; cada cliente conecta, escribe y cierra. Varias conexiones a la vez: un
/// cliente colgado no puede bloquear a los demás (a diferencia de la shell, que atiende de a uno).
pub fn serve(tx: Sender<Msg>) {
    std::thread::spawn(move || unsafe {
        let mut sd = PSECURITY_DESCRIPTOR::default();
        // Solo el dueño del proceso (el usuario) puede abrir la tubería.
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(w!("D:P(A;;GA;;;OW)"), SDDL_REVISION_1, &mut sd, None).is_err() {
            return;
        }
        let sa = SECURITY_ATTRIBUTES { nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: sd.0, bInheritHandle: false.into() };
        loop {
            let h = CreateNamedPipeW(
                NAME,
                FILE_FLAGS_AND_ATTRIBUTES(PIPE_ACCESS_INBOUND.0),
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                0,
                4096,
                0,
                Some(&sa),
            );
            if h.is_invalid() {
                return;
            }
            if ConnectNamedPipe(h, None).is_ok() || GetLastError() == ERROR_PIPE_CONNECTED {
                let tx = tx.clone();
                let raw = h.0 as usize;
                std::thread::spawn(move || read_lines(HANDLE(raw as *mut _), &tx));
            } else {
                let _ = CloseHandle(h);
            }
        }
    });
}

unsafe fn read_lines(h: HANDLE, tx: &Sender<Msg>) {
    let (mut pending, mut buf) = (Vec::new(), [0u8; 256]);
    loop {
        let mut n = 0u32;
        if ReadFile(h, Some(&mut buf), Some(&mut n), None).is_err() || n == 0 {
            break;
        }
        pending.extend_from_slice(&buf[..n as usize]);
        while let Some(i) = pending.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = pending.drain(..=i).collect();
            if let Some(m) = parse(&String::from_utf8_lossy(&line)) {
                let _ = tx.send(m);
            }
        }
        if pending.len() > 4096 {
            break;
        }
    }
    let _ = DisconnectNamedPipe(h);
    let _ = CloseHandle(h);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_known_lines() {
        assert_eq!(parse("ptt.down\r\n"), Some(Msg::Down));
        assert_eq!(parse("ptt.up"), Some(Msg::Up));
        assert_eq!(parse("rm -rf"), None);
    }
}
