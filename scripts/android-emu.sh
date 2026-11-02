#!/usr/bin/env bash
# `just android-emu`: boots the `yogurt` emulator with a window and the Mac's mic, installs the debug app, and lets it reach `just cloud-dev`.
set -euo pipefail
cd "$(dirname "$0")/.."
SDK=${ANDROID_HOME:-$HOME/Library/Android/sdk}
ADB=$SDK/platform-tools/adb
AVD=${YOGURT_AVD:-yogurt}
LOG=${TMPDIR:-/tmp}
LOG=${LOG%/}/yogurt-emulator.log

if ! "$ADB" devices | grep -q '^emulator-.*device$'; then
  "$SDK/emulator/emulator" -avd "$AVD" -no-snapshot-save >"$LOG" 2>&1 &
  echo "android-emu: booting $AVD (log in $LOG)"
fi
"$ADB" wait-for-device
until [ "$("$ADB" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ]; do sleep 2; done
# The virtual mic hears the Mac's mic, so you can talk to the app.
"$ADB" emu avd hostmicon >/dev/null

APK=android/app/build/outputs/apk/debug/app-debug.apk
(cd android && JAVA_HOME="$(/usr/libexec/java_home -v 17)" ANDROID_HOME="$SDK" ./gradlew -q assembleDebug)
"$ADB" install -r "$APK" >/dev/null
# The webapp, Supabase, and the Deepgram relay from `just cloud-dev`, at the same localhost ports on the emulator.
for port in 56448 56441 56449; do "$ADB" reverse "tcp:$port" "tcp:$port" >/dev/null; done
"$ADB" shell am start -n com.jarvisrchen.yogurt/.MainActivity --es cloud http://localhost:56448 >/dev/null
echo "android-emu: Yogurt is open on http://localhost:56448, which \`just cloud-dev\` serves."
