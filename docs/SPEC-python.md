# Inzel — Build Spec / Master Prompt

Use this document as the system prompt when asking an AI (or a contributor) to extend the app.

## 1. What it is
A free, open-source **push-to-talk voice typing** app for **Windows**. Press a global hotkey, speak in any
language, and the recognized text is typed **at the current cursor position of any app** (browser, Word, chat, IDE…).
Optional **Translate mode**: speak in language A, the text appears in language B.

## 2. Tech decisions
| Area | Choice | Why |
|---|---|---|
| Language | Python 3.10+ | fastest to build, rich audio/GUI libs, easy for contributors |
| Speech → text | `SpeechRecognition` → `recognize_google` (Google Web Speech, free, no API key) | no Gemini/AI-key rate-limit headaches |
| Translation | `deep-translator` → `GoogleTranslator` (Google Translate, free) | text translation only |
| Audio capture | `sounddevice` (PortAudio), 16 kHz mono | mic selection, live level meter |
| Global hotkeys | `keyboard` | press/release detection (hold-to-talk) |
| Text insertion | clipboard paste (`pyperclip` + Ctrl+V), fallback: simulated typing | paste is the only reliable way for Bangla/Unicode |
| GUI | `customtkinter` (+ small `tkinter` overlay) | modern dark UI, single dependency set |
| Tray | `pystray` + Pillow | runs in background, "hidden icons" area |
| Packaging | PyInstaller (`--onedir --windowed`) + Inno Setup installer | Next-Next-Finish `Setup.exe` |

Note: Google Translate itself does not do speech recognition. Speech → text uses Google's Web Speech endpoint
(same engine as Chrome dictation); translation uses Google Translate. Both are unofficial/free endpoints, so
rate limits or breakage are possible. `inzel/stt.py` is a single small module so other backends
(Whisper, Vosk, Azure…) can be plugged in.

## 3. Functional requirements
### Hotkeys & recording
- User-defined global hotkey (default `F9`) to dictate. Changeable in Settings by pressing the new keys.
- Recording mode (setting): **Hold to talk** (record while held) or **Click to start / click to stop**.
- **Auto-stop**: in click mode, stop automatically after N seconds of silence once speech was detected
  (N adjustable 1–6 s, default 2.5 s). If nothing is said for 10 s, cancel.
- Separate hotkey (default `F10`) toggles **Translate mode** on/off.
- Microphone selector in Settings (default = system default).

### Output
- Recognized text is inserted at the cursor of the focused app (paste or type — selectable).
- Optional trailing space after each insertion.

### Languages
- ~39 spoken languages (Bangla, English, Hindi, Urdu, Arabic, …; see `inzel/languages.py`).
- Home page: **"I speak"** language + **"Translate to"** language, plus mode switch **Dictate | Translate**.
  - Dictate: text appears in the language spoken.
  - Translate: e.g. speak Bangla → English text is inserted.
- **App UI language** (Settings): English default; Bangla included; add a JSON file in `locales/` for more.

### Screens
1. **Home**: header (logo + name on the left, ⚙ Settings on the right) · language/mode card · status card
   (hotkey hint, state) · **Transcript history** (kept until the app is closed; newest first, copy button, clear) · footer.
2. **Settings**: Back button on top · Hotkeys · Recording (mode, auto-stop, microphone) · App language ·
   Output · System (start with Windows, start minimized, close to tray) · footer.
3. **Recording bar** (small always-on-top pill at bottom-centre, never steals focus): red blinking LED while
   recording, live audio-level dots, translate indicator (e.g. `BN → EN`), ✕ cancel.
4. **Tray icon**: Open · Translate mode (checkbox) · Quit. Closing the window hides to tray.
5. **Footer** (both screens): `Inzel v1.0.0 · by <author> · GitHub ↗`.

### Look & feel
Black and white base; **one accent colour (orange `#FF6A00`)** marks the active/selected button; red = recording.

## 4. Non-functional
- Windows 10/11, single instance, settings in `%APPDATA%\Inzel\config.json`.
- UI never blocks: audio, recognition and translation run in worker threads; UI polls an event queue.
- MIT licence, no telemetry. Audio is sent only to Google's endpoints while processing.

## 5. Architecture
```
main.py                 entry point
inzel/
  app.py                wires everything (UI, engine, hotkeys, tray, event queue)
  engine.py             record → transcribe → (translate) → insert; emits events
  recorder.py           mic stream, level meter, silence detection, segment splitting
  stt.py                Google speech recognition (swap backend here)
  translate.py          Google Translate
  hotkeys.py            global hook, hold/toggle events, hotkey capture
  injector.py           paste / type into focused window
  languages.py          39 languages (UI name, Google STT code, translate code)
  i18n.py, config.py, system.py
  ui/ pages.py overlay.py tray.py icon.py theme.py
locales/ en.json bn.json
```

## 6. Build steps (what to do, in order)
1. `python -m venv .venv && .venv\Scripts\activate && pip install -r requirements.txt`
2. `python main.py` — test: focus Notepad, hold `F9`, speak, release.
3. Tune in Settings (mic, language, mode).
4. `.\build.bat` → `dist\Inzel\Inzel.exe` (generates the icon, runs PyInstaller) and, if Inno Setup is installed, `installer\Inzel-Setup-<version>.exe`.
5. Push to GitHub and tag a release (`git tag vX.Y.Z && git push --tags`); GitHub Actions builds and attaches the installer.

## 7. Known limitations / ideas
- Needs internet. Google endpoints are unofficial and may throttle.
- Typing into **elevated (admin) windows** requires running Inzel as administrator.
- Hotkey keys are not blocked from reaching the focused app — avoid keys that type characters (use F-keys or modifiers).
- Ideas: offline backend (Whisper/Vosk), per-app profiles, custom vocabulary, punctuation commands, sound cues.
