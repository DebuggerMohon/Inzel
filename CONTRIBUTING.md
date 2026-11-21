# Contributing to Inzel

Thanks for helping! Inzel is a small Rust project, so it is easy to get started.

## Ways to help

- **Report a bug** or **suggest a feature** via [Issues](https://github.com/DebuggerMohon/Inzel/issues).
- **Add or improve a UI translation** (no coding needed – see below).
- **Fix a bug or build a feature** via a Pull Request.
- **Improve docs** (README, this file, `docs/SPEC.md`).

## Development setup

Windows 10/11, [Rust](https://rustup.rs) (stable, MSVC) and Visual Studio Build Tools ("Desktop development with C++").

```bat
git clone https://github.com/DebuggerMohon/Inzel.git
cd Inzel
cargo run            :: debug build, shows a console for logs
cargo build --release
```

## Workflow

1. **Fork** the repo and create a branch from `main`:
   `git checkout -b fix/short-description` (or `feat/...`, `docs/...`).
2. Make your change (run `cargo fmt` and `cargo clippy`). Keep it focused – one topic per Pull Request.
3. **Test manually** (see checklist below).
4. Commit with a clear message, e.g. `Fix crash when microphone is unplugged`.
5. Push and open a **Pull Request** to `main`. Describe what changed and why; add screenshots for UI changes.

## Manual test checklist

There is no automated test suite yet. Before opening a PR, check what applies:

- [ ] App starts with `cargo run`, no errors in the console
- [ ] Hold F9 → speak → text appears in Notepad
- [ ] Translate mode (F10) works, e.g. Bangla → English
- [ ] Click mode with auto-stop works
- [ ] Settings save and are kept after restart
- [ ] Closing the window hides to the tray; tray → Quit exits
- [ ] If you touched UI strings: both English and Bangla still display correctly

## Code style

- Follow the surrounding code; keep functions small and readable.
- The UI thread must never block: audio, recognition and translation run on worker threads and report back
  through `events::emit(...)`.
- New setting: add the default in `src/config.rs`, a control in `src/ui.rs`, and labels in `locales/en.json` **and** `locales/bn.json`.
- Keep dependencies minimal — low RAM and a small exe are core goals.

## Add a UI language

1. Copy `locales/en.json` to `locales/<code>.json` (e.g. `fr.json`) and register it in `src/i18n.rs`.
2. Change `"_name"` to the language's own name and translate the values. **Do not change the keys.**
   Keep placeholders like `{hotkey}` and `{s}` unchanged.
3. Open a Pull Request.

(Languages without a file are translated automatically the first time a user picks them.)

## Add a speech engine (offline Whisper, Vosk, …)

All speech recognition lives in `src/stt.rs`. The rest of the app only calls:

- `transcribe(pcm: &[i16], lang_code) -> Result<String, Error>`
- `transcribe_auto(pcm, candidates) -> Result<(String, String), Error>`

`pcm` is mono i16 at 16 kHz. Implement the same functions for another engine; ideally make it
selectable in Settings and keep the Google engine as the default.

## Releases (maintainers)

1. Update the version in `Cargo.toml` **and** `installer.iss` (`MyAppVersion`), and add notes to `CHANGELOG.md`.
2. Commit, then `git tag vX.Y.Z && git push --tags`.
3. GitHub Actions builds the installer and publishes it on the Releases page.

## Code of conduct

Be kind and respectful. Assume good intent, give constructive feedback, and welcome newcomers.
