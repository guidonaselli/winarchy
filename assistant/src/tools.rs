use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Acciones del escritorio (argv fijos) más `run_powershell` y `look_at_screen`, que le dan alcance general.
pub fn definitions(themes: &[String]) -> Vec<Value> {
    vec![
        json!({"type": "function", "name": "focus_workspace", "description": "Go to workspace N (1-10) of the monitor under the mouse.",
            "parameters": {"type": "object", "properties": {"number": {"type": "integer", "minimum": 1, "maximum": 10}}, "required": ["number"], "additionalProperties": false}}),
        json!({"type": "function", "name": "cycle_workspace", "description": "Go to the next or previous workspace.",
            "parameters": {"type": "object", "properties": {"direction": {"type": "string", "enum": ["next", "previous"]}}, "required": ["direction"], "additionalProperties": false}}),
        json!({"type": "function", "name": "open_terminal", "description": "Open a new terminal window.",
            "parameters": {"type": "object", "properties": {}, "additionalProperties": false}}),
        json!({"type": "function", "name": "set_theme", "description": "Switch the desktop theme.",
            "parameters": {"type": "object", "properties": {"name": {"type": "string", "enum": themes}}, "required": ["name"], "additionalProperties": false}}),
        json!({"type": "function", "name": "run_powershell", "description": "Run a PowerShell command on the user's PC (full user permissions) and return its output. Use it for anything else: open apps or URLs, files, system info, git, installs. Times out after 60 s.",
            "parameters": {"type": "object", "properties": {"command": {"type": "string"}}, "required": ["command"], "additionalProperties": false}}),
        json!({"type": "function", "name": "look_at_screen", "description": "Take a screenshot of the monitor with the focused window and look at it. Only when the user asks about what is on screen.",
            "parameters": {"type": "object", "properties": {}, "additionalProperties": false}}),
    ]
}

pub struct Env {
    pub komorebic: PathBuf,
    pub root: PathBuf,
    pub themes: Vec<String>,
}

pub fn env() -> Env {
    let root = std::env::var_os("WINARCHY_ROOT").map(PathBuf::from).unwrap_or_else(|| {
        std::env::current_exe().ok().and_then(|p| p.ancestors().nth(4).map(Path::to_path_buf)).unwrap_or_default()
    });
    let pf = PathBuf::from(r"C:\Program Files\komorebi\bin\komorebic.exe");
    let themes = std::fs::read_dir(root.join("themes"))
        .map(|d| d.flatten().filter(|e| e.path().join("theme.toml").exists()).filter_map(|e| e.file_name().into_string().ok()).collect())
        .unwrap_or_default();
    Env { komorebic: if pf.exists() { pf } else { "komorebic".into() }, root, themes }
}

/// Traduce una llamada del modelo a los comandos que la cumplen; rechaza todo lo que no esté en la lista.
pub fn plan(name: &str, args: &Value, e: &Env) -> Result<Vec<Vec<String>>, String> {
    let k = e.komorebic.to_string_lossy().into_owned();
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    match name {
        "focus_workspace" => {
            let n = args["number"].as_i64().filter(|n| (1..=10).contains(n)).ok_or("number must be 1-10")?;
            Ok(vec![vec![k.clone(), "focus-monitor-at-cursor".into()], vec![k, "focus-workspace".into(), (n - 1).to_string()]])
        }
        "cycle_workspace" => match args["direction"].as_str() {
            Some(d @ ("next" | "previous")) => Ok(vec![vec![k, "cycle-workspace".into(), d.into()]]),
            _ => Err("direction must be next or previous".into()),
        },
        "run_powershell" => {
            let c = args["command"].as_str().filter(|c| !c.trim().is_empty()).ok_or("command is required")?;
            Ok(vec![s(&["powershell.exe", "-NoProfile", "-NonInteractive", "-Command", c])])
        }
        "look_at_screen" => Ok(vec![s(&["powershell.exe", "-NoProfile", "-NonInteractive", "-Command", SCREENSHOT])]),
        "open_terminal" => Ok(vec![s(&["cmd", "/c", "start", "", "wezterm-gui"])]),
        "set_theme" => {
            let t = args["name"].as_str().filter(|t| e.themes.iter().any(|x| x == t)).ok_or("unknown theme")?;
            Ok(vec![vec![e.root.join("bin").join("winarchy.cmd").to_string_lossy().into_owned(), "theme".into(), "set".into(), t.into()]])
        }
        _ => Err(format!("unknown tool {name}")),
    }
}

