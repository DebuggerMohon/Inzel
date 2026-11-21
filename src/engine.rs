//! Record -> transcribe -> (translate) -> insert. UI independent; talks to the UI through events.
use crate::audio::{self, RecOptions, Recorder, FRAME};
use crate::config::Cfg;
use crate::events::{emit, Entry, Event, State};
use crate::languages::no_space;
use crate::{inject, stt, translate, util};
use std::sync::{Arc, Mutex};

struct Inner {
    state: State,
    rec: Option<Recorder>,
    /// stop() arrived before the recorder finished starting: Some(cancel)
    pending_stop: Option<bool>,
}

pub struct Engine {
    cfg: Cfg,
    inner: Mutex<Inner>,
}

enum Fail {
    Network,
    Other(String),
}

impl From<stt::Error> for Fail {
    fn from(e: stt::Error) -> Fail {
        match e {
            stt::Error::Network(_) => Fail::Network,
            stt::Error::Other(m) => Fail::Other(m),
        }
    }
}

impl From<String> for Fail {
    fn from(m: String) -> Fail {
        Fail::Other(m)
    }
}

fn join(pieces: &[(String, String)]) -> String {
    let mut out = String::new();
    for (text, lang) in pieces {
        if !out.is_empty() && !no_space(lang) {
            out.push(' ');
        }
        out.push_str(text);
    }
    out.trim().to_string()
}

impl Engine {
    pub fn new(cfg: Cfg) -> Arc<Engine> {
        Arc::new(Engine { cfg, inner: Mutex::new(Inner { state: State::Idle, rec: None, pending_stop: None }) })
    }

    pub fn state(&self) -> State {
        self.inner.lock().unwrap().state
    }

    fn set(&self, s: State) {
        self.inner.lock().unwrap().state = s;
        emit(Event::State(s));
    }

    // ---- control ---------------------------------------------------------------
    pub fn start(self: &Arc<Self>) {
        {
            let mut g = self.inner.lock().unwrap();
            if g.state != State::Idle {
                return;
            }
            g.state = State::Recording;
            g.pending_stop = None;
        }
        let toggle = self.cfg.str("record_mode") == "toggle";
        let opts = RecOptions {
            device: self.cfg.opt_str("mic"),
            auto_stop: toggle && self.cfg.bool("auto_stop"),
            silence_s: self.cfg.f64("auto_stop_seconds") as f32,
        };
        let me = self.clone();
        let on_auto = Arc::new(move |reason: &'static str| {
            let me = me.clone();
            // must not stop the stream from inside its own callback
            std::thread::spawn(move || me.stop(reason == "no_speech"));
            if reason == "no_speech" {
                emit(Event::Notice("no_speech".into()));
            }
        });
        match Recorder::start(opts, on_auto) {
            Ok(rec) => {
                let pending;
                {
                    let mut g = self.inner.lock().unwrap();
                    g.rec = Some(rec);
                    pending = g.pending_stop.take();
                }
                emit(Event::State(State::Recording));
                if let Some(cancel) = pending {
                    self.stop(cancel);
                }
            }
            Err(e) => {
                eprintln!("microphone error: {e}");
                self.set(State::Idle);
                emit(Event::Error("err_mic".into()));
            }
        }
    }

    pub fn stop(self: &Arc<Self>, cancel: bool) {
        let rec = {
            let mut g = self.inner.lock().unwrap();
            if g.state != State::Recording {
                return;
            }
            match g.rec.take() {
                Some(r) => {
                    g.state = State::Processing;
                    r
                }
                None => {
                    g.pending_stop = Some(cancel);
                    return;
                }
            }
        };
        let pcm = rec.stop();
        if cancel || pcm.len() < FRAME * 8 {
            self.set(State::Idle);
            return;
        }
        emit(Event::State(State::Processing));
        let me = self.clone();
        std::thread::spawn(move || me.process(pcm));
    }

    // ---- pipeline ----------------------------------------------------------------
    fn process(&self, pcm: Vec<i16>) {
        match self.run(&pcm) {
            Ok(()) => {}
            Err(Fail::Network) => emit(Event::Error("err_network".into())),
            Err(Fail::Other(m)) => emit(Event::Error(m)),
        }
        self.set(State::Idle);
    }

    fn run(&self, pcm: &[i16]) -> Result<(), Fail> {
        let src = self.cfg.str("speak_lang");
        let dst = self.cfg.str("target_lang");
        let do_translate = self.cfg.bool("translate_enabled");
        let cands = vec![self.cfg.str("auto_a"), self.cfg.str("auto_b")];

        let mut pieces: Vec<(String, String)> = Vec::new(); // (text, detected lang)
        for seg in audio::split_segments(pcm) {
            let (text, lang) = if src == "auto" {
                stt::transcribe_auto(&seg, &cands)?
            } else {
                (stt::transcribe(&seg, &src)?, src.clone())
            };
            if !text.is_empty() {
                pieces.push((text, lang));
            }
        }
        if pieces.is_empty() {
            emit(Event::Notice("no_speech".into()));
            return Ok(());
        }

        let original = join(&pieces);
        let out_pieces: Vec<(String, String)> = if do_translate {
            let mut v = Vec::new();
            for (t, l) in &pieces {
                let tt = if *l == dst { t.clone() } else { translate::translate(t, l, &dst)? };
                v.push((tt, dst.clone()));
            }
            v
        } else {
            pieces.clone()
        };
        let text = join(&out_pieces);
        let out_lang = out_pieces.last().map(|p| p.1.clone()).unwrap_or_default();
        let mut to_insert = text.clone();
        if self.cfg.bool("trailing_space") && !no_space(&out_lang) {
            to_insert.push(' ');
        }
        inject::insert_text(&to_insert, &self.cfg.str("output"));

        emit(Event::Result(Entry {
            time: util::now_hm(),
            src: pieces[0].1.clone(),
            dst: if do_translate { Some(dst) } else { None },
            original,
            text,
        }));
        Ok(())
    }
}
