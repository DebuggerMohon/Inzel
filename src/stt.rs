//! Speech -> text through Google's free Web Speech endpoint (same one the Python version used).
//! To use another engine (Whisper, Vosk, ...) replace `transcribe` and `transcribe_auto`.
use crate::languages::stt_code;
use crate::net;
use serde_json::Value;

/// Public Chromium key (the same default key the Python SpeechRecognition library uses).
const KEY: &str = "AIzaSyBOti4mM-6x9WDnZIjIeyEU21OpBXqWBgw";
const URL: &str = "https://www.google.com/speech-api/v2/recognize";

#[derive(Debug)]
pub enum Error {
    Network(String),
    Other(String),
}

// ---------------------------------------------------------------- FLAC (verbatim frames)
fn crc8(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for &b in data {
        crc ^= b;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 { (crc << 1) ^ 0x07 } else { crc << 1 };
        }
    }
    crc
}

fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0u16;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x8005 } else { crc << 1 };
        }
    }
    crc
}

fn utf8_number(mut n: u32, out: &mut Vec<u8>) {
    if n < 0x80 {
        out.push(n as u8);
    } else if n < 0x800 {
        out.push(0xC0 | (n >> 6) as u8);
        out.push(0x80 | (n & 0x3F) as u8);
    } else {
        let b2 = 0x80 | (n & 0x3F) as u8;
        n >>= 6;
        let b1 = 0x80 | (n & 0x3F) as u8;
        n >>= 6;
        out.push(0xE0 | (n & 0x0F) as u8);
        out.push(b1);
        out.push(b2);
    }
}

/// Minimal uncompressed FLAC writer: 16 kHz, mono, 16-bit, "verbatim" subframes.
fn flac_encode(pcm: &[i16]) -> Vec<u8> {
    const BLOCK: usize = 4096;
    let total = pcm.len() as u64;
    let mut out: Vec<u8> = Vec::with_capacity(pcm.len() * 2 + 128);
    out.extend_from_slice(b"fLaC");
    // STREAMINFO (last metadata block)
    out.push(0x80);
    out.extend_from_slice(&[0, 0, 34]);
    let bs: u16 = (if pcm.len() >= BLOCK { BLOCK } else { pcm.len().max(16) }) as u16;
    out.extend_from_slice(&bs.to_be_bytes()); // min block size
    out.extend_from_slice(&bs.to_be_bytes()); // max block size
    out.extend_from_slice(&[0, 0, 0]); // min frame size (unknown)
    out.extend_from_slice(&[0, 0, 0]); // max frame size (unknown)
    let packed: u64 = (16000u64 << 44) | (0u64 << 41) | (15u64 << 36) | (total & 0xF_FFFF_FFFF);
    out.extend_from_slice(&packed.to_be_bytes());
    out.extend_from_slice(&[0u8; 16]); // MD5 unknown

    for (idx, block) in pcm.chunks(BLOCK).enumerate() {
        let start = out.len();
        out.push(0xFF);
        out.push(0xF8); // sync + fixed block size
        let bs_code: u8 = if block.len() == BLOCK { 0xC } else { 0x7 };
        out.push((bs_code << 4) | 0x5); // block size code | 16 kHz
        out.push(0x08); // mono | 16 bits per sample | reserved
        utf8_number(idx as u32, &mut out);
        if bs_code == 0x7 {
            out.extend_from_slice(&((block.len() - 1) as u16).to_be_bytes());
        }
        let hcrc = crc8(&out[start..]);
        out.push(hcrc);
        out.push(0x02); // subframe header: verbatim
        for &s in block {
            out.extend_from_slice(&s.to_be_bytes());
        }
        let fcrc = crc16(&out[start..]);
        out.extend_from_slice(&fcrc.to_be_bytes());
    }
    out
}

// ---------------------------------------------------------------- recognition
fn post(lang: &str, content_type: &str, body: &[u8]) -> Result<String, ureq::Error> {
    let resp = net::agent()
        .post(URL)
        .query("client", "chromium")
        .query("lang", lang)
        .query("key", KEY)
        .query("pFilter", "0")
        .set("Content-Type", content_type)
        .send_bytes(body)?;
    resp.into_string().map_err(ureq::Error::from)
}

/// Returns (text, confidence).
fn recognize(pcm: &[i16], lang: &str) -> Result<(String, f32), Error> {
    let flac = flac_encode(pcm);
    let body = match post(lang, "audio/x-flac; rate=16000", &flac) {
        Ok(b) => b,
        Err(ureq::Error::Status(_, _)) => {
            // fallback: raw 16-bit PCM
            let mut raw = Vec::with_capacity(pcm.len() * 2);
            for s in pcm {
                raw.extend_from_slice(&s.to_le_bytes());
            }
            post(lang, "audio/l16; rate=16000", &raw).map_err(|e| Error::Network(e.to_string()))?
        }
        Err(e) => return Err(Error::Network(e.to_string())),
    };
    for line in body.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(first) = v.get("result").and_then(|r| r.as_array()).and_then(|r| r.first()) {
            if let Some(top) = first.get("alternative").and_then(|a| a.get(0)) {
                let text = top.get("transcript").and_then(|t| t.as_str()).unwrap_or("").trim().to_string();
                let conf = top.get("confidence").and_then(|c| c.as_f64()).unwrap_or(0.0) as f32;
                return Ok((text, conf));
            }
            break;
        }
    }
    Ok((String::new(), 0.0))
}

pub fn transcribe(pcm: &[i16], lang_code: &str) -> Result<String, Error> {
    Ok(recognize(pcm, stt_code(lang_code))?.0)
}

/// Try each candidate language in parallel, keep the most confident. Returns (text, lang).
pub fn transcribe_auto(pcm: &[i16], candidates: &[String]) -> Result<(String, String), Error> {
    let mut cands: Vec<String> = Vec::new();
    for c in candidates {
        if !cands.contains(c) {
            cands.push(c.clone());
        }
    }
    let results: Vec<(String, Result<(String, f32), Error>)> = std::thread::scope(|s| {
        let handles: Vec<_> = cands
            .iter()
            .map(|c| {
                let c = c.clone();
                s.spawn(move || {
                    let r = recognize(pcm, stt_code(&c));
                    (c, r)
                })
            })
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    });
    let mut best: (String, String, f32) = (String::new(), cands[0].clone(), -1.0);
    for (lang, r) in results {
        let (text, conf) = r?;
        if !text.is_empty() && conf > best.2 {
            best = (text, lang, conf);
        }
    }
    Ok((best.0, best.1))
}
