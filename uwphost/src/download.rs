use std::{ffi::c_void, io::Write, path::Path};
use windows::{
    core::{w, HSTRING, PCWSTR},
    Win32::Networking::WinHttp::{
        WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
        WinHttpSetTimeouts, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_OPEN_REQUEST_FLAGS, WINHTTP_QUERY_CONTENT_LENGTH, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
    },
};

pub struct Source<'a> {
    pub host: &'a str,
    pub port: u16,
    pub secure: bool,
    pub path: &'a str,
}

pub struct Limits {
    /// Per operation: a stalled server fails, a slow link that keeps sending does not.
    pub stall_ms: i32,
    pub max_bytes: u64,
}

struct Handle(*mut c_void);

impl Handle {
    fn new(raw: *mut c_void, what: &str) -> Result<Self, String> {
        if raw.is_null() {
            return Err(format!("{what}: {}", std::io::Error::last_os_error()));
        }
        Ok(Self(raw))
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: This guard owns a WinHTTP handle.
        let _ = unsafe { WinHttpCloseHandle(self.0) };
    }
}

/// Downloads to a temporary file next to `destination` and renames it only when complete.
pub fn fetch(source: &Source, destination: &Path, limits: &Limits) -> Result<(), String> {
    let temporary = destination.with_extension("download");
    let result = receive(source, &temporary, limits).and_then(|()| std::fs::rename(&temporary, destination).map_err(|e| format!("install: {e}")));
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn receive(source: &Source, temporary: &Path, limits: &Limits) -> Result<(), String> {
    // SAFETY: Every handle is checked and closed by its guard; buffers outlive the synchronous calls.
    unsafe {
        let session = Handle::new(WinHttpOpen(w!("winarchy-uwphost"), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0), "open")?;
        let stall = limits.stall_ms;
        WinHttpSetTimeouts(session.0, stall, stall, stall, stall).map_err(|e| format!("timeouts: {e}"))?;
        let connection = Handle::new(WinHttpConnect(session.0, &HSTRING::from(source.host), source.port, 0), "connect")?;
        let flags = if source.secure { WINHTTP_FLAG_SECURE } else { WINHTTP_OPEN_REQUEST_FLAGS(0) };
        let path = HSTRING::from(source.path);
        let request = Handle::new(WinHttpOpenRequest(connection.0, w!("GET"), &path, PCWSTR::null(), PCWSTR::null(), std::ptr::null(), flags), "request")?;
        WinHttpSendRequest(request.0, None, None, 0, 0, 0).map_err(|e| format!("send: {e}"))?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(|e| format!("response: {e}"))?;
        let number = |query: u32| {
            let mut value = 0u32;
            let mut size = size_of::<u32>() as u32;
            WinHttpQueryHeaders(request.0, query | WINHTTP_QUERY_FLAG_NUMBER, PCWSTR::null(), Some((&mut value as *mut u32).cast()), &mut size, std::ptr::null_mut())
                .map(|()| value)
        };
        let status = number(WINHTTP_QUERY_STATUS_CODE).map_err(|e| format!("status: {e}"))?;
        if status != 200 {
            return Err(format!("HTTP {status}"));
        }
        let mut file = std::fs::File::create(temporary).map_err(|e| format!("create: {e}"))?;
        let mut buffer = vec![0u8; 64 * 1024];
        let mut total = 0u64;
        loop {
            let mut read = 0u32;
            WinHttpReadData(request.0, buffer.as_mut_ptr().cast(), buffer.len() as u32, &mut read).map_err(|e| format!("read: {e}"))?;
            if read == 0 {
                break;
            }
            total += u64::from(read);
            if total > limits.max_bytes {
                return Err(format!("response exceeds {} bytes", limits.max_bytes));
            }
            file.write_all(&buffer[..read as usize]).map_err(|e| format!("write: {e}"))?;
        }
        if total == 0 {
            return Err("empty response".into());
        }
        // WinHTTP reports a connection closed early as a normal end
        if number(WINHTTP_QUERY_CONTENT_LENGTH).is_ok_and(|expected| u64::from(expected) != total) {
            return Err(format!("truncated at {total} bytes"));
        }
        file.sync_all().map_err(|e| format!("flush: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Read, net::TcpListener, time::Duration};

    fn serve(reply: &'static [u8], hold: Duration) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = stream.read(&mut [0u8; 4096]);
            let _ = stream.write_all(reply);
            std::thread::sleep(hold);
        });
        port
    }

    fn run(name: &str, reply: &'static [u8], hold: Duration, max_bytes: u64) -> (Result<(), String>, std::path::PathBuf) {
        let port = serve(reply, hold);
        let destination = std::env::temp_dir().join(format!("uwphost-{name}-{}.pdb", std::process::id()));
        let source = Source { host: "127.0.0.1", port, secure: false, path: "/a.pdb" };
        (fetch(&source, &destination, &Limits { stall_ms: 1000, max_bytes }), destination)
    }

    fn discarded(destination: &Path) {
        assert!(!destination.exists() && !destination.with_extension("download").exists());
    }

    #[test]
    fn installs_a_complete_response() {
        let (result, destination) = run("ok", b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nabcd", Duration::ZERO, 16);
        result.unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), b"abcd");
        std::fs::remove_file(destination).unwrap();
    }

    #[test]
    fn a_stalled_server_fails_and_leaves_nothing() {
        let (result, destination) = run("hang", b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nab", Duration::from_secs(4), 16);
        assert!(result.is_err());
        discarded(&destination);
    }

    #[test]
    fn an_oversized_response_is_rejected() {
        let (result, destination) = run("big", b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nabcdefgh", Duration::ZERO, 4);
        assert!(result.unwrap_err().contains("exceeds"));
        discarded(&destination);
    }

    #[test]
    fn a_truncated_response_is_rejected() {
        let (result, destination) = run("cut", b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nab", Duration::ZERO, 16);
        assert!(result.is_err());
        discarded(&destination);
    }

    #[test]
    fn a_missing_symbol_file_is_not_installed() {
        let (result, destination) = run("404", b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", Duration::ZERO, 16);
        assert_eq!(result.unwrap_err(), "HTTP 404");
        discarded(&destination);
    }
}
