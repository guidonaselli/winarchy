use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const AUTH: &str = "https://auth.openai.com/api/accounts/authorize";
const TOKEN: &str = "https://auth.openai.com/api/accounts/oauth/token";
const RESOURCE: &str = "https://api.openai.com/v1";
const SCOPE: &str = "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
const NEEDED: &str = "chatgpt.tokens.use.direct";
const PORT: u16 = 1455;

#[derive(Serialize, Deserialize, Clone)]
pub struct Session {
    pub client_id: String,
    pub host_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

fn rand_b64(n: usize) -> String {
    let mut b = vec![0u8; n];
    getrandom::getrandom(&mut b).expect("rng");
    B64.encode(b)
}

pub fn challenge(verifier: &str) -> String {
    B64.encode(Sha256::digest(verifier.as_bytes()))
}

fn enc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn dec(s: &str) -> String {
    let b = s.as_bytes();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap_or(b'?'));
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query(url: &str) -> std::collections::HashMap<String, String> {
    let q = url.split_once('?').map_or("", |x| x.1);
    q.split('&').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.to_string(), dec(v))).collect()
}

/// Claims del id_token. La firma no se verifica: llega directo del endpoint de tokens por TLS (OIDC Core 3.1.3.7).
pub fn check_id_token(id_token: &str, client_id: &str, nonce: &str, now: u64) -> Result<(), String> {
    let payload = id_token.split('.').nth(1).ok_or("malformed id_token")?;
    let v: serde_json::Value = serde_json::from_slice(&B64.decode(payload).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    if v["iss"] != "https://auth.openai.com" {
        return Err("bad issuer".into());
    }
    if !(v["aud"] == client_id || v["aud"].as_array().is_some_and(|a| a.iter().any(|x| x == client_id))) {
        return Err("bad audience".into());
    }
    if v["nonce"] != nonce {
        return Err("bad nonce".into());
    }
    if v["exp"].as_u64().is_none_or(|e| e <= now) {
        return Err("expired id_token".into());
    }
    Ok(())
}

fn store_path() -> PathBuf {
    PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into())).join("Winarchy").join("assistant-session.bin")
}

pub fn load() -> Option<Session> {
    let blob = std::fs::read(store_path()).ok()?;
    serde_json::from_slice(&dpapi::unprotect(&blob)?).ok()
}

fn save(s: &Session) -> Result<(), String> {
    let p = store_path();
    std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
    let blob = dpapi::protect(&serde_json::to_vec(s).unwrap()).ok_or("DPAPI failed")?;
    let tmp = p.with_extension("tmp");
    std::fs::write(&tmp, blob).and_then(|_| std::fs::rename(&tmp, &p)).map_err(|e| e.to_string())
}

pub fn logout() {
    let _ = std::fs::remove_file(store_path());
}

fn token_post(form: &[(&str, &str)]) -> Result<serde_json::Value, String> {
    ureq::post(TOKEN)
        .send_form(form)
        .map_err(|e| match e {
            ureq::Error::Status(c, r) => format!("token endpoint {c}: {}", r.into_string().unwrap_or_default().chars().take(300).collect::<String>()),
            e => e.to_string(),
        })?
        .into_json()
        .map_err(|e| e.to_string())
}

