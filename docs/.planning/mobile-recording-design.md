# Recording on the phone

Planned. Written 2026-11-02.
Tickets: `docs/TODO.md` MOB-1, MOB-2, MOB-3, MOB-5, and SYNC-32.
Mocks, light and dark at Pixel 10 Pro XL size: [.lavish/mobile-recording-mocks.html](../.lavish/mobile-recording-mocks.html).

The Yogurt Cloud webapp gains a Record button so a phone can capture a meeting the way the Mac does.
The phone records the room through its own mic, shows a live transcript, takes the user's sparse notes, and enhances them at End.
The meeting lands in the Cloud as a meeting from a phone device.
The next time a Mac pushes, it adopts the meeting into its own SQLite, where the user can edit and re-enhance it like any other.

Android is the target, through a small Android app that wraps the webapp and records in the background.
Any browser, including iOS Safari, gets the same record screen while it stays in front, with the limits below.
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

In a browser, the page must stay in front.
On iOS, a home-screen web app loses the mic when it backgrounds or the screen locks (WebKit bugs 239602 and 226620), while a Safari tab reportedly keeps it.
On Android Chrome, no document says whether capture survives switching apps.
The browser record screen holds a Screen Wake Lock and says plainly that locking the phone or switching apps can stop the recording.
The Android app removes this limit, because its recorder runs in a foreground service.

## Transcription

The phone streams audio to Deepgram `nova-3` live, the same model the Mac uses.
On-device transcription is MOB-4, a later spike: a small model such as Moonshine in the page through Transformers.js, measured on a real phone over a full hour for speed, heat, and battery.

