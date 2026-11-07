# Quietkey

Free, open-source voice dictation for macOS. Hold `fn`, talk, let go, and the text is typed wherever your cursor is: Mail, Slack, Notes, a browser, a terminal.

Speech recognition runs on your Mac with Whisper, so your audio never leaves the computer. No account, no subscription.

## Features

- Hold `fn` to dictate, or press `fn` + `Space` for hands-free and `fn` again to stop. `Esc` cancels.
- Local transcription with whisper.cpp, Metal on Apple Silicon. Pick the model in Settings: Base (60 MB download, about 180 MB of memory), Small (190 MB, about 350 MB) or Large v3 Turbo (574 MB, about 700 MB, the most accurate). Quietkey downloads it for you.
- Your first words are kept: the microphone stays open for 20 seconds after a dictation, so the next one starts the moment you press the key. The macOS mic indicator stays on during that time; nothing is recorded.
- Offline cleanup: removes "um" and "uh", repeated words, and self-corrections such as "at 5, actually 6". Say "new line" or "new paragraph" for line breaks.
- Dictionary for names and terms Whisper gets wrong, with "wrong spelling => right spelling" corrections. Select a misheard word in your history and choose Add to dictionary.
- Snippets: say a short cue like "my email" and get the full text.
- Optional AI cleanup, still free: a [Groq](https://console.groq.com) API key (free tier, no card), or a local model through [Ollama](https://ollama.com). Only the text is sent, never audio.
- `⌃⌘V` pastes your last dictation again, `⌃⌘C` copies it.
- History of your dictations, or turn history off completely.
- Your clipboard is put back after every paste.
- With two displays, the dictation bar opens on the one you are working on.

## Requirements

- macOS 12 or later, Apple Silicon recommended
- 60 to 574 MB of disk space for the speech model, depending on the one you pick

## Build from source

```bash
# Rust, Node 20+ and cmake
brew install cmake
npm install

# Speech model (574 MB)
mkdir -p ~/Library/Application\ Support/dev.abdallah.quietkey/models
curl -L -o ~/Library/Application\ Support/dev.abdallah.quietkey/models/ggml-large-v3-turbo-q5_0.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin
# sha256: 394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2

npm run tauri dev
```

On first launch, allow **Input Monitoring** (to notice the `fn` key), **Accessibility** (to type into other apps) and the **Microphone**.

In System Settings → Keyboard, set **Press fn key to** → **Do Nothing**. Otherwise macOS starts its own dictation or the emoji picker every time you press `fn`.

## How it works

```
fn down → record (cpal) → fn up → Whisper (local) → snippets + dictionary
       → offline cleanup → optional Groq / Ollama → paste with ⌘V → restore clipboard
```

- `src-tauri/src/hotkey.rs` listens for `fn` with a Core Graphics event tap.
- `src-tauri/src/pipeline.rs` is the state machine for one dictation.
- `src-tauri/src/cleanup.rs` holds the offline text rules (unit tested).
- `src-tauri/src/paste.rs` saves every pasteboard type, pastes, then restores it.
- Data is stored in SQLite in `~/Library/Application Support/dev.abdallah.quietkey/`. The Groq key is kept in the macOS Keychain.

## Tests

```bash
cd src-tauri && cargo test --lib
```

`scripts/e2e/run.sh` drives the running app end to end: it simulates the `fn` key, replays a recorded sentence instead of the microphone, and checks the text that lands in TextEdit. See the comments in the script for setup.

## Status

Early. Dictation, cleanup, history and setup work. Screens for dictionary, snippets, style and settings are next; their backend is done.

## License

MIT
