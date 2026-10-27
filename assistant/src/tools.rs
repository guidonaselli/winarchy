use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Lista cerrada de acciones: cada una se traduce a argv fijos, nunca a un string de shell.
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
        "open_terminal" => Ok(vec![s(&["cmd", "/c", "start", "", "wezterm-gui"])]),
        "set_theme" => {
            let t = args["name"].as_str().filter(|t| e.themes.iter().any(|x| x == t)).ok_or("unknown theme")?;
            Ok(vec![vec![e.root.join("bin").join("winarchy.cmd").to_string_lossy().into_owned(), "theme".into(), "set".into(), t.into()]])
        }
        _ => Err(format!("unknown tool {name}")),
    }
}

pub fn run(name: &str, args: &Value, e: &Env) -> String {
    let cmds = match plan(name, args, e) {
        Ok(c) => c,
        Err(m) => return format!("error: {m}"),
    };
    for c in cmds {
        match Command::new(&c[0]).args(&c[1..]).status() {
            Ok(st) if st.success() => {}
            Ok(st) => return format!("error: exit {st}"),
            Err(x) => return format!("error: {x}"),
        }
    }
    "ok".into()
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
}
