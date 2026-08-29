---
phase: 08-local-stt-whisper-cpp
status: human_needed
gate: blocking
hardware_required: "Apple M1 Air (NOT M3 Max — LOCAL-04 floor is M1 Air)"
deferred_from: "Plan 08-03 final checkpoint (`type=\"checkpoint:human-verify\" gate=\"blocking\"`)"
resume_signal: "approved | escalate to v2 — M1 Air bench failed at <Ns>"
---

# Phase 8 — Human Verification: Local STT Acceptance Bench

> **Status:** `human_needed`. This bench cannot be run in the autonomous
> session — no M1 Air hardware is attached. Code-level work for Plan 08-03
> is complete (see `08-03-SUMMARY.md`); the bench gates the phase as a
> whole.

## Why This Is the Kill Criterion

LOCAL-04 says **first-transcript-bar latency must be < 3 s on M1 Air**
with `small.en`. The Phase 8 CONTEXT (D-14) explicitly chose M1 Air as
the acceptance floor — not M3 Max — because the dual-state preview /
settled split was designed to hold on the slowest Apple-Silicon hardware
we ship to. If M1 Air can't hit < 3 s, Phase 8 is not done. Two paths
forward in that case:
1. Tune the dual `whisper_state` split (smaller preview window, faster
   sampler, shorter VAD silence-hang).
2. Escalate to WhisperKit / ANE acceleration in v2 (CONTEXT deferred
   ideas — adds a Swift sidecar + IPC layer).

The user reports the actual measured latency; the orchestrator picks the
path.

## Pre-Bench Sanity (Already Done by Autonomous Run)

- `cargo fmt --all` — clean.
- `cargo clippy --workspace --all-targets --features yogurt-stt/local-stt
  -- -D warnings` — clean.
- `cargo test --workspace --features yogurt-stt/local-stt` — 196 passed,
  3 ignored.
- `cargo build --no-default-features -p yogurt-stt` — 3.2 s, well under
  the 30 s ROADMAP gate.
- `pnpm --dir web test` — 121 passed (incl. 2 new ModelPicker tests).
- `pnpm --dir web build` — clean.
- Release binary built and smoke-tested locally:
  `GET /` → 200, `GET /api/stt/models` → all 4 REGISTRY entries serialize
  with the correct `{name, size_mb, downloaded, intel_supported}` shape.
- Dev server killed; port 7878 free before this hand-off.

## Step-by-Step Bench Protocol (Run on M1 Air)

> **CRITICAL — TIME STEP 6.** That single number (`first-transcript-bar
> latency in seconds`) is the entire kill criterion.

### 1. Build the release binary on an M1 Air

```bash
cd /path/to/yogurt
cargo build -p yogurt --release
```

Prerequisites (one-time): `xcode-select --install` and `brew install cmake`
(see `08-01-SUMMARY.md` for context).

### 2. Launch the server

```bash
./target/release/yogurt start --no-open
```

Confirm `curl -sf http://localhost:7878/api/health` returns
`{"status":"ok","service":"yogurt-server"}`.

### 3. Open the Settings page

Browse to `http://localhost:7878/settings`. Click the **Transcription**
sidebar item. Confirm:

- **No "Coming soon" badge** anywhere on the page.
- The Cloud / Local card pair matches the PRD §5.6 visual spec — Cloud
  on the left (blueberry-bordered when active), Local on the right
  (matcha-bordered when active).
- 4 model pills are visible inside the Local card: `tiny.en`, `small.en`,
  `medium.en`, `large-v3`. Pills not yet downloaded show the `↓` glyph;
  any already-downloaded ones show the `✓` glyph.

### 4. Activate Local STT

Click the **"Use Local"** radio button on the Local card. Confirm:
- The matcha 1.5px border appears around the Local card.
- The Cloud card loses its active border.
- The selection persists across page reload (`PATCH /api/settings` with
  `{stt_provider: "local"}` should have fired — open the network panel
  to verify).

### 5. Download `small.en`

