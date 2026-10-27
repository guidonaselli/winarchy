use serde_json::{json, Value};
use std::io::{BufRead, BufReader};

const URL: &str = "https://api.openai.com/v1/responses";
const MAX_ROUNDS: usize = 4;
const MAX_HISTORY: usize = 24;

pub const INSTRUCTIONS: &str = "You are Winarchy, a voice assistant living in the user's Windows desktop. \
The user talks to you by voice and you answer by voice: reply in Spanish (rioplatense), in one or two short, \
natural sentences, with no lists, markdown or emojis. Use the tools to act on the desktop when asked; \
after acting, confirm briefly. If you cannot do something, say so plainly.";

pub struct Llm {
    pub model: String,
    pub effort: String,
    history: Vec<Value>,
}

/// Saca de `buf` las oraciones completas (terminador seguido de espacio) y deja el resto.
pub fn take_sentences(buf: &mut String) -> Vec<String> {
    let mut out = Vec::new();
    let chars: Vec<(usize, char)> = buf.char_indices().collect();
    let mut start = 0;
    for w in chars.windows(2) {
        let ((i, c), (j, next)) = (w[0], w[1]);
        if matches!(c, '.' | '?' | '!' | '\n') && next.is_whitespace() && buf[start..j].trim().chars().count() >= 8 {
            out.push(buf[start..=i].trim().to_string());
            start = j;
        }
    }
    buf.drain(..start);
    out
}

impl Llm {
    pub fn new(model: &str, effort: &str) -> Self {
        Self { model: model.into(), effort: effort.into(), history: Vec::new() }
    }

    fn trim(&mut self) {
        while self.history.len() > MAX_HISTORY {
            self.history.remove(0);
        }
        // Un function_call_output sin su function_call previo es un error de la API.
        while self.history.first().is_some_and(|m| m["type"] == "function_call_output") {
            self.history.remove(0);
        }
    }

    /// Un turno de voz: transmite oraciones a `say` y ejecuta tools con `act`; devuelve el texto final.
    pub fn turn(&mut self, token: &str, user: &str, tools: &[Value], say: &mut dyn FnMut(&str), act: &mut dyn FnMut(&str, &Value) -> String) -> Result<String, String> {
        self.history.push(json!({"role": "user", "content": user}));
        let mut spoken = String::new();
        for _ in 0..MAX_ROUNDS {
            self.trim();
            let mut body = json!({"model": self.model, "instructions": INSTRUCTIONS, "input": self.history, "tools": tools, "store": false, "stream": true});
            if !self.effort.is_empty() {
                body["reasoning"] = json!({"effort": self.effort});
            }
            let resp = ureq::post(URL).set("Authorization", &format!("Bearer {token}")).send_json(body).map_err(|e| match e {
                ureq::Error::Status(c, r) => format!("responses {c}: {}", r.into_string().unwrap_or_default().chars().take(400).collect::<String>()),
                e => e.to_string(),
            })?;
            let (mut buf, mut calls) = (String::new(), Vec::new());
            let mut event = String::new();
            for line in BufReader::new(resp.into_reader()).lines() {
                let line = line.map_err(|e| e.to_string())?;
                if let Some(e) = line.strip_prefix("event:") {
                    event = e.trim().to_string();
                } else if let Some(d) = line.strip_prefix("data:") {
                    let v: Value = serde_json::from_str(d.trim()).unwrap_or(Value::Null);
                    match event.as_str() {
                        "response.output_text.delta" => {
                            buf.push_str(v["delta"].as_str().unwrap_or(""));
                            for s in take_sentences(&mut buf) {
                                say(&s);
                                spoken.push_str(&s);
                                spoken.push(' ');
                            }
                        }
                        "response.output_item.done" if v["item"]["type"] == "function_call" => calls.push(v["item"].clone()),
                        "response.failed" | "response.incomplete" => {
                            return Err(format!("{event}: {}", v["response"]["error"]["code"].as_str().unwrap_or("unknown")));
                        }
                        _ => {}
                    }
                }
            }
            if !buf.trim().is_empty() {
                say(buf.trim());
                spoken.push_str(buf.trim());
            }
            if calls.is_empty() {
                self.history.push(json!({"role": "assistant", "content": spoken.trim()}));
                return Ok(spoken.trim().to_string());
            }
            for c in calls {
                let args: Value = serde_json::from_str(c["arguments"].as_str().unwrap_or("{}")).unwrap_or(Value::Null);
                let out = act(c["name"].as_str().unwrap_or(""), &args);
                self.history.push(json!({"type": "function_call", "call_id": c["call_id"], "name": c["name"], "arguments": c["arguments"]}));
                self.history.push(json!({"type": "function_call_output", "call_id": c["call_id"], "output": out}));
            }
        }
        Err("too many tool rounds".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentences_are_cut_at_terminator_plus_space() {
        let mut b = "Listo, pasé al workspace tres. Abrí la termi".to_string();
        assert_eq!(take_sentences(&mut b), ["Listo, pasé al workspace tres."]);
        assert_eq!(b.trim(), "Abrí la termi");
    }

    #[test]
    fn decimals_and_short_fragments_do_not_split() {
        let mut b = "La versión 3.5 anda bien. ".to_string();
        assert_eq!(take_sentences(&mut b), ["La versión 3.5 anda bien."]);
        let mut c = "Sí. No sé bien qué decir ahora. ".to_string();
        assert_eq!(take_sentences(&mut c), ["Sí. No sé bien qué decir ahora."]);
    }

    #[test]
    fn trim_never_leaves_an_orphan_tool_output() {
        let mut l = Llm::new("m", "");
        l.history.push(json!({"type": "function_call_output", "call_id": "1", "output": "ok"}));
        l.history.push(json!({"role": "user", "content": "hola"}));
        l.trim();
        assert_eq!(l.history.len(), 1);
    }
}
