---
phase: quick-260709-wnn
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/yogurt-audio/src/mic.rs
  - crates/yogurt-audio/src/lib.rs
  - crates/yogurt-audio/examples/wav_eartest.rs
  - crates/yogurt-audio/examples/dual_smoke.rs
  - crates/yogurt-audio/examples/mic_smoke.rs
  - crates/yogurt-server/src/meetings.rs
  - crates/yogurt-server/src/routes.rs
  - crates/yogurt-server/src/audio.rs
  - crates/yogurt-server/tests/audio_device_switch.rs
  - web/src/lib/api/settings.ts
  - web/src/components/settings/AudioSection.tsx
  - web/src/components/MicDevicePicker.tsx
  - web/src/components/MicDevicePicker.test.tsx
  - web/src/routes/Meeting.tsx
autonomous: true
requirements: []

must_haves:
  truths:
    - "While a meeting is recording, the user can pick a different microphone from a dropdown in the meeting toolbar and the switch takes effect with no gap in the live transcript or system-audio capture"
    - "The picker reflects the actual active device after a successful switch, not just the OS default"
    - "A meeting started after the user picks a device on the Settings page opens that device by default, instead of always the OS default input device"
    - "Switching to a device that fails to open (e.g. unplugged) leaves the previous microphone capturing and surfaces an error, instead of killing the recording"
    - "Requesting a device switch on a meeting that is not currently recording returns 409; requesting one on an unknown meeting id returns 404"
  artifacts:
    - path: "crates/yogurt-audio/src/mic.rs"
      provides: "spawn_mic_capture(tx, requested_device) device-by-name lookup + unit test for an unknown device name"
      contains: "requested_device"
    - path: "crates/yogurt-audio/src/lib.rs"
      provides: "start_capture(mic_device) initial-device parameter + AudioStream::switch_mic_device hot-swap"
      contains: "switch_mic_device"
    - path: "crates/yogurt-server/src/meetings.rs"
      provides: "AudioCommand channel, run_capture_control_loop, Registry::start(mic_device), Registry::switch_mic_device, SwitchDeviceError"
      contains: "run_capture_control_loop"
    - path: "crates/yogurt-server/src/routes.rs"
      provides: "POST /api/meetings/{id}/audio-device endpoint + persisted-device wiring on meeting start"
      contains: "switch_meeting_audio_device"
    - path: "crates/yogurt-server/tests/audio_device_switch.rs"
      provides: "404/409/403 REST contract regression tests for the new endpoint"
    - path: "web/src/components/MicDevicePicker.tsx"
      provides: "in-meeting mic device dropdown, visible while recording, reflecting the currently-active device"
    - path: "web/src/routes/Meeting.tsx"
      provides: "mounts MicDevicePicker in the recording toolbar"
  key_links:
    - from: "crates/yogurt-server/src/meetings.rs (Registry::start)"
      to: "yogurt_audio::start_capture"
      via: "mic_device.as_deref() forwarded so the persisted setting takes effect"
      pattern: "start_capture\\(mic_device"
    - from: "crates/yogurt-server/src/meetings.rs (run_capture_control_loop)"
      to: "yogurt_audio AudioStream::switch_mic_device"
      via: "in-thread call inside the tokio::select! command branch"
      pattern: "switch_mic_device"
    - from: "crates/yogurt-server/src/routes.rs (switch_meeting_audio_device)"
      to: "crates/yogurt-server/src/meetings.rs (Registry::switch_mic_device)"
      via: "state.meetings.switch_mic_device(&id, body.device_id)"
      pattern: "meetings\\.switch_mic_device"
    - from: "web/src/components/MicDevicePicker.tsx"
      to: "POST /api/meetings/:id/audio-device"
      via: "audioApi.switchMeetingDevice mutation, response.device drives the controlled select value"
      pattern: "audio-device"
    - from: "crates/yogurt-server/src/routes.rs (start_meeting)"
      to: "yogurt_db::settings::General.audio_input_device"
      via: "load_general(&state.db) read before Registry::start"
      pattern: "audio_input_device"
---

<objective>
Add true hot-swap mic/audio-device switching during an active meeting: a toolbar control that changes the captured microphone mid-recording with zero gap in the transcript, plus a fix so the already-persisted `audio_input_device` Settings value actually takes effect when a recording starts (today it is dead UI — `spawn_mic_capture` always opens `cpal::default_host().default_input_device()`).

