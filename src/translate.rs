//! Text translation through Google Translate's free endpoint.
use crate::net;
use serde_json::Value;
use std::time::Duration;

const URL: &str = "https://translate.googleapis.com/translate_a/single";

fn chunks(text: &str, n: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut start = 0;
    while chars.len() - start > n {
        let window = &chars[start..start + n];
        let cut = window.iter().rposition(|c| *c == ' ').filter(|p| *p > 0).unwrap_or(n);
        out.push(chars[start..start + cut].iter().collect::<String>());
        start += cut;
        while start < chars.len() && chars[start].is_whitespace() {
            start += 1;
        }
    }
    out.push(chars[start..].iter().collect());
    out
}

fn gtx(text: &str, src: &str, dst: &str) -> Result<String, String> {
    let mut out: Vec<String> = Vec::new();
    for part in chunks(text, 1500) {
        let resp = net::agent()
            .get(URL)
            .query("client", "gtx")
            .query("sl", src)
            .query("tl", dst)
            .query("dt", "t")
            .query("q", &part)
            .call()
            .map_err(|e| e.to_string())?;
        let body = resp.into_string().map_err(|e| e.to_string())?;
        let v: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        let mut piece = String::new();
        if let Some(segs) = v.get(0).and_then(|x| x.as_array()) {
            for seg in segs {
                if let Some(s) = seg.get(0).and_then(|x| x.as_str()) {
                    piece.push_str(s);
                }
            }
        }
        out.push(piece);
    }
    Ok(out.join(" "))
}

pub fn translate(text: &str, src: &str, dst: &str) -> Result<String, String> {
    if text.is_empty() || src == dst {
        return Ok(text.to_string());
    }
    let mut last = String::new();
    for _ in 0..2 {
        match gtx(text, src, dst) {
            Ok(s) => return Ok(s),
            Err(e) => {
                last = e;
                std::thread::sleep(Duration::from_millis(300));
            }
        }
    }
    Err(format!("Translate failed: {last}"))
}

/// Translate many short strings (one request per `size` items).
pub fn translate_batch(items: &[String], src: &str, dst: &str, size: usize) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for chunk in items.chunks(size) {
        let joined = chunk.join("\n");
        let res: Vec<String> = translate(&joined, src, dst)?.split('\n').map(|s| s.to_string()).collect();
        if res.len() == chunk.len() {
            out.extend(res.into_iter().map(|s| s.trim().to_string()));
        } else {
            for x in chunk {
                out.push(translate(x, src, dst)?.trim().to_string());
            }
        }
    }
    Ok(out)
}
