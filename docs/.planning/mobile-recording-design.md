# Recording on the phone

Planned. Written 2026-11-02.
Tickets: `docs/TODO.md` MOB-1, MOB-2, MOB-3, and SYNC-32.

The Yogurt Cloud webapp gains a Record button so a phone can capture a meeting the way the Mac does.
The phone records the room through its own mic, shows a live transcript, takes the user's sparse notes, and enhances them at End.
The meeting lands in the Cloud as a meeting from a phone device.
The next time a Mac pushes, it adopts the meeting into its own SQLite, where the user can edit and re-enhance it like any other.

Android in Chrome is the target.
iOS works as best effort in a Safari tab, with the limits below.
The live recap and Mic echo stay Mac-only for now.

## What the phone cannot do

A phone call on the same phone cannot be recorded, on either platform, speakerphone or not.
Android gives the call the mic and hands every other app silence, and starting the capture still succeeds, so the page would record silence with no error ([Android: sharing audio input](https://developer.android.com/media/platform/sharing-audio-input)).
On iOS the call interrupts the page's audio session, and WebKit bug 208209 reports the mic staying silent after the call until the tab is reopened.
A native app would not change this.
What works is a call on another device, such as a laptop on speaker, with the phone recording the room, or a meeting in person.

The phone has one mic, so there is no Me and Them split.
Every line is stored on the `them` channel, which the enhance prompt never reads, so enhance is unaffected.
Speaker labels from Deepgram diarization are a possible follow-up.

The screen must stay awake.
On iOS, a home-screen web app loses the mic when it backgrounds or the screen locks (WebKit bugs 239602 and 226620), while a Safari tab reportedly keeps it.
On Android Chrome, no document says whether capture survives a screen lock, so MOB-1 starts by testing it on a real phone.
The record screen holds a Screen Wake Lock either way, and says plainly that locking the phone or switching apps can stop the recording.

## Transcription

The phone streams audio to Deepgram `nova-3` live, the same model the Mac uses.
On-device transcription is MOB-4, a later spike: a small model such as Moonshine in the page through Transformers.js, measured on a real phone over a full hour for speed, heat, and battery.

The Deepgram key lives in the Vercel project's environment as `YOGURT_DEEPGRAM_API_KEY`, Sensitive and Production only, beside `YOGURT_LLM_API_KEY`.
The browser never sees it.
A new function, `cloud/api/stt-token.ts`, checks the caller is the signed-in owner with an unlocked browser token, then calls Deepgram's `POST /v1/auth/grant` for a 30 second token and returns only that ([Deepgram token-based auth](https://developers.deepgram.com/guides/fundamentals/token-based-authentication)).
The token only has to be valid when the WebSocket opens, and the page asks for a fresh one on every reconnect.
Browsers cannot set headers on a WebSocket, so the token goes in `Sec-WebSocket-Protocol` ([Deepgram: using the Sec-WebSocket-Protocol](https://developers.deepgram.com/docs/using-the-sec-websocket-protocol)).
The docs show the `token, <api key>` form, and MOB-1 confirms the form for a granted token.

Minting a token needs a key with the Member role or higher.
The key in `.env.local` was checked on 2026-11-02 and gets `403 FORBIDDEN, Insufficient permissions`, so the setup docs tell the user to create a separate Member key for Vercel.
A separate key is better anyway, since it can be revoked without touching the Mac.

### Audio path

The page opens the mic with `getUserMedia` and downsamples to 16 kHz mono linear16 in an AudioWorklet, the Mac's frame contract.
Self-contained PCM chunks make a reconnect simple: while the socket is down, chunks queue in memory, up to 5 minutes (about 9.6 MB), and replay in order once a new socket opens.
Past that cap the oldest chunks drop and the transcript shows a gap marker.
`MediaRecorder` with WebM was rejected because a new socket needs a fresh container header, so a reconnect would lose everything buffered.
Audio is never written to storage, so nothing is left to delete after transcription.

A final Deepgram result becomes one transcript line, `{"channel":"them","text":...,"ts_ms":...}`, with `ts_ms` measured from the recording start like the Mac's.
Interim results show greyed in the live view and are never stored.
The meeting's `stt_engine` is `cloud · nova-3 · phone`.

## The phone as a device

The phone registers in `yogurt_devices` the first time it records, with an id it generates and keeps in `localStorage`, and a name the user picks, defaulting to "Phone".
A new column, `kind text not null default 'mac'`, holds `'phone'` for it, so a Mac can tell a phone's meetings from another Mac's.
The insert policy already allows the signed-in owner to add a device, and `yogurt_start_push` already accepts any device the owner owns, so the phone pushes through the same RPCs and the same `{"v":1,"meeting":...,"chat":[...]}` bundle the Mac sends.
The Library, search, meeting view, and Cloud chat then show phone meetings with no changes.

If the user clears site data, the next recording registers a new phone device.
The old device's meetings stay readable, and the Mac adopts them as usual.

While recording, the phone pushes the meeting every 30 seconds with `ended_at` null, so a killed tab loses at most 30 seconds of transcript.
It also keeps the current meeting in IndexedDB.
Reopening the page with an unfinished meeting there offers Resume or End.
Every push sends the full bundle, so a lost push is repaired by the next one.

## Notes and Enhance

The record screen reuses the Mac's notes editor (`web/src/editor`), which the cloud build already imports through `@web`.

At End the phone enhances with the same code the Mac runs.
`yogurt-prompts` and `yogurt-notes` depend only on `pulldown-cmark`, `serde`, `serde_json`, `anyhow`, `tinytemplate`, `rust-embed`, and `html-escape`, and both pass `cargo check --target wasm32-unknown-unknown` as of 2026-11-02.
A new crate, `yogurt-wasm`, exposes `render_enhance` and `merge_notes` through `wasm-bindgen`, and the cloud build bundles it.
One prompt and one merge then serve both platforms, with no TypeScript port to drift.

The page renders the prompt in WASM and sends it to a new function, `cloud/api/enhance.ts`, which checks the owner the same way `chat.ts` does and streams the completion from `YOGURT_LLM_*`.
The page merges the result with the user's notes in WASM and pushes `enriched_md`, `llm_model`, and `template`.
An hour of transcript can stream for over a minute, so the function asks for `maxDuration: 300`, which MOB-2 checks against the account's Vercel plan.
A failed enhance leaves the meeting saved with its notes and transcript, and an Enhance button retries it.

## Live chat

The meeting view's chat already sends one meeting's transcript to the LLM through `cloud/api/chat.ts`.
During a recording the page pushes the current state before each question, so the chat sees the transcript up to that moment.
The chat function is unchanged.

## The Mac adopts phone meetings

SYNC-32 adds a step to the Mac's push pass, before it plans what to send.

1. An RPC, `yogurt_phone_meetings()`, lists the latest version of each ended, undeleted meeting on a `kind = 'phone'` device.
2. For each one, the Mac inserts it into SQLite under the same ULID, with `source = 'phone'` for provenance, so a rerun is a no-op.
3. The Mac then calls `yogurt_adopt(device_id, meeting_id)`, which writes a null version for the phone's copy only if its latest version is still non-null, and returns whether this call did it.
4. If another Mac adopted it first, this Mac deletes its local insert.
5. The rest of the pass pushes the meeting as one of this Mac's, so it shows once in the Library, under the Mac.

Importing before claiming means a crash between the steps leaves the meeting on both sides, which the next pass repairs, rather than on neither.
Once adopted, the meeting is read-only on the phone like every other Mac meeting.
A meeting still recording is never adopted, because its `ended_at` is null.

## Constraint changes

AGENTS.md and ARCHITECTURE.md §9 and §12 change in MOB-1:

- Audio leaves the phone for Deepgram when the user records there, since a phone has no local STT yet.
- The Cloud's Vercel environment holds the Deepgram key next to the LLM key, Sensitive and Production only, never returned or logged.
- Phone audio is held in memory only while it waits for a socket, and never stored.

## Tickets

- **MOB-1** Record on the phone: Record button, mic with Wake Lock, `stt-token.ts`, Deepgram streaming with reconnect, the `kind` column, pushes every 30 seconds, and Resume after a killed tab.
  It starts with a real-device check on Android: screen lock, a phone call, and the token in `Sec-WebSocket-Protocol`.
- **MOB-2** Notes and Enhance on the phone: the editor, `yogurt-wasm`, and `enhance.ts`.
- **MOB-3** Live chat during a phone recording.
- **SYNC-32** The Mac adopts ended phone meetings.
- **MOB-4** On-device transcription spike.

Each lands on its own, and MOB-1 is useful alone: a recorded, transcribed meeting the Library and Cloud chat can already read.