Purpose: users on machines with multiple inputs (AirPods vs. built-in mic vs. USB interface) need to switch mid-call — e.g. unplugging headphones — without losing transcript continuity or restarting the STT session.

Output:
- `yogurt-audio`: `spawn_mic_capture` accepts an optional device name; `AudioStream::switch_mic_device` hot-swaps the mic producer in place, leaving `mic_tx`, system audio, and all existing broadcast subscribers untouched.
- `yogurt-server`: the meeting's capture `std::thread` gains an `mpsc` command channel serviced alongside the existing shutdown `oneshot` (via a `tokio::select!` loop extracted into a unit-testable `run_capture_control_loop`, mirroring the existing `pump_audio_adapter` pattern); a new `POST /api/meetings/{id}/audio-device` endpoint forwards switch requests into it; `Registry::start` now honors the persisted `audio_input_device` setting.
- Frontend: a `MicDevicePicker` dropdown in the `Meeting.tsx` toolbar, visible while recording, populated from the existing `GET /api/audio/devices`, tracking and reflecting the actual active device after each switch.

This is a concurrency-sensitive change (a `!Send` `cpal::Stream` owned on a dedicated OS thread gains a second inbound channel). Task 2 includes a dedicated unit test that exercises the real `tokio::select!` control loop across a real thread boundary — proving no deadlock and in-order command handling — without depending on real audio hardware (mirrors this codebase's existing hardware-independence testing convention, e.g. `list_input_devices_does_not_panic`).
</objective>

<execution_context>
@$HOME/.claude/gsd-core/workflows/execute-plan.md
@$HOME/.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@crates/yogurt-audio/src/mic.rs
@crates/yogurt-audio/src/lib.rs
@crates/yogurt-audio/src/error.rs
@crates/yogurt-server/src/meetings.rs
@crates/yogurt-server/src/routes.rs
@crates/yogurt-server/src/audio.rs
@crates/yogurt-db/src/settings.rs
@web/src/components/settings/AudioSection.tsx
@web/src/lib/api/settings.ts
@web/src/routes/Meeting.tsx
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: yogurt-audio — device-targeted mic capture + in-place hot-swap</name>
  <files>crates/yogurt-audio/src/mic.rs, crates/yogurt-audio/src/lib.rs, crates/yogurt-audio/examples/wav_eartest.rs, crates/yogurt-audio/examples/dual_smoke.rs, crates/yogurt-audio/examples/mic_smoke.rs</files>
  <behavior>
    - Test (mic.rs `#[cfg(test)] mod tests`): `spawn_mic_capture(tx, Some("definitely-not-a-real-device-xyz123"))` returns `Err(AudioError::MicUnavailable(_))`. This is hardware-independent — the name matches no device on any machine — so it deterministically proves the by-name lookup branch without depending on real input hardware (matches the existing `list_input_devices_does_not_panic` philosophy: never assert on hardware presence).
  </behavior>
  <action>
    In `crates/yogurt-audio/src/mic.rs`, change `pub fn spawn_mic_capture(tx: broadcast::Sender&lt;Frame&gt;) -&gt; Result&lt;MicCapture&gt;` to `pub fn spawn_mic_capture(tx: broadcast::Sender&lt;Frame&gt;, requested_device: Option&lt;&amp;str&gt;) -&gt; Result&lt;MicCapture&gt;`. Replace the current unconditional `host.default_input_device()` lookup: when `requested_device` is `Some(name)` with a non-empty `name`, resolve the device via `host.input_devices()?.find(|d| d.name().map(|n| n == name).unwrap_or(false))`, returning `AudioError::MicUnavailable(format!("input device not found: {name}"))` if no match; when `requested_device` is `None` or `Some("")`, keep the existing `host.default_input_device()` fallback with its existing error. The rest of the function (building the stream, spawning the drainer) is unchanged — the local `device_name` variable derived from `device.name()` after resolution still becomes `MicCapture.device_name` exactly as today.

    In `crates/yogurt-audio/src/lib.rs`, change `pub fn start_capture() -&gt; Result&lt;AudioStream&gt;` to `pub fn start_capture(mic_device: Option&lt;&amp;str&gt;) -&gt; Result&lt;AudioStream&gt;`, forwarding it to `spawn_mic_capture(mic_tx.clone(), mic_device)`. Update the `no_run` doc example above `AudioStream` (currently `yogurt_audio::start_capture()?`) to `yogurt_audio::start_capture(None)?` so `cargo test --doc` still compiles. Add a new method to `impl AudioStream`, after `subscribe_system`: `pub fn switch_mic_device(&amp;mut self, device_name: Option&lt;&amp;str&gt;) -&gt; Result&lt;String&gt;`. It must call `spawn_mic_capture(self.mic_tx.clone(), device_name)` FIRST and only on `Ok` replace `self._mic` with the new `MicCapture` (dropping the old one, which stops its cpal stream + aborts its drainer via existing `Drop` impl / RAII) — on `Err`, return the error and leave `self._mic` untouched, so a bad device id never interrupts a live capture. Return `Ok(new_mic.device_name.clone())` so callers learn the resolved device name. `mic_tx` is reused unchanged, so every existing subscriber (the meeting's Frame→AudioChunk adapter) keeps receiving frames across the swap with no resubscribe and no gap.

    Fix the three examples that call the now-two-argument functions: `wav_eartest.rs` line ~95 `start_capture()?` → `start_capture(None)?`; `dual_smoke.rs` line ~28 same change; `mic_smoke.rs` line ~27 `spawn_mic_capture(tx)?` → `spawn_mic_capture(tx, None)?`.
  </action>
  <verify>
    <automated>cargo build -p yogurt-audio --examples &amp;&amp; cargo test -p yogurt-audio --lib mic &amp;&amp; cargo test -p yogurt-audio --doc</automated>
  </verify>
  <done>`spawn_mic_capture` and `start_capture` take an optional device selector; `AudioStream::switch_mic_device` hot-swaps `_mic` only on success; the new unknown-device unit test passes; all three examples and the crate's doctests still compile.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: yogurt-server — capture-thread command channel, hot-swap endpoint, persisted-device default</name>
  <files>crates/yogurt-server/src/meetings.rs, crates/yogurt-server/src/routes.rs, crates/yogurt-server/src/audio.rs, crates/yogurt-server/tests/audio_device_switch.rs</files>
  <behavior>
    - Unit test in `meetings.rs` (`run_capture_control_loop_services_commands_then_exits_cleanly`): build a real multi-thread `tokio::runtime::Runtime`, spawn `run_capture_control_loop` on a plain `std::thread::spawn` (mirroring how `Registry::start` runs it), drive a `Arc&lt;std::sync::Mutex&lt;Vec&lt;String&gt;&gt;&gt;`-backed fake `switch` closure (no real cpal/SCK) that records each requested device name and echoes back `Ok(format!("resolved:{name}"))`. From the test's own `rt.block_on`, send three `AudioCommand::SwitchMicDevice` commands in sequence over the real `mpsc::Sender`, awaiting each reply through its `oneshot::Receiver` with a 1s `tokio::time::timeout` (a timeout firing means the select loop deadlocked — the test must fail loudly, not hang). Assert all three replies arrive in order and match `resolved:&lt;name&gt;`. Then `drop(shutdown_tx)` and `worker.join()` the `std::thread::JoinHandle` (unbounded, but must return promptly since the loop must observe the dropped oneshot and break) — a hang here means the loop can't exit cleanly, which the test surfaces as a stuck test run rather than a silent pass. Finally assert the recorded device names equal `["mic-a", "mic-b", "mic-c"]` in order. This proves the real `tokio::select!` control loop — the actual concurrency primitive at risk — services multiple hot-swap commands without deadlock and terminates cleanly on shutdown, entirely independent of audio hardware (mirrors how `pump_audio_adapter` is tested today).
    - Integration tests in the new `crates/yogurt-server/tests/audio_device_switch.rs` (copy the `spawn_server()` tempdir-isolated helper pattern from `crates/yogurt-server/tests/meeting_rest.rs`): (1) `POST /api/meetings/{random-uuid}/audio-device` with a valid token returns 404; (2) create a meeting via `POST /api/meetings` but never call `/start`, then `POST .../audio-device` returns 409; (3) `POST .../audio-device` with NO token returns 403 (WR-08 regression, mirroring `it_rejects_unauthenticated_audio_calls` in `audio_api.rs`). None of these require real audio hardware or Screen Recording permission — they exercise only the "meeting not found" / "not recording" / "no token" branches, which is as far as this environment can safely exercise `Registry::start` (which requires real macOS TCC permissions this sandbox does not have — no existing test in this crate calls `Registry::start` successfully either, e.g. `it_rejects_start_without_api_key` returns before audio capture opens).
  </behavior>
  <action>
    In `crates/yogurt-server/src/meetings.rs`: add `mpsc` to the existing `use tokio::sync::{broadcast, oneshot, Mutex, RwLock};` import. Define `pub enum AudioCommand { SwitchMicDevice { device_name: String, reply: oneshot::Sender&lt;std::result::Result&lt;String, String&gt;&gt; } }` and `#[derive(Debug)] pub enum SwitchDeviceError { NotFound, NotRecording, Device(String) }` near the `MeetingId` type alias. Add a field to `Meeting`: `pub audio_cmd_tx: Mutex&lt;Option&lt;mpsc::Sender&lt;AudioCommand&gt;&gt;&gt;`, initialized to `Mutex::new(None)` in `Meeting::new()`.

    Change `Registry::start`'s signature to `pub async fn start(&amp;self, id: &amp;MeetingId, stt_settings: SttSettings, mic_device: Option&lt;String&gt;) -&gt; Result&lt;()&gt;`. Right after the existing `oneshot::channel` pair is created, add `let (cmd_tx, cmd_rx) = mpsc::channel::&lt;AudioCommand&gt;(4);` — `cmd_rx` and `mic_device` are captured by the existing `move ||` capture-thread closure automatically (no extra plumbing needed since the closure is already `move`). Inside that closure, change `let stream = match yogurt_audio::start_capture() {` to `let mut stream = match yogurt_audio::start_capture(mic_device.as_deref()) {` (mut is required because `switch_mic_device` takes `&amp;mut self`). Replace the tail — currently `let _ = shutdown_rx.blocking_recv(); drop(stream);` — with a call to a new extracted function `run_capture_control_loop(&amp;rt_handle, shutdown_rx, cmd_rx, |name| { let opt = if name.is_empty() { None } else { Some(name) }; stream.switch_mic_device(opt).map_err(|e| e.to_string()) })` followed by `drop(stream);` unchanged. Define `run_capture_control_loop` as a standalone `fn` near `pump_audio_adapter` (same "extracted for testability" section): signature `fn run_capture_control_loop(rt_handle: &amp;tokio::runtime::Handle, mut shutdown_rx: oneshot::Receiver&lt;()&gt;, mut cmd_rx: mpsc::Receiver&lt;AudioCommand&gt;, mut switch: impl FnMut(&amp;str) -&gt; std::result::Result&lt;String, String&gt;)`. Body: `rt_handle.block_on(async { loop { tokio::select! { _ = &amp;mut shutdown_rx =&gt; break, cmd = cmd_rx.recv() =&gt; match cmd { Some(AudioCommand::SwitchMicDevice { device_name, reply }) =&gt; { let _ = reply.send(switch(&amp;device_name)); } None =&gt; break, } } } })`. This reuses the `rt_handle` already captured earlier in `Registry::start`'s capture-thread closure for `tokio::runtime::Handle::current()` — no new runtime handle needed.

    Right after `*m.capture_thread.lock().await = Some(capture_thread);` near the end of `start()`, add `*m.audio_cmd_tx.lock().await = Some(cmd_tx);`. In `Registry::stop`, as the very first statement after `let m = self.get(id).await...;`, add `*m.audio_cmd_tx.lock().await = None;` so any switch request racing with shutdown reliably observes "not recording" rather than sending into a channel whose receiver is about to vanish.

    Add a new method to `impl Registry`, after `subscribe`: `pub async fn switch_mic_device(&amp;self, id: &amp;MeetingId, device_name: String) -&gt; std::result::Result&lt;String, SwitchDeviceError&gt;`. Look up the meeting (`SwitchDeviceError::NotFound` if absent); clone `audio_cmd_tx` under its lock (`SwitchDeviceError::NotRecording` if `None`); build a fresh `oneshot::channel`; `tx.send(AudioCommand::SwitchMicDevice { device_name, reply: reply_tx }).await` mapping a send error to `SwitchDeviceError::NotRecording` (receiver dropped mid-race); await the reply through `tokio::time::timeout(Duration::from_secs(5), reply_rx)`, mapping `Ok(Ok(Ok(name)))` → `Ok(name)`, `Ok(Ok(Err(msg)))` → `Err(SwitchDeviceError::Device(msg))`, `Ok(Err(_))` (sender dropped) → `Err(SwitchDeviceError::NotRecording)`, and a timeout → `Err(SwitchDeviceError::Device("timed out waiting for capture thread to switch device".into()))`.

    In `crates/yogurt-server/src/routes.rs`: add `.route("/api/meetings/{id}/audio-device", post(switch_meeting_audio_device))` to the `meeting_routes` router (alongside the existing `/start` and `/stop` routes, so it inherits the same `require_session_token` layer). Add `#[derive(Deserialize)] struct SwitchDeviceRequest { device_id: String }` and handler `async fn switch_meeting_audio_device(State(state): State&lt;AppState&gt;, Path(id): Path&lt;Uuid&gt;, Json(body): Json&lt;SwitchDeviceRequest&gt;) -&gt; impl IntoResponse` that calls `state.meetings.switch_mic_device(&amp;id, body.device_id).await` and maps `Ok(device)` → `200 {"status":"switched","device":device}`, `Err(SwitchDeviceError::NotFound)` → `404 {"error":"meeting not found"}`, `Err(SwitchDeviceError::NotRecording)` → `409 {"error":"meeting is not currently recording"}`, `Err(SwitchDeviceError::Device(msg))` → `400 {"error":msg}` (import `crate::meetings::SwitchDeviceError`). Rework `start_meeting` to first bind `let g = match yogurt_db::settings::load_general(&amp;state.db) { Ok(g) =&gt; g, Err(e) =&gt; { ...unchanged 500 branch... } };`, derive `let mic_device = if g.audio_input_device.is_empty() { None } else { Some(g.audio_input_device.clone()) };`, build `let stt_settings = crate::meetings::SttSettings::from(&amp;g);` as before, and call `state.meetings.start(&amp;id, stt_settings, mic_device).await`.

    In `crates/yogurt-server/src/audio.rs`, update the now-unused-but-still-compiled `start_meeting_recording` to match the new signature: `pub fn start_meeting_recording(mic_device: Option&lt;&amp;str&gt;) -&gt; Result&lt;AudioStream, AudioError&gt; { start_capture(mic_device) }`.

    Create `crates/yogurt-server/tests/audio_device_switch.rs` with the three tests described in `&lt;behavior&gt;`, copying the `spawn_server()` helper verbatim from `crates/yogurt-server/tests/meeting_rest.rs` (tempdir-isolated `RunConfig`, ephemeral port, health-poll loop, pre-seeded session token).
  </action>
  <verify>
    <automated>cargo build --workspace &amp;&amp; cargo test -p yogurt-server run_capture_control_loop &amp;&amp; cargo test -p yogurt-server --test audio_device_switch</automated>
  </verify>
  <done>Workspace builds; the capture-thread control-loop unit test proves in-order command handling and clean shutdown with no deadlock; the three new REST integration tests (404 unknown meeting, 409 not recording, 403 no token) pass; `start_meeting` now reads the persisted `audio_input_device` setting and passes it through to `Registry::start`.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 3: frontend — live mic device picker in the meeting toolbar</name>
  <files>web/src/lib/api/settings.ts, web/src/components/settings/AudioSection.tsx, web/src/components/MicDevicePicker.tsx, web/src/components/MicDevicePicker.test.tsx, web/src/routes/Meeting.tsx</files>
  <behavior>
    - `MicDevicePicker.test.tsx` (Vitest + Testing Library, mirroring `Button.test.tsx`'s style): mock `audioApi.devices` to resolve `[{name:"Built-in Microphone", is_default:true, sample_rate:48000}, {name:"AirPods Pro", is_default:false, sample_rate:16000}]` and mock `audioApi.switchMeetingDevice` as a `vi.fn()` resolving `{status:"switched", device:"AirPods Pro"}`.
    - Test 1: renders a `combobox` with both device names as options, defaulting to the `is_default` device ("Built-in Microphone").
    - Test 2: selecting "AirPods Pro" calls `audioApi.switchMeetingDevice` with `(meetingId, "AirPods Pro")`.
    - Test 3: after the switch mutation resolves, the select's displayed value becomes "AirPods Pro" (the value from the mutation response, not just whatever `is_default` says) — this is the "reflects the currently-active device" requirement, so a second render/re-query does not silently revert the picker back to the OS default.
    - Test 4: while the switch mutation is pending, the select is `disabled` and a "Switching…" indicator is visible; on mutation error, an inline error message renders and the select's value stays on the last-known-good device (matching `AudioSection.tsx`'s loading/disabled pattern).
  </behavior>
  <action>
    In `web/src/lib/api/settings.ts`: fix the `AudioDevice` interface — it currently declares `id: string`, but the backend `DeviceInfo` struct (`crates/yogurt-audio/src/mic.rs`) only ever serializes `{name, is_default, sample_rate}`; there is no `id`. `AudioSection.tsx` line 45 keys/values its `&lt;option&gt;` on `d.id`, which is always `undefined` at runtime — every option silently collides with the `value=""` "System default" option, so the Settings picker can never actually persist a real device name (this directly blocks the `audio_input_device` default-wiring landed in Task 2: if Settings can never save a real device, the persisted-default feature has nothing to read). Remove `id: string` from `AudioDevice` (keep `name: string; is_default: boolean; sample_rate?: number | null` matching the wire shape already in `DeviceInfo`). Update `AudioSection.tsx` line 45 from `&lt;option key={d.id} value={d.id}&gt;` to `&lt;option key={d.name} value={d.name}&gt;` — the device *name* is the identifier the backend already matches on (`Registry`/`spawn_mic_capture` resolve by name). Add to the `audioApi` object: `switchMeetingDevice: (meetingId: string, deviceId: string) => http&lt;{status: string; device: string}&gt;(\`/api/meetings/${meetingId}/audio-device\`, {method: "POST", body: JSON.stringify({device_id: deviceId})})`.

    Create `web/src/components/MicDevicePicker.tsx`: a component taking `{meetingId: string}` (no `token` prop needed — `http()`/`bearerFetch` already resolve the session token internally via `ensureSessionToken()`, matching `AudioSection.tsx`'s pattern of never threading tokens through props). Use `useQuery({queryKey: ["audio-devices"], queryFn: audioApi.devices})` — same query key as `AudioSection.tsx` so the two share one cached fetch. Hold a local `const [activeDevice, setActiveDevice] = useState&lt;string | null&gt;(null)` — this is the "reflects the currently-active device" state, distinct from whichever device the OS reports as `is_default`. Compute the effective displayed value as `activeDevice ?? devices.data?.find((d) =&gt; d.is_default)?.name ?? devices.data?.[0]?.name ?? ""` (falls back to the OS default only until the user has actually switched once). Use `useMutation({mutationFn: (deviceId: string) =&gt; audioApi.switchMeetingDevice(meetingId, deviceId), onSuccess: (data) =&gt; setActiveDevice(data.device)})` for the switch — `onSuccess` is what makes the picker reflect the real active device after a switch, instead of resetting to the default on every re-render. Render: while `devices.isLoading`, a small muted "Loading mics…" label; on `devices.isError`, a small error label; otherwise a `&lt;select aria-label="Microphone"&gt;` populated from `devices.data` (`key`/`value` = `d.name`, label = `d.name + (d.is_default ? " (default)" : "")`), controlled via `value={effectiveValue}` (NOT `defaultValue` — must be controlled so a successful switch is reflected immediately), `disabled={switchDevice.isPending}`, `onChange` calling `switchDevice.mutate(e.target.value)`. Below the select, show "Switching…" while `switchDevice.isPending`, or the mutation's error message (`switchDevice.error instanceof Error ? switchDevice.error.message : "Switch failed"`) when `switchDevice.isError` — on error, `activeDevice` is deliberately left unchanged (only `onSuccess` updates it), so the select stays on the last device that actually succeeded, matching the "leaves the previous microphone capturing" backend guarantee. Match the existing toolbar's compact sizing (`text-[12px] font-mono`, `rounded-md border border-neutral-300`) rather than `AudioSection.tsx`'s larger Settings-page styling — this control lives in the meeting header, not a full settings panel.

    In `web/src/routes/Meeting.tsx`: import `MicDevicePicker` from `../components/MicDevicePicker`. In the header's button row (the `div` at line ~275 containing the Start/Stop/End buttons), render `{meetingId &amp;&amp; recording &amp;&amp; &lt;MicDevicePicker meetingId={meetingId} /&gt;}` — placed before the Stop-recording button so it reads left-to-right as "current input, then stop control." Only visible while actively recording, matching the design decision.
  </action>
  <verify>
    <automated>cd web &amp;&amp; pnpm exec tsc --noEmit &amp;&amp; pnpm test</automated>
  </verify>
  <done>`MicDevicePicker` renders the device list with the default pre-selected, calls `audioApi.switchMeetingDevice` on selection, updates its controlled value to the resolved device on success (and leaves it unchanged on error), and shows loading/switching/error states; it appears in `Meeting.tsx`'s toolbar only while `recording` is true; the `AudioSection.tsx` id/name mismatch is fixed so the Settings page can actually persist a real device name; `pnpm exec tsc --noEmit` and `pnpm test` both pass.</done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| browser -> `POST /api/meetings/:id/audio-device` | user-controlled `device_id` string reaches a cpal device-name lookup |
| tokio task -> capture `std::thread` | new `mpsc::Sender&lt;AudioCommand&gt;` crosses the existing cross-thread bridge alongside the shutdown `oneshot` |

## STRIDE Threat Register

| Threat ID | Category | Component | Disposition | Mitigation Plan |
|-----------|----------|-----------|-------------|-----------------|
| T-wnn-01 | Tampering | `switch_meeting_audio_device` `device_id` body field | mitigate | Value is only ever used as a pure string-equality lookup against `host.input_devices()` names (`mic.rs`); no shell/path/FFI use. An unmatched name yields a typed `AudioError::MicUnavailable` → 400, never a panic — the pre-existing capture keeps running. |
| T-wnn-02 | Denial of Service | Rapid repeated device-switch requests churning the capture thread | accept | Endpoint sits behind the existing `require_session_token` middleware (WR-08) — same trust boundary as `/start`/`/stop`, already accepted as single-user-localhost risk in prior phases. |
| T-wnn-03 | Denial of Service | Capture thread wedged mid-switch, `Registry::switch_mic_device` awaits forever | mitigate | `tokio::time::timeout(5s, reply_rx)` bounds the wait; a timeout surfaces as a 400 with an explicit message rather than hanging the request. |
| T-wnn-04 | Information Disclosure | Device names returned in the switch response | accept | Identical surface already exposed by the pre-existing, session-token-gated `GET /api/audio/devices` — no new disclosure. |
</threat_model>

<verification>
- `cargo build --workspace` succeeds.
- `cargo test -p yogurt-audio --lib mic` and `cargo test -p yogurt-audio --doc` pass (device-by-name lookup + doctest fix).
- `cargo test -p yogurt-server run_capture_control_loop` passes — proves the real `tokio::select!` capture-thread control loop services multiple hot-swap commands in order and exits cleanly on shutdown, with no deadlock, independent of real audio hardware.
- `cargo test -p yogurt-server --test audio_device_switch` passes (404 / 409 / 403 REST contract).
- `cd web && pnpm exec tsc --noEmit && pnpm test` passes, including the new `MicDevicePicker.test.tsx`.
- Manual smoke (developer machine only, not part of automated verify): start a recording with two input devices connected, switch via the toolbar dropdown mid-meeting, confirm the live transcript dock keeps producing text with no visible gap and the dropdown reflects the newly active device.
</verification>

<success_criteria>
- Switching the mic device mid-recording never restarts system audio or the STT session, and never drops the existing `mic_tx` broadcast subscribers.
- A device that fails to open leaves the prior device capturing and reports an error instead of ending the recording.
- The Settings page's persisted `audio_input_device` now actually determines which mic a new recording opens.
- The new REST endpoint enforces the same session-token auth as the rest of the meeting lifecycle surface, and returns 404/409 appropriately.
- The toolbar picker's displayed value tracks the actual active device, not a static default.
</success_criteria>

<output>
Create `.planning/quick/260709-wnn-add-live-mic-device-switching-during-mee/260709-wnn-SUMMARY.md` when done.
</output>
