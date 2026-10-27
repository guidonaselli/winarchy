use serde::Deserialize;
use serde_json::{json, Value};
use std::io::Write;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::Storage::FileSystem::{ReadFile, WriteFile, FILE_FLAGS_AND_ATTRIBUTES, PIPE_ACCESS_DUPLEX};
use windows::Win32::System::Pipes::*;

pub const PIPE_NAME: PCWSTR = w!("\\\\.\\pipe\\winarchy-shell");
const VERSION: u32 = 1;

#[derive(Deserialize)]
struct Request {
    v: u32,
    id: Value,
    cmd: String,
    #[serde(default)]
    params: Value,
}

#[derive(Clone, Default, PartialEq, Debug)]
pub struct Island {
    pub title: String,
    pub detail: String,
    pub progress: Option<f64>,
}

pub static ISLAND: std::sync::Mutex<Option<Island>> = std::sync::Mutex::new(None);

const MAX_TEXT: usize = 120;

// Solo campos estructurados y acotados: el cliente nunca dibuja UI arbitraria.
fn island_from(params: &Value) -> Result<Island, &'static str> {
    let text = |k: &str| match params.get(k) {
        None => Ok(String::new()),
        Some(Value::String(s)) if s.chars().count() <= MAX_TEXT => Ok(s.clone()),
        _ => Err("bad field"),
    };
    let progress = match params.get("progress") {
        None | Some(Value::Null) => None,
        Some(v) => match v.as_f64() {
            Some(p) if (0.0..=1.0).contains(&p) => Some(p),
            _ => return Err("bad field"),
        },
    };
    Ok(Island { title: text("title")?, detail: text("detail")?, progress })
}

pub enum Command {
    Reload,
    Quit,
    Redraw,
    Expand,
}

// Un cliente por vez; el hilo de UI recibe Reload/Quit por `dispatch`, nunca bloquea acá.
pub fn serve(dispatch: impl Fn(Command) + Send + 'static) {
    std::thread::spawn(move || unsafe {
        // Solo el dueño del proceso (el usuario) puede abrir la tubería.
        let mut sd = PSECURITY_DESCRIPTOR::default();
        let ok = ConvertStringSecurityDescriptorToSecurityDescriptorW(w!("D:P(A;;GA;;;OW)"), SDDL_REVISION_1, &mut sd, None);
        if ok.is_err() {
            return;
        }
        let sa = SECURITY_ATTRIBUTES { nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: sd.0, bInheritHandle: false.into() };
        loop {
            let h = CreateNamedPipeW(
                PIPE_NAME,
                FILE_FLAGS_AND_ATTRIBUTES(PIPE_ACCESS_DUPLEX.0),
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                4096,
                4096,
                0,
                Some(&sa),
            );
            if h.is_invalid() {
                return;
            }
            if ConnectNamedPipe(h, None).is_ok() || GetLastError() == ERROR_PIPE_CONNECTED {
                session(h, &dispatch);
            }
            let _ = DisconnectNamedPipe(h);
            let _ = CloseHandle(h);
        }
    });
}

unsafe fn session(h: HANDLE, dispatch: &impl Fn(Command)) {
    let mut pending = Vec::new();
    let mut buf = [0u8; 1024];
    loop {
        let mut n = 0u32;
        if ReadFile(h, Some(&mut buf), Some(&mut n), None).is_err() || n == 0 {
            return;
        }
        pending.extend_from_slice(&buf[..n as usize]);
        while let Some(i) = pending.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = pending.drain(..=i).collect();
            let mut reply = handle(&line, dispatch).to_string().into_bytes();
            reply.write_all(b"\n").ok();
            let mut w = 0u32;
            if WriteFile(h, Some(&reply), Some(&mut w), None).is_err() {
                return;
            }
        }
        if pending.len() > 64 * 1024 {
            return;
        }
    }
}