const SCREENSHOT: &str = "Add-Type -AssemblyName System.Windows.Forms,System.Drawing;\
Add-Type 'using System;using System.Runtime.InteropServices;public class W{[DllImport(\"user32.dll\")]public static extern bool SetProcessDPIAware();[DllImport(\"user32.dll\")]public static extern IntPtr GetForegroundWindow();}';\
[W]::SetProcessDPIAware()|Out-Null;\
$b=[Windows.Forms.Screen]::FromHandle([W]::GetForegroundWindow()).Bounds;\
$s=New-Object Drawing.Bitmap $b.Width,$b.Height;\
[Drawing.Graphics]::FromImage($s).CopyFromScreen($b.Location,[Drawing.Point]::Empty,$b.Size);\
$w=[Math]::Min(2560,$b.Width);$o=New-Object Drawing.Bitmap $s,$w,([int]($b.Height*$w/$b.Width));\
$m=New-Object IO.MemoryStream;$o.Save($m,[Drawing.Imaging.ImageFormat]::Jpeg);\
[Convert]::ToBase64String($m.ToArray())";

const TIMEOUT: Duration = Duration::from_secs(60);
const MAX_OUT: usize = 4000;

/// Resultado de una herramienta: texto para el modelo y, si hubo captura, la imagen como data URL.
pub struct Out {
    pub text: String,
    pub image: Option<String>,
}

fn err(m: String) -> Out {
    Out { text: format!("error: {m}"), image: None }
}

fn drain(r: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut b = Vec::new();
        if let Some(mut r) = r {
            let _ = r.read_to_end(&mut b);
        }
        String::from_utf8_lossy(&b).into_owned()
    })
}

fn exec(c: &[String]) -> Result<(bool, String), String> {
    let mut child = Command::new(&c[0]).args(&c[1..]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    let (o, e) = (drain(child.stdout.take()), drain(child.stderr.take()));
    let t0 = Instant::now();
    let status = loop {
        if let Some(st) = child.try_wait().map_err(|e| e.to_string())? {
            break st;
        }
        if t0.elapsed() > TIMEOUT {
            let _ = child.kill();
            return Err("timed out after 60 s".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let (o, e) = (o.join().unwrap_or_default(), e.join().unwrap_or_default());
    Ok((status.success(), format!("{o}{e}")))
}

pub fn run(name: &str, args: &Value, e: &Env) -> Out {
    let cmds = match plan(name, args, e) {
        Ok(c) => c,
        Err(m) => return err(m),
    };
    let mut text = String::new();
    for c in cmds {
        match exec(&c) {
            Ok((true, out)) => text = out,
            Ok((false, out)) => return err(format!("failed: {}", clip(&out))),
            Err(x) => return err(x),
        }
    }
    if name == "look_at_screen" {
        return Out { text: "screenshot attached".into(), image: Some(format!("data:image/jpeg;base64,{}", text.trim())) };
    }
    Out { text: if text.trim().is_empty() { "ok".into() } else { clip(&text) }, image: None }
}

fn clip(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() <= MAX_OUT {
        return s.to_string();
    }
    format!("{}…[truncated]", s.chars().take(MAX_OUT).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> Env {
        Env { komorebic: "komorebic".into(), root: PathBuf::from("R"), themes: vec!["nord".into()] }
    }

    #[test]
    fn workspace_is_range_checked_and_zero_based() {
        let p = plan("focus_workspace", &json!({"number": 3}), &env()).unwrap();
        assert_eq!(p[1], ["komorebic", "focus-workspace", "2"]);
        assert!(plan("focus_workspace", &json!({"number": 0}), &env()).is_err());
        assert!(plan("focus_workspace", &json!({"number": 11}), &env()).is_err());
        assert!(plan("focus_workspace", &json!({"number": "3; calc"}), &env()).is_err());
    }

    #[test]
    fn only_listed_verbs_and_values_pass() {
        assert!(plan("cycle_workspace", &json!({"direction": "next"}), &env()).is_ok());
        assert!(plan("cycle_workspace", &json!({"direction": "sideways"}), &env()).is_err());
        assert!(plan("set_theme", &json!({"name": "nord"}), &env()).is_ok());
        assert!(plan("set_theme", &json!({"name": "nord & calc"}), &env()).is_err());
        assert!(plan("close_window", &json!({}), &env()).is_err());
        assert!(plan("shell", &json!({"cmd": "calc"}), &env()).is_err());
    }

    #[test]
    fn powershell_needs_a_command_and_passes_it_as_one_argument() {
        let p = plan("run_powershell", &json!({"command": "Get-Date; calc"}), &env()).unwrap();
        assert_eq!(p[0].last().unwrap(), "Get-Date; calc");
        assert!(plan("run_powershell", &json!({"command": "  "}), &env()).is_err());
    }

    #[test]
    fn output_is_captured_and_clipped() {
        let o = run("run_powershell", &json!({"command": "'hola'"}), &env());
        assert_eq!(o.text, "hola");
        let o = run("run_powershell", &json!({"command": "'x' * 5000"}), &env());
        assert!(o.text.ends_with("[truncated]"));
        assert!(run("run_powershell", &json!({"command": "exit 3"}), &env()).text.starts_with("error"));
    }
}