Click the `small.en ↓` pill. Confirm:
- The STATE-04 download dialog appears (PRD §5.11 — matcha modal,
  420 px, native `<dialog>` so Escape closes it).
- The progress bar fills with matcha green from left to right.
- The mono caption ticks visibly: `bytes_downloaded / total_bytes ·
  bytes_per_sec/s · etas left` should update roughly every 500 ms.
- Dialog auto-closes ~600 ms after reaching 100% (the `_complete` event).
- After close, the pill renders `small.en ✓`.

### 6. SHA256 verification (model integrity)

```bash
shasum -a 256 ~/.yogurt/models/ggml-small.en.bin
```

Confirm the hash matches the `small.en` entry in
`crates/yogurt-stt/src/models.rs::REGISTRY`. (If the SHA mismatched,
`models::download` would have hard-failed and the dialog would have
shown an error — passing this step is implicit in step 5 succeeding,
but worth confirming visually.)

### 7. **TIMED BENCH — the kill criterion**

Click **"+ New meeting"** in the sidebar. Once the meeting page loads,
start talking. **Use a stopwatch.**

Start the stopwatch the moment you start a continuous sentence.
**Stop it the moment the first transcript bar appears in the dock.**

- **Pass:** `< 3.0 s` first-bar latency.
- **Fail:** `≥ 3.0 s` — record the exact measurement.

Talk for 30 seconds. Confirm:
- Transcript text appears in the dock as you talk (preview / partial
  bars at opacity ~0.7, finalizing into solid bars).
- The text is recognizable English (whisper.cpp `small.en` is not
  Deepgram-quality but should be readable).
- No latency drift over 30 s — the 30th-second bar should appear with
  the same ~3 s lag as the 1st-second bar.

End the meeting. Confirm the transcript persists (Library page lists it).

### 8. **OFFLINE TEST — the privacy claim**

1. Disable wifi (Mac menu bar → Wi-Fi → Turn Off).
2. Close the browser tab.
3. Kill and restart the server: `pkill yogurt; ./target/release/yogurt
   start --no-open`.
4. Reopen `http://localhost:7878/`.
5. Click **"+ New meeting"**, talk for 30 seconds.

- **Pass:** transcription still works with no network access — bars
  appear, text is coherent.
- **Fail:** transcription fails or hangs — file an issue; the WhisperLocal
  adapter or `models::is_downloaded` short-circuit is broken.

### 9. (Optional — if you have an Intel Mac nearby)

On an Intel Mac:
- The Local card pill row should render a yellow `slow` chip next to
  `medium.en` and `large-v3` (per PRD §5.8 — those models are not
  marked `intel_supported` in REGISTRY).
- `small.en` should still work, slower-than-realtime but acceptable.

## What to Report Back

In the orchestrator resume signal, send back at minimum:

- M1 Air `small.en` first-transcript-bar latency: `_._ s`
- Offline test (step 8): `pass | fail`
- SHA256 verification (step 6): `match | mismatch`
- (Optional) Intel chip visible at step 9: `yes | no | n/a`

Resume signals:
- `"approved"` — if all gates pass.
- `"escalate to v2 — M1 Air bench failed at <Ns>"` — if step 7 measures
  ≥ 3.0 s; this captures the bench data and stops Phase 8 pending
  WhisperKit/ANE work.

## What Was Already Verified Autonomously (For Audit)

- The Plan 08-03 code-level work is committed:
  - `252b9b1` — Task 1 (server WS events + REST)
  - `9b5307e` — Task 2 (`select_stt` branch + V005 KV seed)
  - `a0dc737` — Task 3 (frontend API client + WS hook)
  - `8e2a1a5` — Task 4 (LocalSTTCard + ModelPicker + Dialog + Settings rewire)
  - `5be3c71` — Task 5 (rustfmt sweep + bench notes)
- The release binary boots, serves `GET /api/stt/models` correctly, and
  the SPA loads at `/` — wire-level smoke is green.
- The dev-server process was killed at end-of-run; port 7878 was free
  at hand-off time.

The ONLY remaining surface that needs M1 Air physical hardware is the
< 3 s latency measurement and the offline test.