fn handle(line: &[u8], dispatch: &impl Fn(Command)) -> Value {
    let Ok(req) = serde_json::from_slice::<Request>(line) else {
        return json!({"v": VERSION, "id": null, "ok": false, "error": "bad request"});
    };
    let ok = |extra: Value| {
        let mut r = json!({"v": VERSION, "id": req.id, "ok": true});
        if let (Some(r), Some(e)) = (r.as_object_mut(), extra.as_object()) {
            r.extend(e.clone());
        }
        r
    };
    if req.v != VERSION {
        return json!({"v": VERSION, "id": req.id, "ok": false, "error": "unsupported version"});
    }
    match req.cmd.as_str() {
        "ping" => ok(json!({"pong": true})),
        "version" => ok(json!({"shell": env!("CARGO_PKG_VERSION")})),
        "hello" => ok(json!({"shell": env!("CARGO_PKG_VERSION"), "caps": ["island", "assistant"]})),
        "island.present" | "island.update" => match island_from(&req.params) {
            Ok(i) => {
                *ISLAND.lock().unwrap() = Some(i);
                dispatch(Command::Redraw);
                ok(json!({}))
            }
            Err(e) => json!({"v": VERSION, "id": req.id, "ok": false, "error": e}),
        },
        // El asistente (proceso aparte) publica su estado; la isla solo lo dibuja.
        "assistant.state" => match req.params["state"].as_str().and_then(crate::spirit::Assist::parse) {
            Some(a) => {
                crate::spirit::set(a);
                dispatch(Command::Redraw);
                ok(json!({}))
            }
            None => json!({"v": VERSION, "id": req.id, "ok": false, "error": "bad state"}),
        },
        "island.expand" => {
            dispatch(Command::Expand);
            ok(json!({}))
        }
        "island.dismiss" => {
            *ISLAND.lock().unwrap() = None;
            dispatch(Command::Redraw);
            ok(json!({}))
        }
        // Sin UI de confirmación todavía: se deniega siempre.
        "confirm.request" | "capture.request" => ok(json!({"result": "denied"})),
        "reload" => {
            dispatch(Command::Reload);
            ok(json!({}))
        }
        "quit" => {
            dispatch(Command::Quit);
            ok(json!({}))
        }
        other => json!({"v": VERSION, "id": req.id, "ok": false, "error": format!("unknown command {other}")}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_ping_and_rejects_garbage() {
        let r = handle(br#"{"v":1,"id":7,"cmd":"ping"}"#, &|_| {});
        assert_eq!(r["ok"], true);
        assert_eq!(r["id"], 7);
        assert_eq!(handle(b"nope", &|_| {})["ok"], false);
        assert_eq!(handle(br#"{"v":2,"id":1,"cmd":"ping"}"#, &|_| {})["ok"], false);
        assert_eq!(handle(br#"{"v":1,"id":1,"cmd":"zzz"}"#, &|_| {})["ok"], false);
    }

    #[test]
    fn island_validates_fields_and_confirm_denies() {
        let r = handle(br#"{"v":1,"id":1,"cmd":"island.present","params":{"title":"hi","progress":0.5}}"#, &|_| {});
        assert_eq!(r["ok"], true);
        assert_eq!(ISLAND.lock().unwrap().as_ref().unwrap().title, "hi");
        assert_eq!(handle(br#"{"v":1,"id":1,"cmd":"island.update","params":{"progress":2}}"#, &|_| {})["ok"], false);
        assert_eq!(handle(br#"{"v":1,"id":1,"cmd":"island.dismiss"}"#, &|_| {})["ok"], true);
        assert!(ISLAND.lock().unwrap().is_none());
        assert_eq!(handle(br#"{"v":1,"id":1,"cmd":"assistant.state","params":{"state":"thinking"}}"#, &|_| {})["ok"], true);
        assert_eq!(crate::spirit::current().0, crate::spirit::Assist::Thinking);
        assert_eq!(handle(br#"{"v":1,"id":1,"cmd":"assistant.state","params":{"state":"dancing"}}"#, &|_| {})["ok"], false);
        handle(br#"{"v":1,"id":1,"cmd":"assistant.state","params":{"state":"idle"}}"#, &|_| {});
        assert_eq!(crate::spirit::current().0, crate::spirit::Assist::Idle);
        assert_eq!(handle(br#"{"v":1,"id":1,"cmd":"confirm.request"}"#, &|_| {})["result"], "denied");
    }
}