The Deepgram key lives in the Vercel project's environment as `YOGURT_DEEPGRAM_API_KEY`, Sensitive and Production only, beside `YOGURT_LLM_API_KEY`.
The browser never sees it.
A new function, `cloud/api/stt-token.ts`, checks the caller is the signed-in owner with an unlocked browser token, then calls Deepgram's `POST /v1/auth/grant` for a 30 second token and returns only that ([Deepgram token-based auth](https://developers.deepgram.com/guides/fundamentals/token-based-authentication)).
The token only has to be valid when the WebSocket opens, and the page asks for a fresh one on every reconnect.
Browsers cannot set headers on a WebSocket, so the token goes in `Sec-WebSocket-Protocol` ([Deepgram: using the Sec-WebSocket-Protocol](https://developers.deepgram.com/docs/using-the-sec-websocket-protocol)).
The docs show the `token, <api key>` form for a key, and a granted token goes as `bearer, <token>`.

Minting a token needs a key with the Member role or higher.
The key in `.env.local` was checked on 2026-11-02 and gets `403 FORBIDDEN, Insufficient permissions`, so the setup docs tell the user to create a separate Member key for Vercel.
A separate key is better anyway, since it can be revoked without touching the Mac.

### Audio path

The page opens the mic with `getUserMedia` and downsamples to 16 kHz mono linear16 in an AudioWorklet, the Mac's frame contract.
Self-contained PCM chunks make a reconnect simple: every chunk stays in memory until a final result covers it, up to 5 minutes (about 9.6 MB), and the held chunks replay in order once a new socket opens.
Holding the chunks Deepgram has but has not finalized means a drop loses none of them, and since a replay starts after the last final, no line repeats.
Past that cap the oldest chunks drop and the transcript shows a gap marker.
`MediaRecorder` with WebM was rejected because a new socket needs a fresh container header, so a reconnect would lose everything buffered.
Audio is never written to storage, so nothing is left to delete after transcription.

A final Deepgram result becomes one transcript line, `{"channel":"them","text":...,"ts_ms":...}`, with `ts_ms` measured from the recording start like the Mac's.
Interim results show greyed in the live view and are never stored.
The meeting's `stt_engine` is `cloud · nova-3 · phone`.

## The Android app

Switching apps mid-meeting is normal, and only a native recorder can keep the mic there.
The app is a plain Kotlin project in `android/`, with no Capacitor, id `com.jarvisrchen.yogurt`, built with Gradle and the Android SDK.
It targets the current SDK and is tested on a Pixel 10 Pro XL.

It has three parts:

- **Shell.** One activity with a full-screen WebView that loads the Cloud address the user enters on first launch.
  Every screen is the webapp, so each Vercel deploy updates the app with no reinstall.
- **Recorder.** A foreground service of type `microphone`, started from Record while the app is on screen as Android 14 and later require, with a persistent Recording notification and a Stop action.
  It owns the mic, the Deepgram socket, and the 5 minute buffer, so the recording survives switching apps and a locked screen.
  It asks the page for a fresh Deepgram token through the bridge, keeps every final line in memory and in a file, and hands the lines to the page whenever the page is visible.
- **Bridge.** A `@JavascriptInterface` object, `YogurtAndroid`, that the webapp feature-detects.
  The record screen talks to one `Recorder` interface with two implementations: the browser one from MOB-1, and the bridge one when `window.YogurtAndroid` exists.
  The page still does every push, so the device, bundle, and push code exist once.
  The bridge accepts calls only from the Cloud origin the user entered.

### Bridge contract

Every value crossing the bridge is a JSON string, because `@JavascriptInterface` passes only strings and numbers.

| Call, page to app | Argument | Returns |
| --- | --- | --- |
| `YogurtAndroid.start(json)` | `{"meetingId","startedAtMs","have","tokenUrl","auth":{"jwt","browserToken"}}`, where `have` is how many of the meeting's lines the page holds | `{"ok":true}` or `{"ok":false,"error"}` |
| `YogurtAndroid.setAuth(json)` | `{"jwt","browserToken"}`, sent whenever the page refreshes its session | nothing |
| `YogurtAndroid.stop()` | none | nothing, at once; the last lines and then a `stopped` state follow as events |
| `YogurtAndroid.status()` | none | `{"recording","meetingId","startedAtMs","state","lines"}`, where `lines` is the index the next line gets |
| `YogurtAndroid.lines(meetingId, from)` | the meeting, and the index of the first line wanted | JSON array of that meeting's final lines from that index, empty for any other meeting |
| `YogurtAndroid.appVersion()` | none | the app's version name, such as `"0.1.0"` |

The app posts events to the page with `window.dispatchEvent(new CustomEvent("yogurt-native", {detail}))`, where `detail` is one of:

- `{"type":"line","meetingId","index","line":{"channel":"them","text","ts_ms"}}` for each final line.
- `{"type":"interim","text"}` for the words Deepgram has not finalized.
- `{"type":"state","state":"connecting"|"live"|"reconnecting"|"stopped"}`.
- `{"type":"gap","from_ms","to_ms"}` when the buffer overflowed and audio was dropped.
- `{"type":"error","message"}`.
- `{"type":"auth"}` when the Cloud refused the session the app holds, which the page answers with `setAuth`.

The service mints its own Deepgram token by calling `tokenUrl` with the latest `auth`, so a reconnect works while the page is in the background.
A page in the background can stop renewing its session, so when `tokenUrl` answers 401 the app sends an `auth` event, and the page renews its session and calls `setAuth`, which the next attempt uses.
The WebView keeps running while the foreground service holds the process, since the app never pauses its timers, and the refresh token never leaves the page.
If no page answers, the audio waits in the 5 minute buffer while the app keeps asking.
A page that joins a running recording also hands the app its session.
The call is a POST with the JWT as `Authorization: Bearer` and `{"token": browserToken}` as the body, the same request the browser makes, and the answer is `{"token","url"}`.
The app opens `url` with the granted token in an `Authorization: Bearer` header, where the browser, which cannot set headers on a WebSocket, offers the `bearer` subprotocol with the token.
Final lines are numbered once for the whole meeting and kept in a file until another meeting starts.
A new meeting numbers from `have`, and a `start` of the meeting the file already holds carries on its numbering, so a resume after the app was killed keeps one numbering with the lines the page already has.
A page that comes back calls `lines(meetingId, n)` with the count it already has and never misses or repeats one.
Lines and line events carry the meeting, so the page takes none of another meeting's, such as the previous meeting's lines while Android's mic dialog holds up a new `start`.
Events sent before the record screen listens, such as while the page reloads, are not replayed: the page recovers the state from `status()` and the lines from `lines(meetingId, n)` when it attaches.

`stop()` returns at once, because a `@JavascriptInterface` call blocks the page's JavaScript thread until it returns, and stopping waits up to 5 seconds for Deepgram's last results.
The recorder then sends those lines as events, followed by `{"type":"state","state":"stopped"}`, also when nothing was recording.
The page's `NativeRecorder.stop()` waits for that state, at most 10 seconds, and then calls `lines(meetingId, n)` once to take any line it missed.

On launch, the app compares its version with the latest GitHub release and, when a newer one has an APK, shows an Update available banner.
Tapping it downloads the APK and hands it to Android's installer, so an update is one tap and keeps the sign-in.
Every APK is signed with the same release key, kept in Keychain and as a GitHub secret, because Android refuses an update signed with a different key.
The release pipeline builds the APK and attaches it to the GitHub release next to the Mac binary.

Install is a download: open the release on the phone, allow Chrome to install unknown apps once, and tap Install.
Google's developer verification for sideloaded apps started on 2026-09-30 in four countries, with a global rollout planned for 2027.
`adb install` stays exempt, and a free hobbyist account covers personal use when it arrives.

## The phone as a device

The phone registers in `yogurt_devices` the first time it records, with an id it generates and keeps in `localStorage`, and a name the user picks, defaulting to "Phone".
A new column, `kind text not null default 'mac'`, holds `'phone'` for it, so a Mac can tell a phone's meetings from another Mac's.
The insert policy already allows the signed-in owner to add a device, and `yogurt_start_push` already accepts any device the owner owns, so the phone pushes through the same RPCs and the same `{"v":1,"meeting":...,"chat":[...]}` bundle the Mac sends.
The Library, search, meeting view, and Cloud chat then show phone meetings with no changes.
The webapp offers Enhance only on a meeting whose device is a phone, which it reads from `yogurt_phone_devices(token)`, because a Mac's adopted copy keeps the phone's `stt_engine`.

If the user clears site data, the next recording registers a new phone device.
The old device's meetings stay readable, and the Mac adopts them as usual.

While recording, the phone pushes the meeting every 30 seconds with `ended_at` null, so a killed tab loses at most 30 seconds of transcript.
It also keeps the current meeting in IndexedDB.
Reopening the page with an unfinished meeting there offers Resume or End.
Start recording waits until that meeting is resumed or ended, because a new one would replace it in IndexedDB.
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

End runs in this order:

1. The recorder stops, and the Android one is waited for until its last lines are in.
2. The page pushes the final transcript and notes with `ended_at` still null.
3. The meeting view opens with the enhance streaming under EnhancingBanner.
4. The page pushes once more with `ended_at` set, and with `enriched_md`, `llm_model`, and `template` when the enhance worked.
   A failed enhance, or a meeting too short to enhance, is pushed ended without them, and the view offers Enhance.

A Mac adopts only ended meetings, so it never takes one before its enhanced version exists.
The meeting stays in IndexedDB until that last push, so if the tab dies during the enhance, the Resume card's End runs the same steps again.

## Live chat

The meeting view's chat already sends one meeting's transcript to the LLM through `cloud/api/chat.ts`.
During a recording the page pushes the current state before each question, so the chat sees the transcript up to that moment.
The chat function is unchanged.

## The Mac adopts phone meetings

SYNC-32 adds a step to the Mac's push pass, before it plans what to send.

1. An RPC, `yogurt_phone_meetings()`, lists the latest version of each ended, undeleted meeting on a `kind = 'phone'` device, with the phone's name and the version's push id.
2. For each one, the Mac inserts it into SQLite with `source = 'phone'`, the phone's ULID as `source_id`, and the phone's name for provenance, so a rerun is a no-op.
   The row gets a new UUID, because the Mac's meeting routes (chat, Enhance) take a UUID and a phone mints ULIDs.
3. The Mac then calls `yogurt_adopt(device_id, meeting_id, push_id)`, which writes a null version for the phone's copy only if `push_id` is still its latest version and that version has ended, and returns whether this call did it.
   The push id keeps a Mac from adopting a copy the phone has since replaced, such as one pushed again after Enhance.
4. If another Mac adopted it first, this Mac deletes its local insert, unless the row was there before the pass or the claim was retried after a lost answer.
5. The rest of the pass pushes the meeting as one of this Mac's, so it shows once in the Library, under the Mac.

Importing before claiming means a crash between the steps leaves the meeting on both sides, which the next pass repairs, rather than on neither.
Once adopted, the phone edits the meeting as it edits every other Mac meeting, by asking that Mac (MOB-6, ARCHITECTURE.md §12).
The Cloud refuses a phone's push of an adopted meeting, so an Enhance started on the phone before the adoption cannot bring the phone's copy back, and the page says the meeting now lists under the Mac.
A meeting still recording is never adopted, because its `ended_at` is null.

## Constraint changes

AGENTS.md and ARCHITECTURE.md §9 and §12 change in MOB-1, and MOB-5 adds the Android app:

- Audio leaves the phone for Deepgram when the user records there, since a phone has no local STT yet.
- The Cloud's Vercel environment holds the Deepgram key next to the LLM key, Sensitive and Production only, never returned or logged.
- Phone audio is held in memory only while it waits for a socket, and never stored.
- "macOS 13+ only" gains an Android companion app, which records for the Cloud and runs no part of the Mac app.

## Tickets

- **MOB-1** Record on the phone in the webapp: Record button, mic with Wake Lock, `stt-token.ts`, Deepgram streaming with reconnect, the `kind` column, pushes every 30 seconds, and Resume after a killed tab.
- **MOB-5** The Android app: shell, background recorder service, bridge, update check, and the APK in each release.
- **MOB-2** Notes and Enhance on the phone: the editor, `yogurt-wasm`, and `enhance.ts`.
- **MOB-3** Live chat during a phone recording.
- **SYNC-32** The Mac adopts ended phone meetings.
- **MOB-4** On-device transcription spike.

Each lands on its own, in the order MOB-1, MOB-5, MOB-2, MOB-3, SYNC-32, and MOB-1 is useful alone: a recorded, transcribed meeting the Library and Cloud chat can already read.
