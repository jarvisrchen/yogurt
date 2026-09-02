---
name: yogurt-control
description: Read or control a meeting in a running yogurt instance. Use when handed a yogurt meeting URL (http://localhost:7878/meeting/<id>/post) or asked "what was discussed in this meeting", "summarize this yogurt meeting", "what did they say about X" - and for "start a meeting in yogurt", "start recording", "end/stop the meeting", "is yogurt recording".
---

# Read and control yogurt

Run `yogurt ctl` first, with no arguments: it prints status - instances found, the active meeting, and the active provider.
Needs yogurt 0.8.0 or newer: check with `yogurt --version`, and run `brew upgrade jarvisrchen/yogurt/yogurt` if it's older.

## The binary

<!-- yogurt-cli:start -->
- `yogurt start` - Launch the local server and open the browser
- `yogurt doctor` - Print diagnostic info (rust, macOS, perms, providers, models) + repair actions
- `yogurt ctl` - Control a running yogurt instance: status, meetings, detection, windows
  - `yogurt ctl status` - Instances found, active/detected meeting, stt engine, provider, permission grants
  - `yogurt ctl meeting` - Create, start, stop, and read meetings on a running instance
  - `yogurt ctl detect` - What meeting detection currently sees (MTG-11), or dismiss the prompt
  - `yogurt ctl windows` - On-screen windows and each one's meeting-detection verdict. No server needed
  - `yogurt ctl settings` - General settings the server exposes: get and set
  - `yogurt ctl provider` - Configured LLM providers: list, activate, test
  - `yogurt ctl models` - STT models: list, download, delete
  - `yogurt ctl ws` - Subscribe to the server websocket, printing one JSON frame per line
<!-- yogurt-cli:end -->

[docs/FEATURES.md](../../../docs/FEATURES.md) maps every feature to its UI path, API route, and `ctl` command.
Reads fall back to the local database (`source: db`) when no server answers, so they still work with yogurt not running.

Three rules:

- **Summary before transcript.** `ctl meeting summary` is small; `ctl meeting transcript` is large - reach for it only when the summary doesn't answer the question.
- **One recording at a time.** Run `ctl status` before `ctl meeting start`, since a second recording overlaps audio captures.
- **Never cat or print the session token.** `ctl` handles auth itself; `~/.yogurt/session-token` is not for the agent to read.

See [docs/AI-INTEGRATION.md](../../../docs/AI-INTEGRATION.md) for anything `ctl` doesn't cover yet.
