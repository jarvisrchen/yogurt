#!/usr/bin/env bash
# tail-transcript — dump a meeting's PERSISTED transcript to a file, on a
# loop, so you can keep it open in an editor and watch it grow.
#
# Why this exists: the transcript dock renders the live WS stream (partials
# included, merged client-side by `mergeEvent`), while `transcript_json` in
# SQLite only ever holds FINAL segments appended by `persist_transcript`.
# When the dock looks wrong, this is how you tell "the UI mis-rendered" from
# "we persisted the wrong thing".
#
# Usage:
#   scripts/tail-transcript.sh                  # newest meeting -> /tmp/yogurt-transcript.json
#   scripts/tail-transcript.sh <meeting-id>
#   OUT=~/t.json INTERVAL=1 scripts/tail-transcript.sh
set -euo pipefail

DB="${YOGURT_DB:-$HOME/.yogurt/db.sqlite}"
OUT="${OUT:-/tmp/yogurt-transcript.json}"
INTERVAL="${INTERVAL:-2}"
ID="${1:-}"

[ -f "$DB" ] || { echo "no db at $DB" >&2; exit 1; }
if [ -z "$ID" ]; then
  ID=$(sqlite3 "$DB" "select id from meetings order by started_at desc limit 1;")
  [ -n "$ID" ] || { echo "no meetings in $DB" >&2; exit 1; }
fi

echo "meeting $ID -> $OUT (every ${INTERVAL}s, ctrl-c to stop)" >&2
while true; do
  # Write to a temp file then mv: editors that reload on change never see a
  # half-written file. json_each flattens the array to one segment per line.
  sqlite3 "$DB" "
    select json_extract(value, '\$.ts_ms') || '  ' ||
           json_extract(value, '\$.channel') || '  ' ||
           json_extract(value, '\$.text')
    from meetings, json_each(meetings.transcript_json)
    where meetings.id = '$ID';" > "$OUT.tmp" 2>/dev/null || true
  mv "$OUT.tmp" "$OUT"
  sleep "$INTERVAL"
done
