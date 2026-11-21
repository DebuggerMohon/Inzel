//! UI translations. en + bn are built in; other languages are translated from English with Google Translate
//! the first time they are chosen, then cached in %APPDATA%\Inzel\locales\<code>.json.
use crate::config::config_dir;
use crate::translate::translate_batch;
use serde_json::Value;
use std::collections::HashMap;

const EN: &str = include_str!("../locales/en.json");
const BN: &str = include_str!("../locales/bn.json");

type Dict = HashMap<String, String>;

fn parse(text: &str) -> Dict {
    let mut d = Dict::new();
    if let Ok(Value::Object(m)) = serde_json::from_str::<Value>(text) {
        for (k, v) in m {
            if let Value::String(s) = v {
                d.insert(k, s);
            }
        }
    }
    d
}

fn user_dir() -> std::path::PathBuf {
    let d = config_dir().join("locales");
    let _ = std::fs::create_dir_all(&d);
    d
}

fn read(code: &str) -> Dict {
    match code {
        "en" => parse(EN),
        "bn" => parse(BN),
        _ => std::fs::read_to_string(user_dir().join(format!("{code}.json"))).map(|t| parse(&t)).unwrap_or_default(),
    }
}

pub fn has_locale(code: &str) -> bool {
    !read(code).is_empty()
}

/// Splits "Hello {name}!" into [("lit","Hello "),("ph","{name}"),("lit","!")].
fn split_placeholders(s: &str) -> Vec<(bool, String)> {
    let chars: Vec<char> = s.chars().collect();
    let mut out: Vec<(bool, String)> = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '{' {
            if let Some(end) = chars[i..].iter().position(|c| *c == '}') {
                let inner: String = chars[i + 1..i + end].iter().collect();
                if !inner.is_empty() && inner.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    if !cur.is_empty() {
                        out.push((false, std::mem::take(&mut cur)));
                    }
                    out.push((true, chars[i..=i + end].iter().collect()));
                    i += end + 1;
                    continue;
                }
            }
        }
        cur.push(chars[i]);
        i += 1;
    }
    if !cur.is_empty() {
        out.push((false, cur));
    }
    out
}

/// Translate en.json into `code` and cache it. Blocking (network).
pub fn build_locale(code: &str) -> Result<(), String> {
    let en = read("en");
    let mut keys: Vec<&String> = en.keys().filter(|k| !k.starts_with('_')).collect();
    keys.sort();
    let mut segs: Vec<String> = Vec::new();
    // per key: list of pieces; Lit(text) or Tr(index, lead, trail)
    enum P {
        Lit(String),
        Tr(usize, String, String),
    }
    let mut plan: Vec<(String, Vec<P>)> = Vec::new();
    for k in &keys {
        let mut entry = Vec::new();
        for (is_ph, part) in split_placeholders(&en[*k]) {
            if is_ph || part.trim().is_empty() {
                entry.push(P::Lit(part));
            } else {
                let lead: String = part.chars().take_while(|c| c.is_whitespace()).collect();
                let trail: String = {
                    let t: String = part.chars().rev().take_while(|c| c.is_whitespace()).collect();
                    t.chars().rev().collect()
                };
                entry.push(P::Tr(segs.len(), lead, trail));
                segs.push(part.trim().to_string());
            }
        }
        plan.push(((*k).clone(), entry));
    }
    let tr = translate_batch(&segs, "en", code, 20)?;
    let mut out = serde_json::Map::new();
    let name = crate::languages::LANGS.iter().find(|l| l.0 == code).map(|l| l.3).unwrap_or(code);
    out.insert("_name".into(), Value::String(name.to_string()));
    for (k, entry) in plan {
        let mut s = String::new();
        for p in entry {
            match p {
                P::Lit(t) => s.push_str(&t),
                P::Tr(i, lead, trail) => {
                    s.push_str(&lead);
                    s.push_str(&tr[i]);
                    s.push_str(&trail);
                }
            }
        }
        out.insert(k, Value::String(s));
    }
    let text = serde_json::to_string_pretty(&Value::Object(out)).map_err(|e| e.to_string())?;
    std::fs::write(user_dir().join(format!("{code}.json")), text).map_err(|e| e.to_string())
}

pub struct I18n {
    pub lang: String,
    en: Dict,
    cur: Dict,
}

impl I18n {
    pub fn new(lang: &str) -> I18n {
        let mut i = I18n { lang: String::new(), en: read("en"), cur: Dict::new() };
        i.load(lang);
        i
    }

    pub fn load(&mut self, lang: &str) {
        self.lang = lang.to_string();
        self.cur = if lang == "en" { Dict::new() } else { read(lang) };
    }

    pub fn t(&self, key: &str) -> String {
        self.cur
            .get(key)
            .filter(|s| !s.is_empty())
            .or_else(|| self.en.get(key))
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }

    /// Like t() but replaces {name} placeholders.
    pub fn tf(&self, key: &str, args: &[(&str, String)]) -> String {
        let mut s = self.t(key);
        for (name, val) in args {
            s = s.replace(&format!("{{{name}}}"), val);
        }
        s
    }
}
