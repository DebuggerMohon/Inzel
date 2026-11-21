<div align="center">

# Inzel

**Free, open-source push-to-talk voice typing for Windows — now written in Rust.**
Hold a key, speak in any of 39 languages, and the text appears at your cursor, in any app.
Optional **Translate mode**: speak Bangla, get English (or any other pair).

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Platform: Windows 10/11](https://img.shields.io/badge/platform-Windows%2010%2F11-0078D6.svg)](#download--install)
[![Latest release](https://img.shields.io/github/v/release/DebuggerMohon/Inzel)](https://github.com/DebuggerMohon/Inzel/releases/latest)

</div>

## Why Rust?

The original Inzel (Python + CustomTkinter) used about 50 MB of RAM. This version has no interpreter and no GUI
framework: the interface is drawn directly with Win32/GDI, so the whole app is a single small `.exe` and uses a
fraction of the memory (the working set is also trimmed whenever the window is hidden in the tray).

## Features

- Types anywhere — browser, Word, chat apps, IDEs.
- 39 languages, Translate mode, auto-detect between two languages.
- Hold-to-talk or click-to-start / click-to-stop with silence auto-stop.
- Small always-on-top recording bar with a live level meter (never steals focus).
- Transcript history with copy buttons; tray icon; start with Windows.
- UI in English and বাংলা (other languages are translated automatically on first use).
- No account, no API key, no telemetry.

## Download & install

1. Open the [**Releases**](https://github.com/DebuggerMohon/Inzel/releases/latest) page.
2. Download `Inzel-Setup-x.y.z.exe` and run it: **Next → Next → Install → Finish**.

> **SmartScreen** may say "Windows protected your PC" because the installer is not code-signed yet.
> Click **More info → Run anyway**, or build it yourself (below).

## How to use

| Action | Default key |
|---|---|
| Dictate (hold, speak, release) | `F9` |
| Toggle Translate mode | `F10` |

Closing the window hides Inzel to the tray; tray icon → **Quit** exits. Settings are stored in
`%APPDATA%\Inzel\config.json` (same format as the Python version).

## Privacy

No server, no telemetry. While you dictate, audio is sent to Google's speech recognition endpoint; with Translate
mode on, text is sent to Google Translate. The engine is one small file (`src/stt.rs`) that can be replaced by an
offline engine.

## Build from source

You need Windows 10/11, [Rust](https://rustup.rs) (stable, MSVC toolchain) and the
**"Desktop development with C++"** workload of [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/).

```bat
git clone https://github.com/DebuggerMohon/Inzel.git
cd Inzel
cargo run --release          :: run it
.\build.bat                  :: exe + installer (needs Inno Setup 6/7)
```

Output: `target\release\Inzel.exe` and `installer\Inzel-Setup-x.y.z.exe`.
Pushing a tag like `v1.0.0` builds the installer on GitHub Actions and attaches it to a Release.

## Project structure

```
src/main.rs        entry point
src/ui.rs          main window, pages, popups (GDI, immediate-mode)
src/gfx.rs         tiny GDI toolkit (rects, text, fonts, off-screen canvas)
src/overlay.rs     recording bar
src/engine.rs      record -> transcribe -> (translate) -> insert
src/audio.rs       microphone (cpal/WASAPI), level meter, auto-stop, segment splitting
src/stt.rs         speech recognition (Google) + minimal FLAC encoder
src/translate.rs   Google Translate
src/hotkeys.rs     global low-level keyboard hook
src/inject.rs      paste / type into the focused window
src/tray.rs, system.rs, config.rs, i18n.rs, languages.rs, theme.rs
locales/           en.json, bn.json (embedded in the exe)
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Ideas: offline Whisper/Vosk engine, spoken punctuation, per-app profiles.

## License

[MIT](LICENSE) © 2026 Debugger Mohon
