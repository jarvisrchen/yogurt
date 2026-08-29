# Model evals

How to feed two recordings the exact same audio, run them through different STT engines or LLMs, and get a graded comparison of the AI summaries.

The pieces live in `scripts/eval/`:

| File | Role |
|---|---|
| `conversation.txt` | A scripted two-person meeting (~5 min spoken). It is both the audio source and the ground truth the judge grades against. |
| `play.sh` | Speaks the script through the speaker with two macOS `say` voices, honoring `PAUSE` lines. |
| `compare.sh` | Pulls two meetings from SQLite, bundles them with the script, and asks headless `claude -p` for a scorecard. |

## Run one trial

1. Pick the STT engine and LLM under test in Settings (or start the backend with `YOGURT_LLM_*` env vars).
2. Put on headphones. System audio is captured through ScreenCaptureKit whatever the output device, so the "them" channel stays clean, but an open mic next to speakers would double-capture the audio as "me".
3. Start a new meeting in the UI.
4. In a terminal: `just eval-play`. It prints each line as it is spoken and ends with `done - stop the meeting now`.
5. Stop the meeting and let enhance finish.
6. Copy the meeting URL from the address bar.

Repeat with a different engine or model.
Each meeting row records what produced it: `stt_engine` is stamped at start, `llm_model` is stamped by enhance, so runs cannot be mixed up later.

## Compare two trials

```bash
just eval-compare http://localhost:7878/meeting/<id-a>/post http://localhost:7878/meeting/<id-b>/post > report.md
```

Bare ids work too.
The judge grades transcript accuracy (numbers, names, dates), summary faithfulness (hallucinations), coverage (decision, disagreement, self-correction, every action item with owner and date), and usefulness, then names the better run and the most damaging error in each.

Knobs:

- `PROMPT_ONLY=1` prints the assembled prompt instead of calling Claude, useful for pasting into another model.
- `JUDGE_MODEL=opus` is passed through to `claude --model`.
- `SCRIPT=path/to/other.txt` swaps the ground truth, for both `play.sh` (first argument) and `compare.sh`.
- `A_VOICE`, `B_VOICE`, `RATE`, `GAP` change the voices, words per minute, and the silence between turns in `play.sh`.

## Better voices

The voices that ship with macOS (Samantha, Daniel) are the compact variants and sound robotic.
Apple's Premium and Enhanced variants are free downloads that `say` can use: System Settings > Accessibility > Spoken Content > System Voice > Manage Voices..., then tick Ava (Premium), Zoe (Premium), Samantha (Enhanced), Daniel (Enhanced), Tom (Enhanced).
`play.sh` picks them up automatically once installed, in that order of preference, or set `A_VOICE="Zoe (Premium)"` explicitly.
Siri voices are not exposed to `say`.
`say -v '?'` lists what is installed.

## Writing another script

Keep the format: `A:` and `B:` lines are spoken, `PAUSE <seconds>` is silence, `#` is a comment.
Spell numbers and dates out the way a person would say them, because that is what the STT hears and what the judge checks against.
Include things worth grading: at least one decision, one disagreement, one correction, and action items with owners and dates.
