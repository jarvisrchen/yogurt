# Agent workflow ship log

Orchestrated overnight run, 2026-09-01 23:40 to 2026-09-02 02:05 PDT.
Source of truth for the plan: [agent-workflow.md](agent-workflow.md) section 6 (ship order).
This file is the record of what shipped, what was found on the way, and what is left for Richard.

## Result

All 14 units of the ship order landed as 17 squash-merged PRs (#52 to #68), then this log and one fix it surfaced (#69, #70), in about two and a half hours, plus the two by-hand items.
Every PR was built by a Sonnet implementer in its own worktree, reviewed by a fresh Sonnet reviewer that ran the code, fixed until clean, then merged by the orchestrator after CI.
Nothing was released: the binary changes (`yogurt ctl`, health fields, fixtures, `--data-dir`) sit on `main` and go out with the next tag.

| Ticket | PR | What shipped |
| --- | --- | --- |
| DX-2 | #52, #54 | `docs/TODO-DONE.md` (TODO.md 84 KB to 20 KB); `just ticket` list, show, next, done, `--check` in `just lint` |
| DX-6 | #53 | one release procedure; `docs/RELEASE-LOG.md`; stale runbooks archived; exact-version formula assert in `release.yml` |
| CLI-4 | #55 | `yogurt ctl` status, meeting new/start/stop/show/summary/transcript/enhance, detect, windows; `/api/health` gains version and mode; `just dev` prints `YOGURT_PORT` |
| DX-7 | #56, #61, #64 | `scripts/release.sh preflight`, `verify`, `finish`, `untag`, `ship`; `scripts/lib/docs-only.sh`; release skill shrunk to three commands |
| DX-4 | #57 | CI calls `just`; `lint-web`; Playwright in `just test`; `scripts/check-docs.sh` with a `docs.yml` job; control-skill dedupe; `--help` drift test |
| DX-3 | #58 | `just start`, `just worktrees`, `just dev-bg`, `just dev-stop`; `scripts/task.sh` |
| CLI-7 | #59 | `yogurt start --data-dir` and `YOGURT_DATA_DIR`, honored by doctor and the ctl db fallback |
| CLI-5 | #60 | fixture meetings: `POST /api/meetings` accepts `transcript_json` and `ended`; `ctl meeting new --transcript-file` and `--from-script` |
| DX-8 | #62 | `docs/FEATURES.md` (19 rows) with a route coverage rule in check-docs |
| DX-5 | #63 | `just pr` and `just land` (`scripts/ship.sh`); tracked git hooks in `.githooks/` |
| DX-10 | #65 | `scripts/check-published.sh` and a weekly `check-published.yml` that opens an issue on drift |
| DX-1 | #66 | real-binary smoke suite in CI (23 tests) and `just test-hw` for the capture pipeline |
| DX-9 | #68 | AGENTS.md rewritten around the six-command lifecycle (1558 to 608 words); rationale moved to CONTRIBUTING.md |
| CLI-6 | #67 | `ctl settings`, `provider`, `models`, `ws`, `meeting mute`, `search`, `delete`; ticket stays open for the skill rewrite (see below) |
| B4 | none | GitHub settings were already applied on 2026-09-01; recorded in RELEASING.md by #53 |
| E4 | none | memory notes pruned outside the repo: plans inventory deleted, four notes now point at the new commands |

## What the reviews caught

These are the things that would have shipped without the second pass.

- `ctl ws` and `models download --wait` printed the websocket URL, token included, on any connect failure (#67). Fixed with the server's own redaction helper and a regression test that asserts the test token never appears in stdout or stderr.
- `just start` validated the ticket against the shared main checkout's files, which are routinely behind `origin/main` (#58). It now reads the ticket from `origin/main` through a temp `TICKET_DOCS_DIR`.
- `just land` folded a real `git status` failure into "clean" and would have removed a broken worktree (#63).
- `release.sh` read release tags from the local clone; CI's shallow checkout has none, and the same shape bit the docs-only test (#56, #61). Both now read `git ls-remote`.
- `release.sh ship` left an orphaned bump worktree on every failed retry (#64); `verify` leaked its temp dir on a failing check (#61).
- AGENTS.md's rewrite dropped the "do not generalize the subprocess exception" clause and the Lavish-surfaces-go-in-docs steer (#68); both restored.
- Em dashes in new script comments would have failed the check-docs gate the same night it landed (#56).
- Landing this log with `just land` found one more: GitHub deletes the remote branch on merge asynchronously, so the script's own remote delete lost the race and exited 1 after all the useful work (#69). Fixed in #70, which was landed with `just land` itself.

## Findings worth knowing

- The hardware smoke test hung twice on this Mac before passing three times straight. Root cause was not TCC: a force-killed test run had left a stale ScreenCaptureKit/mic session that blocked the next capture start. The test is now wall-clock bounded and the underlying behavior is filed as **AUD-8**.
- `just dev`'s trap did not catch HUP, so `tmux kill-window` leaked the backend and Vite (fixed in #58). One such orphan from before the fix was found on :7878 at the end of the run and killed; nothing else was left listening.
- `verify` against a release older than the latest fails its formula checks by design: the tap's default branch only carries the latest formula and old PR branches are gone.
- `ctl settings set` with an unknown key is a silent no-op because the server ignores unknown fields. Pre-existing; not fixed.
- `ctl ws --count 1` against an idle server waits forever for a frame; no timeout flag was specified.

## Decisions for Richard

Update 2026-09-02 09:10 PDT: Richard asked for both remaining items to be done, so CLI-6's skill rewrite landed as #72 and v0.8.0 was cut with `scripts/release.sh ship 0.8.0` (bump PR #73, tag at the merge sha, tap PR #9, brew upgraded and tested). The first real ship needed three resumes and a hand patch; the bugs are listed in the v0.8.0 row of `docs/RELEASE-LOG.md` and fixed in a follow-up PR. Items 1 and 2 below are therefore done.

1. **Cut v0.8.0.** It is the first release carrying `yogurt ctl`, `--data-dir` and the fixture endpoint, and the first real run of `scripts/release.sh ship`. The skill is three commands now: `scripts/release.sh preflight 0.8.0`, act on its judgment items, `scripts/release.sh ship 0.8.0`, paste the printed row into `docs/RELEASE-LOG.md` with the narrative. `ship -n --allow-open-docs` was run against the live repo and printed a correct plan; the real thing was left for you.
2. **CLI-6's skill rewrite** (about 150 words around the generated command block) is the only open scope from the plan, and by the ticket's own rule it waits for that release: the README's `npx skills add` path installs the skill standalone, so it must not name commands the brew binary lacks.
3. The `llm5-todo-done` worktree (PR #27, merged) is still on disk; `just worktrees` lists it as removable. It predates this run and was left alone.
4. The shared main checkout is 20 commits behind: `cd /Users/rchen/Documents/code/yogurt && git pull --ff-only`. Nothing in the run touched it except this file.

## Morning checklist

```
cd /Users/rchen/Documents/code/yogurt && git pull --ff-only
just worktrees                       # one row per worktree, ports, dirty, removable
just ticket                          # 8 open tickets, CLI-6 narrowed to the skill rewrite
just start CLI-6 skill               # the whole lifecycle in one command, when ready
scripts/release.sh preflight 0.8.0   # read-only; the judgment items for the release
```

## How the run worked

- One worktree per ticket under `~/Documents/code/yogurt-worktrees/<slug>`, created from `origin/main`.
- A Sonnet implementer subagent built the ticket from the proposal text, ran the gates, checked the ticket off, and opened the PR.
- A fresh Sonnet reviewer subagent checked spec compliance and code quality, running the scripts and tests itself; findings went back to the implementer until clean.
- The orchestrator waited for CI, squash-merged with an explicit subject and body, removed the worktree, and deleted the branch.
- Up to five lanes ran in parallel; every `docs/TODO-DONE.md` append conflicted with its neighbor and was rebased by keeping both blocks.

## Timeline (merge times, PDT)

- 23:40 run started; repo surveyed; no server on :7878, no tmux session.
- 23:53 #52 DX-2 docs split.
- 00:01 #53 DX-6.
- 00:08 #54 DX-2 `just ticket`.
- 00:27 #56 DX-7 preflight.
- 00:29 #55 CLI-4.
- 00:41 #57 DX-4.
- 00:43 #59 CLI-7.
- 00:49 #58 DX-3.
- 00:59 #60 CLI-5.
- 01:06 #61 DX-7 verify, finish, untag.
- 01:09 #62 DX-8.
- 01:29 #63 DX-5.
- 01:37 #64 DX-7 ship.
- 01:43 #65 DX-10.
- 01:51 #66 DX-1.
- 01:57 #68 DX-9.
- 02:00 #67 CLI-6 second slice. Orphaned dev server from a removed worktree killed; ports clear; no open PRs.
- 02:10 #69 this log, landed with `just pr` and `just land`; the land race surfaced.
- 02:25 #70 the land race fix, landed with `just land`; main CI green on a4ffa37.
