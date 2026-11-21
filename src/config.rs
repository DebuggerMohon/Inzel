//! Settings stored in %APPDATA%\Inzel\config.json (same keys as the Python version).
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub fn config_dir() -> PathBuf {
    let base = std::env::var("APPDATA").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."));
    let d = base.join("Inzel");
    let _ = std::fs::create_dir_all(&d);
    d
}

fn defaults() -> Map<String, Value> {
    let v = json!({
        "ui_language": "en",
        "speak_lang": "bn",
        "target_lang": "en",
        "auto_a": "bn",
        "auto_b": "en",
        "translate_enabled": false,
        "hotkey_dictate": "f9",
        "hotkey_translate": "f10",
        "record_mode": "hold",
        "auto_stop": true,
        "auto_stop_seconds": 2.5,
        "mic": null,
        "output": "paste",
        "trailing_space": true,
        "start_with_windows": false,
        "start_minimized": false,
        "close_to_tray": true
    });
    match v {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

struct Inner {
    path: PathBuf,
    data: Map<String, Value>,
    defaults: Map<String, Value>,
}

/// Cheap to clone, safe to share between threads.
#[derive(Clone)]
pub struct Cfg(Arc<Mutex<Inner>>);

impl Cfg {
    pub fn load() -> Cfg {
        let path = config_dir().join("config.json");
        let defaults = defaults();
        let mut data = defaults.clone();
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(Value::Object(m)) = serde_json::from_str::<Value>(&text) {
                for (k, v) in m {
                    data.insert(k, v);
                }
            }
        }
        Cfg(Arc::new(Mutex::new(Inner { path, data, defaults })))
    }

    fn value(&self, key: &str) -> Value {
        let g = self.0.lock().unwrap();
        g.data.get(key).or_else(|| g.defaults.get(key)).cloned().unwrap_or(Value::Null)
    }

    pub fn str(&self, key: &str) -> String {
        match self.value(key) {
            Value::String(s) => s,
            _ => String::new(),
        }
    }

    pub fn opt_str(&self, key: &str) -> Option<String> {
        match self.value(key) {
            Value::String(s) if !s.is_empty() => Some(s),
            _ => None,
        }
    }

    pub fn bool(&self, key: &str) -> bool {
        self.value(key).as_bool().unwrap_or(false)
    }

    pub fn f64(&self, key: &str) -> f64 {
        self.value(key).as_f64().unwrap_or(0.0)
    }

    pub fn set(&self, key: &str, value: Value) {
        let mut g = self.0.lock().unwrap();
        g.data.insert(key.to_string(), value);
        let text = serde_json::to_string_pretty(&Value::Object(g.data.clone())).unwrap_or_default();
        let tmp = g.path.with_extension("tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &g.path);
        }
    }

    pub fn set_str(&self, key: &str, v: &str) {
        self.set(key, Value::String(v.to_string()));
    }

    pub fn set_bool(&self, key: &str, v: bool) {
        self.set(key, Value::Bool(v));
    }
}
