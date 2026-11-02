# Yogurt for Android

A small app that opens your Yogurt Cloud in a full-screen WebView and records meetings in the background.
Every screen is the Cloud webapp, so each Vercel deploy updates the app with no reinstall.
The recorder runs in a foreground service, so a recording keeps going while you switch apps or lock the phone.
The design is [docs/.planning/mobile-recording-design.md](../docs/.planning/mobile-recording-design.md#the-android-app).

## What it sends where

The mic's audio goes to Deepgram, through a short-lived token your Cloud grants, and is never written to storage.
While the socket is down, up to 5 minutes of audio waits in memory and replays once a new socket opens.
Past that cap the oldest audio drops, and the page shows a gap.
Final transcript lines are kept in the app's own storage until the next recording starts, so the page can fetch any it missed.
On launch the app asks `api.github.com` for the latest yogurt release, to offer an update.
There is no analytics and no Google Play services.

## Install

On the phone, open the latest [release](https://github.com/jarvisrchen/yogurt/releases/latest), download `yogurt-android-<version>.apk`, allow Chrome to install unknown apps once, and tap Install.
From a Mac with the phone or an emulator connected over adb:

```bash
adb install -r yogurt-android-<version>.apk
```

## First launch

The app asks once for your Cloud address, such as `https://yogurt-example.vercel.app`, and then opens it.
Only https addresses are accepted.
To change it later, long-press the app icon and pick Change Cloud address.
If the Cloud cannot be reached at launch, the app shows the address screen again with the address filled in.

When a newer release carries an APK, a banner at the top offers the update.
Tapping it downloads the APK and hands it to Android's installer, which keeps your sign-in.

## Build

The build needs JDK 17 and the Android SDK with platform 36, at `~/Library/Android/sdk` unless `ANDROID_HOME` says otherwise.

```bash
just android            # debug APK and unit tests: android/app/build/outputs/apk/debug/app-debug.apk
just android-release    # signed release APK: android/app/build/outputs/apk/release/app-release.apk
```

`just android-release` signs with `~/.yogurt/android-release.jks`, reading its password from Keychain (`security find-generic-password -a yogurt -s yogurt-android-keystore -w`).
Gradle reads the keystore from `YOGURT_ANDROID_KEYSTORE` and `YOGURT_ANDROID_KEYSTORE_PASSWORD`, and without them it builds an unsigned release APK.
Every release must use that same key, since Android refuses an update signed with another.
The release pipeline builds the APK from repo secrets and attaches it to each GitHub release, as [docs/RELEASING.md](../docs/RELEASING.md) describes.
Every yogurt release ships the APK under its own version, which Gradle takes from the tag, or locally from `[workspace.package]` in `Cargo.toml`.

## Test

Unit tests cover the reconnect queue, the line store, Deepgram message parsing, version comparison, and the Cloud address check.
They run on the JVM with `just android`.

The end-to-end test drives the debug app on an emulator against `tools/fake-cloud.mjs`, a fake Cloud, Deepgram, and GitHub release on the Mac, reached through `adb reverse`.
Its test page calls the bridge the way the webapp does, and the recorder reads a generated wav instead of the mic.
The wav stamps every sample with its second, so a lost or repeated line after a reconnect shows up as a gap or a repeat in the "second N" lines.
It checks the first launch screen, recording through Home and a screen turned off for 75 seconds, the notification and its Stop, a reconnect after the fake server dies with no lost or repeated line, `lines(n)` after a reload, `stop()` returning only after the last line, and the update download and install intent.
It wipes the app's data first, and takes about 4 minutes.

```bash
# once: the SDK pieces and a Pixel AVD
sdkmanager --sdk_root="$HOME/Library/Android/sdk" "cmdline-tools;latest" platform-tools emulator "platforms;android-36" "system-images;android-36;google_apis;arm64-v8a"
~/Library/Android/sdk/cmdline-tools/latest/bin/avdmanager create avd -n yogurt -k "system-images;android-36;google_apis;arm64-v8a" -d pixel_9_pro_xl

~/Library/Android/sdk/emulator/emulator -avd yogurt -no-window -no-snapshot -no-boot-anim -no-audio &
just android && node android/tools/e2e.mjs
```

The Stop check sends the intent the notification's Stop button carries, as the app's own user, because `uiautomator` cannot read the shade while the notification's clock ticks.
To try the app by hand against the fake, run `node android/tools/fake-cloud.mjs 8787`, `adb reverse tcp:8787 tcp:8787`, enter `http://localhost:8787` as the Cloud, and queue commands for the test page with `curl -d '{"cmd":"start","meetingId":"m-1"}' localhost:8787/cmd` (or `stop`, `reload`).

Debug builds also open `http://localhost` and `http://10.0.2.2` Clouds, and take two launch extras that release builds do not contain:

```bash
adb shell am start -n com.jarvisrchen.yogurt/.MainActivity --es wav test.wav --es releasesUrl http://localhost:8787/releases/latest
```

`wav` names a 16 kHz mono wav in the app's files directory to record instead of the mic, and `releasesUrl` replaces the GitHub releases address.
An empty value turns either back off.

## The bridge

The page talks to the recorder through `window.YogurtAndroid`, as the design's [bridge contract](../docs/.planning/mobile-recording-design.md#bridge-contract) specifies.
Calls work only while the WebView shows the Cloud origin you entered, and the app opens any other address in the browser.
`start()` refuses a `tokenUrl` off that origin, since the recorder sends the session to it: a POST with the JWT as a Bearer header and `{"token": browserToken}` as the body, which is what `cloud/api/stt-token.ts` reads, expecting `{"token","url"}` back.
The recorder opens `url` with the granted token as `Authorization: Bearer`, which Deepgram's token guide documents.
`stop()` returns at once, and the recorder then sends Deepgram's last lines as events, waiting up to 5 seconds for them, followed by a `stopped` state event, after which `lines(n)` holds every line.
`start()` returns `{"ok":true}` while Android asks for the microphone, and the recording then starts, or ends with an `error` and a `stopped` state event.
Events wait in the app while no Cloud page is loaded and go out once one is, except interim text.

## Licenses

The app is MIT licensed like the rest of yogurt.
It bundles OkHttp and AndroidX Core (Apache-2.0), and the Manrope and JetBrains Mono fonts (SIL Open Font License 1.1).
