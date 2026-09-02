#!/usr/bin/env bash
# compare.sh - judge two enhanced meetings that were fed the same audio.
#
# Pulls both meetings straight from SQLite (no server needed), bundles them
# with the ground-truth script, and asks headless Claude to grade transcript
# accuracy, summary faithfulness, and action-item recall. Report goes to
# stdout - redirect it to keep it.
#
# Usage:
#   scripts/eval/compare.sh <meeting-url-or-id> <meeting-url-or-id> [> report.md]
#   PROMPT_ONLY=1 scripts/eval/compare.sh A B      # print the prompt, skip the judge
#   JUDGE_MODEL=opus scripts/eval/compare.sh A B   # passed to `claude --model`
#   SCRIPT=other.txt scripts/eval/compare.sh A B   # different ground truth
set -euo pipefail

[ $# -eq 2 ] || { echo "usage: $0 <meeting-url-or-id> <meeting-url-or-id>" >&2; exit 1; }
DB="${YOGURT_DB:-$HOME/.yogurt/db.sqlite}"
SCRIPT="${SCRIPT:-$(dirname "$0")/conversation.txt}"
[ -f "$DB" ] || { echo "no db at $DB" >&2; exit 1; }
[ -f "$SCRIPT" ] || { echo "no script at $SCRIPT" >&2; exit 1; }

# http://localhost:7878/meeting/<id>/post  ->  <id>
meeting_id() { local s="${1##*/meeting/}"; echo "${s%%/*}"; }

# One meeting -> markdown section. Transcript is flattened to one line per
# persisted segment (finals only, same as `yogurt ctl meeting transcript`).
section() {
  local label="$1" id="$2"
  local n; n=$(sqlite3 "$DB" "select count(*) from meetings where id='$id';")
  [ "$n" = 1 ] || { echo "meeting $id not found in $DB" >&2; exit 1; }
  echo "## Meeting $label ($id)"
  echo
  sqlite3 "$DB" "select '- title: ' || title || char(10) ||
                        '- stt_engine: ' || coalesce(stt_engine, 'unknown') || char(10) ||
                        '- llm_model: '  || coalesce(llm_model,  'unknown') || char(10) ||
                        '- duration_s: ' || coalesce((ended_at - started_at) / 1000, 'n/a')
                 from meetings where id='$id';"
  echo
  echo "### Transcript $label"
  echo '```'
  sqlite3 "$DB" "select json_extract(value,'\$.channel') || ': ' || json_extract(value,'\$.text')
                 from meetings, json_each(meetings.transcript_json) where meetings.id='$id';"
  echo '```'
  echo
  echo "### AI summary $label"
  echo
  # Strip the inline <span data-ts> transcript-link markup so the judge
  # reads prose, not HTML.
  sqlite3 "$DB" "select coalesce(enriched_md, '(not enhanced)') from meetings where id='$id';" \
    | sed -E 's/<[^>]+>//g; s/ *↳ [0-9:]+//g'
  echo
}

A=$(meeting_id "$1"); B=$(meeting_id "$2")
PROMPT=$(mktemp -t yogurt-compare); trap 'rm -f "$PROMPT"' EXIT
{
cat <<'HDR'
You are grading two runs of a meeting copilot. Both meetings were fed the exact same audio: a scripted two-person conversation, given below as the ground truth. Each run may have used a different speech-to-text engine and/or a different LLM for the summary; the metadata says which.

Grade each run on a 1-5 scale, with evidence quoted from the run, on:

1. Transcript accuracy - words, numbers, names, and dates versus the ground truth. Call out every wrong number or name; those matter most.
2. Summary faithfulness - does the AI summary say only things the conversation actually said? List every hallucination or misattribution.
3. Coverage - the decision, the disagreement and how it was resolved, the self-correction, and every action item with owner and date. Table: item, in A?, in B?
4. Usefulness - would a person who missed the meeting be correctly briefed? Structure, concision, no filler or tangents (the espresso machine is a tangent and should not be a headline).

End with a scorecard table (rows = criteria, columns = A and B), a one-paragraph verdict naming the better run and why, and the single most damaging error in each run.

# Ground truth script

```
HDR
grep -vE '^\s*(#|$)' "$SCRIPT"
echo '```'
echo
section A "$A"
section B "$B"
} > "$PROMPT"

if [ "${PROMPT_ONLY:-0}" = 1 ]; then cat "$PROMPT"; exit 0; fi
command -v claude >/dev/null || { echo "claude CLI not found; use PROMPT_ONLY=1" >&2; exit 1; }
echo "judging $A vs $B with claude${JUDGE_MODEL:+ ($JUDGE_MODEL)}..." >&2
claude -p --output-format text ${JUDGE_MODEL:+--model "$JUDGE_MODEL"} < "$PROMPT"
