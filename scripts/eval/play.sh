#!/usr/bin/env bash
# play.sh - speak scripts/eval/conversation.txt through the speaker so a
# recording meeting captures the same audio every run.
#
# Workflow: start a meeting in the UI, run this, stop the meeting when it
# prints "done", let enhance run, then scripts/eval/compare.sh the results.
#
# Wear headphones (or mute the mic): system audio is captured via
# ScreenCaptureKit regardless of output device, so the "them" channel is
# always clean, but an open mic next to speakers double-captures as "me".
#
# Usage:
#   scripts/eval/play.sh                       # default script + voices
#   scripts/eval/play.sh path/to/other.txt
#   A_VOICE="Zoe (Premium)" B_VOICE="Tom (Enhanced)" RATE=190 GAP=0.6 scripts/eval/play.sh
set -euo pipefail

SCRIPT="${1:-$(dirname "$0")/conversation.txt}"
# The built-in compact voices sound robotic. Prefer the downloadable
# Premium / Enhanced ones when installed (System Settings > Accessibility >
# Spoken Content > System Voice > Manage Voices...). Last name is the
# always-installed fallback. `say -v '?'` lists what you have.
pick_voice() { local v; for v in "$@"; do say -v '?' | grep -q "^$v  *[a-z]" && { echo "$v"; return; }; done; echo "$v"; }
A_VOICE="${A_VOICE:-$(pick_voice "Ava (Premium)" "Zoe (Premium)" "Samantha (Enhanced)" "Samantha")}"
B_VOICE="${B_VOICE:-$(pick_voice "Daniel (Enhanced)" "Tom (Enhanced)" "Daniel")}"
RATE="${RATE:-175}"              # words per minute
GAP="${GAP:-0.5}"                # seconds between turns

[ -f "$SCRIPT" ] || { echo "no script at $SCRIPT" >&2; exit 1; }
command -v say >/dev/null || { echo "macOS 'say' not found" >&2; exit 1; }

start=$(date +%s)
echo "speaking $SCRIPT (A=$A_VOICE, B=$B_VOICE, ${RATE}wpm)" >&2
while IFS= read -r line || [ -n "$line" ]; do
  case "$line" in
    ''|'#'*) continue ;;
    'PAUSE '*) sleep "${line#PAUSE }" ;;
    'A: '*) printf 'A  %s\n' "${line#A: }"; say -v "$A_VOICE" -r "$RATE" -- "${line#A: }"; sleep "$GAP" ;;
    'B: '*) printf 'B  %s\n' "${line#B: }"; say -v "$B_VOICE" -r "$RATE" -- "${line#B: }"; sleep "$GAP" ;;
    *) echo "unrecognized line: $line" >&2; exit 1 ;;
  esac
done < "$SCRIPT"
echo "done in $(( $(date +%s) - start ))s - stop the meeting now" >&2