fn new_host_id() -> String {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).expect("rng");
    b[6] = b[6] & 0x0f | 0x40;
    b[8] = b[8] & 0x3f | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("urn:uuid:{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

pub fn login() -> Result<Session, String> {
    let host_id = load().map(|s| s.host_id).unwrap_or_else(new_host_id);
    let (verifier, state, nonce) = (rand_b64(32), rand_b64(16), rand_b64(16));
    let redirect = format!("http://127.0.0.1:{PORT}/auth/callback");
    let listener = TcpListener::bind(("127.0.0.1", PORT)).map_err(|e| format!("port {PORT}: {e}"))?;
    let params = [
        ("client_id", "dynamic_agent_client"),
        ("agent_name_hint", "Winarchy"),
        ("ext_agent_host_id", &host_id),
        ("response_type", "code"),
        ("redirect_uri", &redirect),
        ("scope", SCOPE),
        ("resource", RESOURCE),
        ("state", &state),
        ("nonce", &nonce),
        ("code_challenge_method", "S256"),
        ("code_challenge", &challenge(&verifier)),
    ];
    let url = format!("{AUTH}?{}", params.iter().map(|(k, v)| format!("{k}={}", enc(v))).collect::<Vec<_>>().join("&"));
    std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", &url]).spawn().map_err(|e| e.to_string())?;
    let (mut sock, _) = listener.accept().map_err(|e| e.to_string())?;
    let mut buf = [0u8; 4096];
    let n = sock.read(&mut buf).map_err(|e| e.to_string())?;
    let target = String::from_utf8_lossy(&buf[..n]).lines().next().unwrap_or("").split(' ').nth(1).unwrap_or("").to_string();
    let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<h3>Listo, ya pod&eacute;s cerrar esta pesta&ntilde;a.</h3>");
    let q = query(&target);
    if let Some(e) = q.get("error") {
        return Err(format!("login cancelled: {e}"));
    }
    if q.get("state") != Some(&state) {
        return Err("state mismatch".into());
    }
    let client_id = q.get("client_id").ok_or("registration incomplete: no issued client_id")?.clone();
    let t = token_post(&[
        ("grant_type", "authorization_code"),
        ("client_id", &client_id),
        ("code", q.get("code").ok_or("no code")?),
        ("code_verifier", &verifier),
        ("redirect_uri", &redirect),
        ("resource", RESOURCE),
    ])?;
    if !t["scope"].as_str().unwrap_or("").split(' ').any(|s| s == NEEDED) {
        return Err(format!("scope {NEEDED} not granted"));
    }
    check_id_token(t["id_token"].as_str().ok_or("no id_token")?, &client_id, &nonce, now())?;
    let s = Session {
        client_id,
        host_id,
        access_token: t["access_token"].as_str().ok_or("no access_token")?.into(),
        refresh_token: t["refresh_token"].as_str().ok_or("no refresh_token")?.into(),
        expires_at: now() + t["expires_in"].as_u64().unwrap_or(3600),
    };
    save(&s)?;
    Ok(s)
}

/// Access token vigente; renueva (y guarda el refresh rotado) si le quedan menos de 2 min.
pub fn token() -> Result<String, String> {
    let mut s = load().ok_or("not logged in: run `winarchy assistant login`")?;
    if s.expires_at > now() + 120 {
        return Ok(s.access_token);
    }
    let t = token_post(&[("grant_type", "refresh_token"), ("client_id", &s.client_id), ("refresh_token", &s.refresh_token), ("resource", RESOURCE)])?;
    s.access_token = t["access_token"].as_str().ok_or("no access_token")?.into();
    if let Some(r) = t["refresh_token"].as_str() {
        s.refresh_token = r.into();
    }
    s.expires_at = now() + t["expires_in"].as_u64().unwrap_or(3600);
    save(&s)?;
    Ok(s.access_token)
}

mod dpapi {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB};

    fn run(data: &[u8], protect: bool) -> Option<Vec<u8>> {
        unsafe {
            let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
            let mut out = CRYPT_INTEGER_BLOB::default();
            let ok = if protect {
                CryptProtectData(&input, None, None, None, None, 0, &mut out)
            } else {
                CryptUnprotectData(&input, None, None, None, None, 0, &mut out)
            };
            ok.ok()?;
            let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            let _ = LocalFree(HLOCAL(out.pbData as *mut _));
            Some(v)
        }
    }

    pub fn protect(d: &[u8]) -> Option<Vec<u8>> {
        run(d, true)
    }

    pub fn unprotect(d: &[u8]) -> Option<Vec<u8>> {
        run(d, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_rfc7636_vector() {
        assert_eq!(challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    fn jwt(claims: &str) -> String {
        format!("h.{}.s", B64.encode(claims))
    }

    #[test]
    fn id_token_claims_are_enforced() {
        let ok = jwt(r#"{"iss":"https://auth.openai.com","aud":"c1","nonce":"n","exp":2000}"#);
        assert!(check_id_token(&ok, "c1", "n", 1000).is_ok());
        assert!(check_id_token(&ok, "c2", "n", 1000).is_err());
        assert!(check_id_token(&ok, "c1", "x", 1000).is_err());
        assert!(check_id_token(&ok, "c1", "n", 2000).is_err());
        assert!(check_id_token(&jwt(r#"{"iss":"https://evil","aud":"c1","nonce":"n","exp":2000}"#), "c1", "n", 1000).is_err());
    }

    #[test]
    fn callback_query_is_decoded() {
        let q = query("/auth/callback?code=a%2Fb&state=xyz&client_id=oaiapp_1");
        assert_eq!(q["code"], "a/b");
        assert_eq!(q["client_id"], "oaiapp_1");
    }

    #[test]
    fn dpapi_roundtrip() {
        let b = dpapi::protect(b"secreto").unwrap();
        assert_ne!(b, b"secreto");
        assert_eq!(dpapi::unprotect(&b).unwrap(), b"secreto");
    }
}
