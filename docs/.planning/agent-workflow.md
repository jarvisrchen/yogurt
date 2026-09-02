# Agent workflow: applying pstack to yogurt

Research doc, 2026-09-01.
Companion review surface: [../.lavish/agent-workflow.html](../.lavish/agent-workflow.html).

Richard read Part 1 of lauren (poteto)'s pstack guide and asked what in it applies here.
The specific ask: what can be scripted between "work on ticket X" and "PR open", at PR completion (merge, cleanup), and for releases, so that each is more repeatable and costs fewer tokens.
This doc answers that with measured numbers from this repo, a set of proposals that survived adversarial review, an explicit do-not-build list, a ship order with ticket IDs, and the answers to the questions it raised, all but one of them defaults.

How it was produced: seven research passes over the repo (one per lifecycle or principle), each proposal then reviewed by an independent agent told to refute it against the real files, then a completeness pass against the guide.
Claims below cite the file they were checked against.

## 1. What pstack says, and what it means here

The guide's ideas, against what yogurt has today.

| pstack principle | yogurt today | Verdict |
| --- | --- | --- |
| A verification skill is critical infrastructure: the agent closes the loop itself | `.claude/skills/yogurt-control/SKILL.md` teaches curl recipes; every macOS-facing check is a human clicking. MTG-11 shipped with a fabricated window-title verification (`docs/RELEASING.md`, v0.7.0 row) | The central gap. CLI-4 and DX-1 already name it; this doc makes them concrete (section 4D) |
| Build the Lever: tools over markdown. A small agent-friendly CLI with `--dry-run`, subcommands, descriptive errors, machine-readable output | `yogurt doctor --json` is the only machine-readable surface. Everything else is prose in AGENTS.md, two skills, and three copies of the release procedure | Applies to all three lifecycles. Most proposals below are "replace a paragraph with a command" |
| Invest in the dev utility: seeding data, test users, a consistent environment | `just bootstrap` and `just dev` are good. Nothing creates the worktree, finds the ticket, opens or lands the PR. Nothing can seed a meeting with a known transcript: `test_support::seed_meeting` only compiles under `cargo test`, and the only scripted conversation is `scripts/eval/play.sh`, which speaks it through the speaker for minutes | Sections 4A, 4B, and the fixture loader in 4D |
| Cloud agents over worktrees for parallelism | The product needs ScreenCaptureKit, TCC grants, real audio and a Metal whisper build. The workspace does not compile on Linux (`yogurt-server/Cargo.toml` pins `local-stt`, which pulls whisper's `metal` feature). 6 of the last 9 feature PRs could only be truthfully verified on Richard's Mac | Deliberate non-fit for Rust work. GitHub's free macos-26 runner is already the cloud verifier, about 2 minutes warm. Section 4F |
| Feature Map as materialized memory | `docs/TODO.md`'s 64 KB DONE section is the de facto feature inventory, at 16k tokens a read | Build a one-table map, but only once `yogurt ctl` exists to name in its last column, and guard it with a check so it cannot rot. Section 4D |
| A daily maintenance loop so the CLI and map never drift | No drift check of any kind. AGENTS.md describes `just lint` and `just build` wrongly today; the control skill states a CLI gap that does not exist | Do not schedule an LLM audit. Every drift in this repo's history is a string-existence failure a 40-line grep catches on the PR that introduces it. Section 4E |

The guide also says worktrees "use a lot of storage" and are slow.
Measured here on 2026-09-01: `just bootstrap` in a fresh worktree took 8 seconds and a cold debug `cargo build -p yogurt` took 35 seconds, compiling all 290 crates including whisper.cpp, into a 1.7 GB `target/`.
The 8-to-15-minute figure in `scripts/setup.sh` is the release profile (`lto`, `codegen-units = 1`), which the dev loop never uses.
Worktree cost is not the problem here; the missing scripts around worktrees are.

## 2. The three lifecycles today, measured

Numbers come from the repo on 2026-09-01: 50 merged PRs in the three days since 2026-08-30, seven releases, `gh api` for repo settings, and `wc` on the docs.

### 2.1 Task start

What an agent does between "work on MTG-10" and the first edit, following AGENTS.md lines 52-61 and CONTRIBUTING.md:

| Step | Today | Cost |
| --- | --- | --- |
| Fetch | `git fetch origin --prune`, not written down but visible in the reflog | 1 call |
| Worktree | `git worktree add ../yogurt-worktrees/<slug> -b <branch> origin/main`, slug and branch invented each time (four naming styles across 50 PRs) | 1 call, one naming decision |
| Bootstrap | `just dev` runs `just bootstrap` (copies `.env.local` from the main checkout, `pnpm install`, web build) | already scripted |
| Find the ticket | grep and sed into `docs/TODO.md`, or a full read | up to 16k tokens; the file is 75.7 KB and 85% of it is DONE |
| Run | `just dev` blocks the shell, so agents wrap it in tmux from memory, poll `/api/health`, then `tmux capture-pane` to learn which port pair it picked | 3-5 calls, 2-4k tokens |
| Prose read to get this right | AGENTS.md worktree paragraphs (about 600 words) plus the CONTRIBUTING worktree section | every session |

Evidence that prose does not hold: the `llm5-todo-done` worktree is still on disk although its PR #27 merged on 2026-09-01, and the cleanup rule at AGENTS.md line 61 has been followed zero times out of 23 merges (23 merged branches still exist on origin; `delete_branch_on_merge` is false).

### 2.2 Task end

| Step | Today | Cost or failure |
| --- | --- | --- |
| Gate | `just lint`, `just test` | `just test` omits Playwright, so it is weaker than CI (DX-1). AGENTS.md line 25 says `just lint` runs the web typecheck; the recipe does not |
| Commit | conventional message with the ticket ID, no agent trailers | enforced by prose only; no git hooks exist |
| PR | `gh pr create` with a hand-written body | bodies are inconsistent; six sampled PRs use five different heading schemes, and PR #46's manual-test line used the relative path AGENTS.md forbids |
| CI wait | `gh pr checks --watch` | exits 1 with "no checks reported" on docs-only PRs, because `ci.yml` `paths-ignore` skips them entirely |
| Merge | `gh pr merge --squash --delete-branch` | `--delete-branch` fails when run from inside the worktree (the branch is checked out there); rebase and merge-commit are still enabled on the repo, so the squash-only rule is one click from being broken |
| TODO checkoff | move the block to DONE, in the same PR | 17 of 18 ticketed PRs did it; #14 and #27 were repair PRs for the two that forgot |
| Cleanup | `git worktree remove`, `git branch -D`, delete the remote branch | 0 of 23 |
| Handover | absolute `cd ... && just dev` plus clicks | re-derived from AGENTS.md line 59 each time |

### 2.3 Release

The release skill is 11 steps and 846 words.
Every release so far has also needed two extra PRs: a version bump (Cargo.toml plus `cargo update --workspace`) and a release-log row.
Seven releases in three days means 14 mechanical PRs whose bodies were rewritten from scratch each time.

Classifying the 11 steps:

| Step | Kind |
| --- | --- |
| 1 clean tree, CI green on the sha | mechanics, but impossible to satisfy literally when `main`'s HEAD is a docs-only commit that CI skipped |
| 2 version matches tag; bump with `cargo update --workspace` | mechanics plus one hidden PR |
| 3 frozen lockfile | redundant: both CI jobs already run `--frozen-lockfile` on the sha |
| 4 README wording; merge open doc PRs first | judgment, with a mechanical list (the jq is already in the skill) |
| 5 dry run and watch | mechanics, 3 minutes |
| 6 tag and push | mechanics; the v0.3.0 "main moved" lesson is a human-memory rule |
| 7 watch four jobs | mechanics, 4 minutes |
| 8 re-download tarballs, hash, compare to the tap PR | mechanics, the step the skill says not to trust the pipeline on |
| 9 merge the tap PR | mechanics |
| 10 brew smoke test | mechanics; the skill's untap-first order fails on this machine because `yogurt-model-tiny-en` depends on `yogurt` (v0.5.0 and v0.6.0 rows), so upgrade-in-place has been the real path for three releases |
| 11 log row | facts are mechanics (run ids, shas, scope); the narrative caveat is judgment and is the row's whole value |

The procedure exists in three copies that disagree: the skill, `docs/RELEASING.md` "Cutting a release" (7 steps, different order, the opposite untap order), and `scripts/release-checklist.md`, which still says the repo is private and no tag has been cut.
`CONTRIBUTING.md` carries a short summary with a pointer, which is fine.
`scripts/homebrew/` (a placeholder formula and a README that calls the tap bootstrap "not yet done") is dead: `release.yml` generates the formula inline and never reads it.

### 2.4 What every session pays for

| File | Words | Tokens (approx) | When paid |
| --- | --- | --- | --- |
| AGENTS.md | 1,415 | 1.8k | every session, via `CLAUDE.md` |
| docs/TODO.md | 11,047 | 15-19k | every task that reads the backlog; 64 KB of it is DONE |
| docs/RELEASING.md | 2,409 | 3.1k | every release; the log table is 9.5 KB and grows 2 KB per release |
| release skill | 846 | 1.1k | every release |
| yogurt-control skill | 714 | 0.9k | every meeting read or control request; duplicates docs/AI-INTEGRATION.md |
| docs/ARCHITECTURE.md | 8,138 | 10.6k | structural changes only; the right price, leave it |

AGENTS.md line by line: about a third is hard constraints and style that must stay prose.
About a third is procedure a script replaces with one line (worktree add, handover, cleanup, the command table).
The rest is rationale for failure modes that a tool could make impossible (shared-checkout builds, spliced binaries, relative paths), which belongs in CONTRIBUTING.md next to the mechanism.

## 3. The shape of the answer

Three scripts and one CLI subcommand absorb the mechanics; the skills shrink to naming them; AGENTS.md shrinks to constraints plus a six-command lifecycle.

```
ticket -> just start MTG-10 flash     (fetch, worktree, bootstrap, ticket text, handover line)
       -> just dev-bg                 (server in tmux, prints the port pair)
       -> edit; verify with yogurt ctl ... and just test
       -> just ticket done MTG-10 --note-file note.md
       -> just pr "MTG-10: ..." --body-file body.md
       -> just land                   (CI wait, squash, worktree + branch cleanup, handover)

release -> scripts/release.sh preflight 0.8.0     (read-only; lists the judgment calls)
        -> scripts/release.sh ship 0.8.0          (bump PR, dry run, tag by merge sha, watch, verify, finish)
```

Everything that is judgment stays with the agent or with Richard: what to build, whether a doc PR describes something installable at this tag, the release narrative, the manual-test clicks, and whether to merge at all.

## 4. Proposals

Each proposal lists what it absorbs, what judgment remains, an honest token estimate, the main risk, and effort (S under half a day, M a day, L more).
Priorities: **now** is worth doing this week, **next** after the "now" items land, **later** after a precondition is met.
The reviewers' amendments are folded in; where a reviewer re-rated effort or priority, the re-rating is what appears.

### 4A. Task start

**A1. `just start <ID|slug> [words]`** (now, S, after A2).
`scripts/task.sh` behind a `start` recipe; a standalone script rather than an inline recipe because of the exit-code branching.
Resolves the main checkout with the same `git-common-dir` trick `just bootstrap` uses, fetches origin with prune, requires the ID to be an open ticket (exit 2 otherwise, naming `just ticket`), derives one name from the lowercase ID plus optional words (`mtg-10-flash`) and uses it for both the worktree directory and the branch.
If both the directory and the branch already exist under that name, it resumes: re-runs `just bootstrap` (which is not atomic, so a network blip mid-install must be recoverable) and reprints the handover.
It refuses only when the name is claimed by something else, and it checks `refs/heads/<name>` as well as the directory, because a branch outlives its worktree whenever the cleanup step was skipped.
Then `git worktree add ../yogurt-worktrees/<name> -b <name> origin/main`, `just bootstrap` there, the ticket block, the absolute handover line `cd /Users/rchen/Documents/code/yogurt-worktrees/<name> && just dev`, and the path alone on the last line.
It does not build and does not start a server; those are separate decisions.
A free-form slug (no ticket) is allowed for docs and release tasks.
Keep the `../yogurt-worktrees` convention: moving to `.claude/worktrees/` would put multi-GB `target/` directories inside the shared checkout and break the handover string.
A session that can should call `EnterWorktree` with the printed path; a pinned subagent cannot enter paths outside `.claude/worktrees/`, so subagents keep using absolute paths.
Absorbs AGENTS.md line 52 and the naming decision.
Judgment left: the slug words; whether to run the server.
Tokens: 3-5k per task from collapsing four or five round trips, and no more edits landing in the shared checkout by accident.

**A2. Split DONE out of `docs/TODO.md`, add `just ticket`** (now, S, no dependencies).
Two PRs: the docs-only split first (CI skips it), then the script.
Move everything from `## DONE` into `docs/TODO-DONE.md` as a flat list: no outer `<details>` wrapper, no `###` subsections (the DONE section already has `### Meetings` twice, so the subsections are not maintained).
Keep it in `docs/`, not `docs/archive/`, because ID allocation still counts it and open tickets cite DONE outcomes; add one sentence to AGENTS.md's repo-layout bullet saying so, so a future session does not helpfully archive it.
`scripts/ticket.sh` behind a `ticket` recipe: no arguments lists open items as `ID<TAB>title`; `<ID>` prints one block; `next <PREFIX>` prints the next free number across both files; `done <ID> --note-file <path>` flips the checkbox, moves the block, and appends the note paragraph, which is how every real checkoff looks (`Landed in #N ...`).
The note is a file, never a positional argument: real resolution notes contain backticks, dollar signs and JSON literals, which do not survive shell quoting.
The block boundary is the next line starting with `- [` or `#`, never `</details>`: UI-6 and UI-7 have no details block, and five DONE entries carry their resolution text after the closing tag.
The scanner must skip fenced code, because the example at TODO.md line 33 uses the real ID `UI-5`; change the example to `UI-0` in the same PR.
BSD awk only (macOS ships `awk 20200816`, no gawk).
One runnable check, `scripts/ticket.sh --check`, wired into `just lint` (not `just test`, whose comment says "what CI runs"): IDs unique across both files, every `- [` line carries an ID, every block non-empty, `next` equals max plus one.
Update the allocation rule at TODO.md line 18 to name `just ticket next` and the checkoff instructions at lines 41-43 to name `just ticket done`.
Absorbs both rules and the 16k-token read.
Tokens: the open section is about 5 KB (1.3k tokens) against 75.7 KB (15-19k) for a full read, and one block is 150-600 tokens; the "400k per three days" figure some reviewers quoted is an upper bound that assumes whole-file reads rather than greps.

**A3. `just worktrees`** (next, S).
One row per worktree from `git worktree list --porcelain` (works from any worktree, no cd to main needed): path, branch, PR number and state (one `gh pr list --state all -L 100` call, with a per-branch `--head` fallback for anything older than the window, time-boxed to a few seconds and skippable with `--no-pr`), listening ports whose process cwd is that worktree (`lsof`, reusing `_ypg_holder_pid` from `port-guard.sh`; the multi-record parse is new and needs testing against a real two-worktree session), and `dirty` when `git status --porcelain` is non-empty.
`dirty` is in v1, not deferred: a worktree with uncommitted work marked removable because its PR merged is the silent-data-loss case.
Rows with a merged PR and a clean tree are marked removable.
Foreign worktrees such as `~/.treehouse/...` are listed, never touched.
Absorbs the "who owns :7878" investigation that precedes every port decision and is the prerequisite for `just land`'s cleanup.

**A4. `just dev-bg [name]`** (next, S).
Do not re-implement port resolution: open `tmux new-window -t yogurt -n <sanitized worktree name> -c <path> 'just dev'`, then poll `tmux capture-pane` for the port lines `just dev` already prints (and the `YOGURT_PORT=` line from D5), then poll `/api/health` on that port, then print `backend=http://localhost:PORT vite=PORT tmux=yogurt:<name>`.
On timeout, print the pane tail and the literal `tmux attach -t yogurt:<name>` command.
`just dev-stop [name]` kills the window and exits 0 if it is already gone.
Absorbs the tmux incantation that today lives only in agent memory and the three or four polling calls.
Makes tmux a documented dev dependency in CONTRIBUTING.md; it is installed (3.5a) and is Richard's stated preference.

**A5. Build cache across worktrees** (do not build).
Measured cold build is 35 seconds.
A shared `CARGO_TARGET_DIR` would serialize builds on cargo's directory lock and make `target/debug/yogurt` "whichever worktree linked last", which is the no-provenance binary AGENTS.md lines 53-55 exist to prevent.
Revisit only if the cold build ever exceeds a few minutes; the safe option then is an APFS clone (`cp -c`) of `target/debug` as a `--warm` flag on `just start`, with a runtime check that the cloned build-script outputs resolve inside the clone.

### 4B. Task end

**B1. `just pr <title> --body-file f [--draft] [--dry-run]`** (now, S).
`scripts/ship.sh pr`.
Refuses, with the fix in the message, when: not on a non-main branch in a linked worktree; the title matches neither `^[A-Z]{2,4}-[0-9]+: ` nor `^(docs|chore|ci|fix|feat|test|build)(\(.+\))?: ` (the two shapes cover all 50 merged titles); the body contains `Generated with`, `Co-Authored-By`, or an em dash outside code spans; the diff touches `crates/`, `web/src`, `justfile` or `scripts/` and the body has no line matching `cd (/Users/[^ ]+|~)/.*yogurt-worktrees/[^ ]+ (&&|;) just dev`; or the title's ticket ID is not `- [x]` in `docs/TODO-DONE.md` on the current branch.
The handover check is a content pattern, not a heading: six sampled PRs use five heading schemes, and #46 shipped the relative form one PR after #44 fixed the rule in prose, which a heading check would have missed.
The checkoff check reads the branch's tree, not `origin/main`, because the checkoff rides in the same PR.
Then pushes, creates the PR, prints the URL and `next: just land`.
It validates and never generates: the narrative and the clicks are the agent's.
A `.github/pull_request_template.md` cannot do this because `gh pr create` ignores it whenever `--body` is passed, which is always.
Tokens: near zero per PR; it prevents the repair PRs (#14, #27, #44), each a full agent turn.

**B2. `just land [pr] [--dry-run]`** (now, M, same PR as B1).
`scripts/ship.sh land`, sharing the docs-only and bookkeeping checks with `pr` through one function so they cannot drift.
Resolves the PR from the current branch, or from the argument, in which case the branch comes from `gh pr view --json headRefName` and the worktree is found by that branch in `git worktree list --porcelain` (if none matches, branch cleanup still runs and worktree removal is skipped with a message).
If already merged, skips to cleanup.
Preflight: clean tree, HEAD pushed, ticket moved to DONE if the title carries an ID.
CI: if every changed path matches `ci.yml`'s docs-only globs, print `ci skipped (docs-only)`; otherwise `gh pr checks --watch --fail-fast` with a cap of a few minutes and a "run `just land` again to resume" message, and on red print the failing check names and `gh run view --log-failed`, exit 1, no override flag.
Merge: `gh pr merge --squash --match-head-commit <sha> --subject "<PR title>" --body "<PR body minus the manual-test section>"`, never `--delete-branch`.
Setting the message explicitly means the squash commit on `main` carries the validated narrative whatever `squash_merge_commit_message` is set to (today it is `COMMIT_MESSAGES`, the concatenated branch commits), and the handover lines, whose worktree path is gone after cleanup, stay out of history.
Cleanup, from the main checkout: `git worktree remove` (no `--force`; a dirty tree is listed and the script refuses, and this path gets a test), `git branch -D`, `git push origin --delete` only if the ref still exists (after B4 it will not), `git fetch origin`.
Never pulls the shared checkout; prints the pull command for Richard instead.
Every step is skip-if-done so a re-run resumes.
Ends by re-printing the PR body's handover rewritten for `main`, and `cwd is gone: cd /Users/rchen/Documents/code/yogurt`.
Absorbs AGENTS.md lines 59-61 and the merge/worktree/branch ordering git actually permits.
Judgment left: whether to run it.
`just pr` never merges and `just land` always does, so the task prompt chooses; 13 of the last 20 PRs merged within three minutes of opening, three waited hours for Richard.
Tokens: 1.5-2.5k per task; the real win is cleanup going from 0 of 23 to every time.

**B3. Tracked git hooks** (next, S).
`.githooks/commit-msg` rejects `Co-Authored-By` naming an agent (claude, anthropic, codex, cursor, opencode, copilot), `Generated with`, and em dashes.
`.githooks/pre-commit` refuses when the branch is `main`, printing the worktree-add line from AGENTS.md (switch the message to `just start` once A1 exists).
`just bootstrap` and `scripts/setup.sh` both run `git config core.hooksPath .githooks`; the config lives in the common git dir, so it covers every worktree, and it covers cursor-agent and opencode because they use the same git binary.
Commit the hook files with the executable bit (`git ls-files -s` shows `100755`); a hook without it silently never runs.
`--no-verify` stays the escape hatch.
PR bodies are not covered by a git hook; B1 covers those.
Richard's global Claude Code settings already set `attribution` to empty, so the hook is the backstop for other agents, not a duplicate.
Absorbs AGENTS.md line 50's enforcement half and line 57.

**B4. GitHub settings, one time** (now, minutes, Richard's call).
`gh api -X PATCH repos/jarvisrchen/yogurt -F allow_rebase_merge=false -F allow_merge_commit=false -F delete_branch_on_merge=true`.
After this the only merge button is Squash and the branch is deleted on merge, which makes the remote half of `land`'s cleanup a no-op to verify rather than an action.
No ruleset on `main` (decision 1): Richard is the sole collaborator on a public repo, so nobody else can push there today, and a PR requirement would only constrain him.
The "never commit or push to `main`" convention stays prose for humans and becomes the pre-commit hook (B3) for agents.
No change to `squash_merge_commit_message` either: `land` sets the squash commit's subject and body itself (see B2), so the repo default is irrelevant.
Record the command in `docs/RELEASING.md` "One-time prerequisites" and shrink AGENTS.md line 51 to one fact.
Absorbs the squash-merge paragraph including the `1656270` story.

**B5. Shared-checkout build guard** (do not build; decision 11).
The design was sound: `scripts/lib/tree-guard.sh` refusing `just build` and `just dev` from the main checkout while other worktrees exist unless `YOGURT_OWN_MAIN=1` is set for that one invocation (never a persistent export, which every agent spawned from that shell would inherit).
It is not worth its cost.
Once `just start` exists an agent is never in the main checkout to begin with, the pre-commit hook (B3) stops the commit half of the failure, and the guard's remaining friction lands on Richard, who is the one who runs from `main`.
Move the build-splice rationale from AGENTS.md lines 54-56 into CONTRIBUTING.md's worktree section as prose, and revisit the guard only if an agent builds in the shared checkout after DX-3 lands.

### 4C. Release

All of these live in one `scripts/release.sh` with subcommands, bash plus `gh` and `jq`, in the style of `scripts/publish-model-mirror.sh`.
No `just` recipe, because `just release` already means "run the release binary".
Flags only, no prompts; `-n` prints the plan on anything that mutates; `--json` emits a per-check array where there are checks.
The three docs-only-PR detectors this doc implies (preflight, ship, and `ship.sh`'s gate) must be one shared `is_docs_only` helper in `scripts/lib/` derived from `ci.yml`'s `paths-ignore` list.

**C1. `release.sh preflight <version>`** (now, M, first PR of the script).
Read-only, works from any worktree, reads `origin/main` after a fetch.
Checks: gh and jq present (not brew, which only `finish` needs); the tag does not exist on origin via `git ls-remote --tags` (a stale local tag after a deleted-and-repushed release would lie); `Cargo.toml` version on `origin/main` is below the target; CI green for `origin/main`'s sha, or, when that sha was a docs-only commit CI skipped, green for the newest ancestor within the last 30 runs that has a run with only docs-only paths between, else hard-fail with "CI status not found; check manually"; open PRs touching `docs/` or `README.md` listed; README lines matching `Status:|coming soon|not yet` printed; `git log v<last>..origin/main` printed with PR numbers and ticket IDs (exit 1 if empty).
Ends with the next-step hint.
The skill's steps 1-4 are replaced by this call in the same PR, otherwise the manual checklist stays authoritative and the saving never happens.
Absorbs skill steps 1-4's mechanics and deletes step 3.
Judgment left: whether each open doc PR describes something installable at this tag; README wording; whether the scope is worth a release and minor versus patch.

**C2. `release.sh ship <version> [--allow-open-docs] [--no-dry-run] [-n]`** (next, M, after C1 and C3).
Runs preflight (blocking on open doc PRs unless allowed).
Dispatches the pipeline dry run at `main` and records its head sha P (skipped when a green dispatch run at P already exists).
Bumps in a throwaway detached worktree from P: edit `Cargo.toml`, `cargo update --workspace --offline` (fail loud with the skill's hand-pinned-path-dep hint, never retry without `--offline`), commit `chore: bump version to <v>`, push `release/v<v>`, open the PR with a body generated from the preflight scope plus an HTML comment `<!-- yogurt-release: P=<sha> -->` so a resumed run reuses the exact P this PR was built from, `cd` out, remove the worktree.
Waits on the bump PR's checks, squash-merges it, takes the merge sha S, asserts parent(S) is P or that everything between is docs-only (via the shared helper), else exits with "main moved; re-run ship".
Waits for the dry run and for S's CI run, printing four job conclusions rather than the refresh stream.
Tags with `gh api repos/jarvisrchen/yogurt/git/refs -f ref=refs/tags/v<v> -f sha=$S`, which needs no local git objects (skip if the remote tag already points at S; exit 2 if it points elsewhere).
Watches the tag run, then calls verify and finish.
Every step derives done or not-done from GitHub (tag, PR state, run conclusion), so a re-run after the Bash tool's 600-second cap resumes instead of duplicating; the skill must say to call it again on timeout.
Absorbs skill steps 2, 5, 6, 7, the whole hidden bump PR, and the v0.3.0 rule, which tagging by merge sha makes unnecessary.
The bump and the tag stay two git objects, because AGENTS.md forbids direct pushes to `main` and the PR is what gives the tagged sha its own CI run.
Tokens: this is where most of the 20-35k per release goes today (about 20 tool calls, two `gh run watch` streams, a hand-written bump body); ship is one or two calls with about 400 tokens of output.

**C3. `release.sh verify | finish | untag <version>`** (now, M, second PR of the script, with C5).
`verify` re-downloads both tarballs and `SHA256SUMS`, hashes them, fetches `Formula/yogurt.rb` from the tap PR branch and requires both sha lines and the version to match, untars the host-arch binary and requires `yogurt --version` to print exactly `yogurt <v>` (safe to run: a `gh` download never carries `com.apple.quarantine`, and a comment next to the line says so), then prints PASS or FAIL per check.
`finish` runs verify, finds the tap PR by head branch `bump-<v>`, merges it, pulls the local tap clone with `--ff-only` (lighter than `brew update`), then `brew upgrade jarvisrchen/yogurt/yogurt` (or `reinstall` if already at that version), never untap or uninstall, asserts the version, runs `brew test`, checks for no `com.apple.quarantine`, and prints the log row pre-filled with run ids (looked up itself via `gh run list -w Release` on the tagged sha, so `finish` works standalone after an aborted `ship`), sha prefixes, the Ships list from `git log`, and a `NARRATIVE:` slot.
`untag` refuses (exit 2) if a GitHub Release exists, pointing at the fix-the-formula-by-hand path; otherwise deletes the remote tag and the local one.
Absorbs skill steps 8-11's mechanics and the v0.5.0/v0.6.0 upgrade-in-place workaround, which becomes the default.
Judgment left: the narrative sentence, and any feature-in-artifact `strings` check, which stays a one-line rule in the skill because the log already records that `strings | comm` is a false check.
The log row still ships as its own docs-only PR because it needs the narrative.

**C4. One release procedure** (now, S, before the script).
Keep the release skill as the executable checklist until C1-C3 exist; do not rewrite it to call a script that is not built.
Fix the untap order in both copies now, and rewrite the smoke-test step to say what has been true for three releases: `brew untap` and `uninstall` refuse while a `yogurt-model-*` formula is installed, upgrade-in-place is the normal path, from-scratch is the fallback.
Delete "Cutting a release" from `docs/RELEASING.md` in favour of a pointer to the skill; keep RELEASING.md for the decisions and recovery paths; leave CONTRIBUTING.md's short summary alone.
Move the release log table to `docs/RELEASE-LOG.md` and promote the four buried lessons (false `strings | comm` check, `brew untap` refusal, re-read `origin/main`'s sha before tagging, `git log <lasttag>..origin/main` for scope) into "When it goes wrong".
`git mv` `scripts/release-checklist.md` and all of `scripts/homebrew/` to `docs/archive/` per the archive rule; the two only reference each other, so nothing dangles.
Once C1-C3 ship, the skill shrinks to about 200 words: when to release, `preflight`, the one doc-PR judgment rule, `ship`, write the narrative into the printed row and open the log PR, and a pointer to `untag`.

**C5. Formula test asserts the exact version** (next, S, alongside C3).
In `release.yml`'s tap heredoc, `assert_equal "yogurt #{version}", shell_output("#{bin}/yogurt --version").strip` instead of the substring match; clap prints exactly `yogurt <version>`.
The tap job does not run on dry runs, so the first real release is the test; note it in that release's log row, then delete the skill's line-37 warning.

### 4D. The verification lever

**D1. `yogurt ctl`** (now, M for the first slice, L in full).
This is CLI-4 made concrete and the lever the rest stands on.
A `Ctl` subcommand in the existing binary: one `Client` (base URL, token from `~/.yogurt/session-token`, reqwest, which moves from `[dev-dependencies]` to `[dependencies]` in `yogurt-cli/Cargo.toml`; it is already compiled into the binary through yogurt-server) and thin subcommands.

First slice, the CLI-4 ticket:

```
yogurt ctl                         # no args = status (content-first)
yogurt ctl status                  # instances found (port, version, mode), active meeting, detected meeting, stt engine, provider, grants
yogurt ctl meeting list [--limit] | new [--title] [--start] | start <id|last> | stop [<id>]
yogurt ctl meeting show | summary | transcript [--follow] <id|url|last>
yogurt ctl meeting enhance <id|url|last>
yogurt ctl detect [dismiss]        # what meeting detection currently sees
yogurt ctl windows                 # on-screen windows with each one's match verdict; in-process SCK, no server needed
```

Second slice, a fast-follow ticket once the client and discovery are proven: `settings [get] | set k=v [--dry-run]`, `provider list | activate | test`, `models list | download [--wait] | delete [--dry-run]`, `ws [<id>] [--types]`, `meeting mute | search | delete [--dry-run]`.

`<id|url|last>` accepts a bare id, a meeting URL (port and id come from it), or `last`.
Read commands fall back to the DB and notes directory when no server answers, printing `source: db`.
Server discovery: `--port`, then `YOGURT_PORT`, then a health scan of 7878-7898 (the same window `port-guard.sh` uses); two answers make `status` list both and everything else exit 1 with `help: pass --port`.
`enhance` blocks for the whole LLM generation, so it forwards the server's `enhance_progress` frames to stderr as `phase: ...` lines; otherwise it reads as hung on a CLI provider and an agent retries and duplicates the generation.
`windows` on a machine without a Screen Recording grant prints `screen recording: denied` and exits 1, never an empty list that reads as "no meetings" (`detect_meeting` returns `None` on Denied today).
Output is compact text by default and `--json` on request; errors go to stdout as `error:` plus `help: <exact command>` with exit 1, usage errors exit 2, mutations are idempotent (`stop` on a stopped meeting is a no-op with exit 0).
No subcommand can set or reveal a key or the token, enforced by a test on `--help` output and on `provider list` output, not just by omission.
TOON output is not worth a dependency: no crate exists in the workspace and the difference on a five-column list is tens of tokens.
Absorbs every recipe in the control skill and docs/AI-INTEGRATION.md, `scripts/tail-transcript.sh`, the websocat recipe in docs/DEBUGGING-TRANSCRIPTS.md, the `meeting_windows` example in `crates/yogurt-audio/examples/`, and steps 1, 3 and 5 of docs/MODEL-EVAL.md.
It is a product-binary change and ships to brew users, so it needs `--help` and tests to README standard.
The server spawns nothing, so the one-process constraint is untouched.
Tokens: 2.5-4k per task that touches the running app, and no more retries against stale recipes because the surface is compiled against the server it talks to.

**D2. The control skill, in three steps** (now / now / later).
Now, no dependency: delete the recipe bodies the skill duplicates from docs/AI-INTEGRATION.md (about 400 of its 714 words), and correct the false sentence in both files: `yogurt start --no-open` runs without a browser tab but still in the foreground, so the caller backgrounds it (tmux by convention); the gap is control of a running instance, which is CLI-4.
Now, no dependency: a test in `crates/yogurt-cli/tests/skill_help.rs` that runs `yogurt --help` and each `yogurt <sub> --help` through `assert_cmd` (no `[lib]` target needed, unlike a `Cli::command()` import), renders a subcommand-plus-about block, compares it with the text between `<!-- yogurt-cli:start -->` and `<!-- yogurt-cli:end -->` markers in the skill, and rewrites it under `YOGURT_UPDATE_DOCS=1` in the INSTA_UPDATE style CI already uses; it also asserts every `yogurt <word>` mentioned in the skill, AI-INTEGRATION.md and README.md is a real subcommand.
Today's generated block already refutes the "no CLI" sentence on its own.
Later, after D1 has shipped in a brew release: the full rewrite to about 150 words (run `yogurt ctl` first, the generated block, a link to the Feature Map, and three rules: summary before transcript; one recording at a time, so `status` before `start`; never `cat` the token), fold AI-INTEGRATION.md's recipes into a route table plus one curl line, delete `scripts/tail-transcript.sh` (updating its mentions at TODO.md line 105 and the comment in `scripts/eval/compare.sh`), and point DEBUGGING-TRANSCRIPTS.md's two "Watch" sections at `ctl`.
The README's `npx skills add` path installs the skill standalone, so a skill naming `ctl` commands must not precede the binary that has them.

**D3. Feature Map, one table, guarded** (later, S, after D1 and E2).
`docs/FEATURES.md`, about 19 rows: Library and filters, new meeting and live page, transcript dock, mic mute, device switch, return-to-recording pill, detection banner, post-meeting page and enhance, notes editor and export, chat, labels, delete, Settings model / transcription / audio / general, dark mode, onboarding, style guide, CLI.
Columns: what it does, UI path, API, which test or spec covers it, source anchor; the `ctl` column is added only once D1 exists and can be checked against real `--help` output.
`docs/` rather than the skill's `references/` so the archive and docs-lint conventions apply uniformly; the skill links to it.
The coverage rule lives in `scripts/check-docs.sh` (E2), not a Rust test: yogurt-cli has no `[lib]` target for a test to import the clap tree from.
It extracts every `.route("...")` literal from `crates/yogurt-server/src` (7 of the 18 calls in `routes.rs` are multi-line, so the extraction must span lines), every `path:` in `web/src/router.tsx` (`/settings/:section` is one literal route, and the map row says so), and asserts each appears in the table or in an explicit `internal:` list at the bottom (session-token, ws, detected/dismiss).
It fails loudly if it finds zero routes.
That check is the maintenance loop: gated on every PR instead of once a day.
One file; split a row out only when its cell overflows.

**D4. DX-1: real-binary smoke suite, and `just test-hw`** (next, M, after D1).
`crates/yogurt-cli/tests/ctl_smoke.rs`, shaped like the existing `tests/cli.rs` that already spawns the real binary with a temp `HOME` (every `~/.yogurt` path resolves through `BaseDirs`, so `HOME` is the isolation mechanism): one server per test file (each boot runs real SQLite migrations), ephemeral port, `yogurt start --port P --no-open`, poll health, then drive `yogurt ctl --port P ... --json` and assert.
Hardware-free path, runs on macos-26 CI as is: status empty, `meeting new`, `show`, `list` with a total, `summary` is front-matter only, `enhance last` returns `too_short` on the MockLlm path, `stop` on a never-started meeting is a no-op, `ctl --port <free>` exits 1 with a hint.
Hardware path under `#[ignore]` plus `YOGURT_HW_TESTS=1`, exposed as `just test-hw` alongside the two hardware tests that already exist as `#[ignore]` (`yogurt-audio/tests/permission.rs`, `yogurt-stt/tests/whisper_smoke.rs`, which also needs `RUN_WHISPER_SMOKE=1` and a downloaded `small.en`, so the recipe sets and says both): `ctl windows` prints rows, `meeting new --start` shows active with `stt_engine` stamped, `mute on`, `stop` stamps `ended_at`.
That asserts "the capture pipeline opened and closed", which is the thing MTG-11 could not machine-verify; a real call is still needed for detection rules, and faking a window is the trap the log documents.
The gate is stated at the call site in the test file and the recipe comment: hardware tests never run under `just test` or CI, because a background `cargo test` that starts recording is the failure to avoid.

**D5. Health identity and the port line** (now, inside D1's PR).
`/api/health` gains `version` and `mode`, additive, no `pid` (new unauthenticated information for no consumer); extend `tests/health.rs` and the documented shape in AI-INTEGRATION.md.
`just dev` prints `YOGURT_PORT=<backend>   # pass --port or set this for yogurt ctl` after resolving the pair (not `export`, which a recipe cannot do into the caller's shell).
No server-written port registry: every worktree shares `~/.yogurt`, so the file would be wrong exactly when two instances exist.

**D6. `yogurt start --data-dir <path>`** (later, S, ticket first).
Scope is the actual hazard only: one `YOGURT_DATA_DIR` env var read in `yogurt-cli/src/main.rs` and threaded into the `db_path` and `app_db_path` seams `RunConfig` already has; not keys (a per-worktree copy of `keys.json` conflicts with "keys live only in `~/.yogurt/keys.json`"), not models, not notes.
`yogurt doctor` must read the same variable or it reports the wrong path.
The reason it exists at all: the two migration runners share one `db.sqlite` and `migrations.rs` says "whichever runner fires first wins", so a branch carrying a migration silently upgrades the real database under the main binary.
CLI-3's DONE entry already names this fix, conditionally ("if that ever bites"); it has not bitten, so it stays later.

**D7. Fixture meetings: `ctl meeting new --transcript-file <segments.json> [--ended]`** (next, S, with or right after D1).
The one pstack principle no proposal covered: seed the dev database.
Today the only ways to get a meeting with a transcript are a live recording or `just eval-play`, which speaks `scripts/eval/conversation.txt` through the speaker for five minutes and needs TCC grants; `test_support::seed_meeting` never compiles into a runnable binary.
So every PR that touches augmented notes or chat is verified by recording a real meeting, and D4's CI path can only ever reach the `too_short` branch of enhance.
Design: extend `POST /api/meetings` to accept optional `transcript_json` segments and `ended: true`, creating a finished, never-recorded meeting; `ctl meeting new --transcript-file` sends them, and `--from-script scripts/eval/conversation.txt` converts the `A:`/`B:` lines to `me`/`them` segments with synthetic timestamps so the existing eval ground truth doubles as the fixture.
The server keeps owning the transcript column, which DEBUGGING-TRANSCRIPTS.md calls the source of truth; writing the row from `ctl` through the DB crate was rejected for that reason.
Then `ctl meeting enhance last` verifies enhance against known content in seconds, D4 can assert on real enriched output under MockLlm, and `just eval-compare` works on fixtures.
Two optional fields on a local, token-guarded API is the whole product surface change.

### 4E. Instructions and docs

**E1. Rewrite AGENTS.md to about 480 words** (later, S, after A1, A2, B1-B4, E3).
Outline: what yogurt is (40 words); read ARCHITECTURE first (20); hard constraints as six self-contained bullets, with the CLI-provider exception cut to one real sentence plus a pointer to ARCHITECTURE section 7.6 (95); the task lifecycle as the six commands in section 3 (70); repo layout as pointers (100); conventions (150), including the one cloud-session paragraph from F1.
Evicted rationale (build splice, relative-path handover, port pair, gitignored files) is written into CONTRIBUTING.md's worktree section first; only the port-pair and gitignored-files parts are there today.
The word count is a budget, not a contract: the file has changed every few days.
Fix the two stale lines now regardless: `just build` does not run the web build, and `just lint` does not typecheck.
Tokens: about 1.2k per session, every session; the point is that five paragraphs become unnecessary rather than cheaper.
Rewrite only after the tools ship: prose describing a script that does not exist is worse than the current prose.

**E2. `scripts/check-docs.sh`** (now, S, size budget after A2).
About 40 lines, run by `just lint` and by a new `.github/workflows/docs.yml` on ubuntu with no path filter (about 15 seconds).
Rules: every `/api/...` token in docs, README and skills (excluding TODO.md and archive, and skipping `$VAR` segments and wildcards) matches a `.route("...")` literal, spanning lines; every backticked `just <name>` is a recipe (`^[a-z][a-z0-9-]*`, digits included, or `test-e2e` truncates); every relative markdown link outside the archive resolves; every backticked repo path exists; no em dash in tracked markdown, justfile and scripts outside the archive, test fixtures and prompt templates; size budgets `AGENTS.md < 12 KB` and `docs/TODO.md < 24 KB`, the latter only after A2 lands or as a ratcheting placeholder above today's size.
Scope of the em-dash rule, stated in the script: prose docs and scripts only. The count in those paths today is 61 (justfile 12, `scripts/*.sh` 26, `docs/.planning/` 15, `docs/superpowers/plans/` 8), which the same PR cleans up; the 5,883 occurrences across 378 tracked files are almost all Rust and TypeScript comments, which the rule was never meant to reach.
The first run also fails on `crates/yogurt-audio/README.md` line 60, whose link points at a file that moved to the archive.
It has to run outside the rust job because `ci.yml` skips docs-only PRs, which is where docs drift is introduced.
The route rule is one-directional (documented paths must exist); the reverse rule is D3's.

**E3. CI calls `just`** (now, S, before E1).
Install `just` in both CI jobs with a pinned action version.
Keep `lint` as fmt plus clippy (called by the rust job) and add `lint-web` for the typecheck (called by the web job, which has no cargo); make `test-web` run vitest plus Playwright; `test-rust` gains `--no-fail-fast` to match CI; `test` equals the two halves.
Note the Playwright first-run prerequisite (`pnpm --dir web exec playwright install chromium`) on the recipe.
AGENTS.md's command table shrinks to recipe names; the false "what CI runs" claim to fix is the comment on the `test` recipe in the justfile as well as AGENTS.md line 25.
Absorbs DX-1 part (a) and the class of drift where a check exists but sits in no gate.

**E4. Prune the agent memory directory** (now, minutes, outside the repo).
Four of the eleven notes restate AGENTS.md or the release skill; `plans_inventory.md` describes ten phases that all shipped; `project_e2e_second_instance.md` and `feedback_tmux_for_background_servers.md` describe a port-7879, `touch lib.rs`, raw `pnpm dev` workflow that `just dev` and `run-frontend.sh` replaced; `project_overview.md` gives a `cargo run -p yogurt-cli` command that fails because the package is `yogurt`.
Keep the notes about Richard's preferences; those are not in the repo.
Memory notes should point at repo files instead of restating them, so staleness has one place to go.

### 4F. Parallelism

**F1. Policy, no code** (one paragraph, inside E1).
Bucketing the last 30 merged PRs by what can verify them: 15 docs-only; 8 closable by cargo test, vitest and mocked Playwright (five version bumps and three UI PRs); 1 needing the real binary without hardware (AUD-4); 6 local-only (MTG-11, AUD-6, AUD-1 need hardware; LLM-5, LLM-6, LLM-7 need an installed, authenticated local agent CLI).
So: a ticket whose diff stays under `web/` or `docs/` may run as a cloud session, which is a fresh sandbox rather than a worktree, using the five lines from `ci.yml`'s web job as the environment, with Playwright screenshots attached to the PR because the pixel-perfect rule wants a human eye anyway and the Playwright suite mocks the backend.
Docs-only PRs skip CI, so the "watch the macos-26 job" advice applies to the web bucket only.
Rust stays local; the free macos-26 runner is the cloud verifier (`gh run watch` after push, about 2 minutes warm) and it already exists.
No self-hosted Mac runner, no Linux port of the workspace, no simulated meeting window.
Demand today is one open ticket (UI-5), so this is a permission, not an investment.

**F2. Published-drift check** (later, S, after C3).
`scripts/check-published.sh`, runnable by hand (the real trigger is "I just hand-edited the tap formula, did I break it") and weekly from a scheduled ubuntu workflow with `workflow_dispatch` as the escape hatch, no LLM: the latest `v*` tag equals the tap formula's version, both tarball URLs return 200 with matching shas, every `brew install ... yogurt-model-*` line in the README names an existing formula, every model mirror URL baked into `yogurt-stt` returns 200.
On failure, `gh issue create` rather than relying on the workflow email.
This is the one drift a PR-time check cannot see (the v0.3.0 README-versus-tap failure).
At one release a day the release smoke test already covers it; it matters once releases slow down.

## 5. Do not build

Consolidated from the reviewers, with the reason each time.

- A `yogurt task`, `yogurt dev`, or `yogurt release` subcommand in the product binary. Dev orchestration belongs in `scripts/` and the justfile; the binary ships to brew users and carries the one-process constraint. `ctl` is different: it speaks HTTP to a running server.
- Auto-starting `just dev` or a cargo build inside `just start`. A build kicked off invisibly in a tree is the hazard AGENTS.md lines 53-55 describe.
- A shared `CARGO_TARGET_DIR`, sccache, or an APFS clone. The cold build is 35 seconds.
- Auto-deriving the slug from the ticket title; the ID is already unique and one human word is cheaper than a heuristic.
- A JSON or SQLite ticket store. The markdown format is machine-parseable by its own rules.
- `--json` on `just start`, `just ticket`, `just pr`, `just land` before something parses it; one stable line per step is cheaper for an agent reading a terminal.
- A ruleset on `main` requiring a pull request, `gh pr merge --auto`, or required status checks. Richard is the sole collaborator, so a PR requirement only binds him; docs-only PRs never report a check, so a required check blocks a third of PRs forever; `land` waiting 1-3 minutes synchronously is simpler.
- A `.github/pull_request_template.md`. Ignored whenever `--body` is passed.
- A `--force` or merge-on-red flag on `land`. Red CI is a hard stop; the one-off merge stays a deliberate act by hand.
- `land` pulling the shared checkout's `main`, writing the DONE note itself, or removing a worktree with untracked files.
- The shared-checkout tree guard itself, and any persistent `YOGURT_OWN_MAIN` export. `just start` keeps agents out of the main checkout; the guard would only tax Richard, and a profile export would disable it for every agent spawned from that shell.
- A Claude Code PreToolUse hook for commit rules. It covers only Claude Code; the tracked git hooks cover every agent that uses git.
- Enabling the global `gsd-validate-commit.sh` hook via `.planning/config.json`. It enforces Conventional Commits, which rejects the `UI-7: ...` title convention.
- Release-profile builds for both arches on every PR to make the dry run unnecessary. Two extra macOS runners on 28 merges in three days to save one 3-minute dispatch per release, and the packaging steps would still be unexercised.
- A bot that bumps `Cargo.toml` on `main` and tags in one dispatch. Removes the CI run on the bump commit and contradicts the no-direct-push rule.
- Deriving the version from the tag at build time. Institutionalizes the Cargo.toml-versus-release mismatch the skill calls silent and ugly.
- Generating the log narrative with an LLM. The narrative's value is the human-noticed caveat; an auto-written one is the fabricated-verification failure the log exists to prevent.
- A local state file for `ship` progress. Derive done or not-done from GitHub, plus the one marker in the bump PR body, so re-runs and by-hand steps stay correct.
- Rewriting the release skill to call `scripts/release.sh` before the script exists.
- A TOON serializer. No crate, no consumer, tens of tokens of difference.
- `ctl eval play|compare` in the binary. Both spawn processes (`say`, `claude -p`); keep them as scripts.
- `ctl provider key set`, or any key or token in `ctl` output. Keys would land in shell history and transcripts.
- `pid` in `/api/health`. New unauthenticated information with no consumer.
- A server-written port registry. Shared `~/.yogurt` makes it wrong exactly when two instances exist.
- A per-worktree copy of `keys.json` for `--data-dir`. Conflicts with the keys-only-in-one-file constraint.
- Writing fixture meetings into SQLite from `ctl`. The server owns the transcript; go through the API.
- A daily or weekly LLM audit of AGENTS.md and the skills. Every drift in the history is a string-existence failure; the one non-grep failure (the "no CLI" sentence) was wrong when written, which an audit would not have caught either. Re-evaluate after E2 has run for a month if docs-fix commits keep appearing.
- A live-server docs-versus-router test when a source scan of `.route("` literals gives the same answer with no server.
- A Rust drift test that imports the clap tree. yogurt-cli has no `[lib]` target; parse `--help` output instead.
- Per-feature Feature Map files up front. Nineteen files nobody reads is the DONE section again.
- Synthetic audio injection at `/start` for CI. A production code path guarded by an env var in the capture supervisor; the fixture loader (D7) covers the enhance and chat cases without it.
- A cloud macOS environment, a Linux build of the workspace, a simulated meeting window, or a fake audio device. None can grant TCC or produce an honest signal.
- A generic markdownlint pass. It fights the one-sentence-per-line convention and catches none of the drift above.
- An em-dash check over Rust and TypeScript comments. 5,883 pre-existing hits and the rule says "in prose".
- Splitting or summarizing `docs/ARCHITECTURE.md`. It is read on demand for structural changes and 10.6k tokens is the right price.

## 6. Ship order

Dependency-respecting, in PR-sized units.
Tickets are allocated per unit rather than per proposal, because each ticket is a PR and the point is fewer PRs, not more.
The first two units are one afternoon each and pay back the same day.

| Unit | Contains | Size | Ticket |
| --- | --- | --- | --- |
| 1 | A2 TODO split (docs PR), then `just ticket` with its `--check` in `just lint`; fix the two stale AGENTS.md command lines | afternoon | DX-2 |
| 2 | A1 `just start`; A3 `just worktrees`; A4 `just dev-bg` | afternoon | DX-3 |
| 3 | E3 CI calls `just`, `lint-web`, Playwright in `test`; E2 `check-docs.sh` with the 61-hit cleanup; D2's two "now" steps (skill dedupe and false claim, `--help` drift test) | afternoon | DX-4 |
| 4 | B1 `just pr` and B2 `just land` in one `scripts/ship.sh`; B3 git hooks | day | DX-5 |
| 5 | C4 one release procedure (untap fix, archive, log split); C5 formula assert | afternoon | DX-6 |
| 6 | C1 preflight with the skill edit; then C3 verify/finish/untag; then C2 ship; then shrink the skill. Prove it on the next release | day, three PRs | DX-7 |
| 7 | D5 health identity; D1 `yogurt ctl` first slice | day | CLI-4 (exists) |
| 8 | D7 fixture meetings | half day | CLI-5 |
| 9 | D1 second slice; D2's full skill rewrite after the brew release that carries `ctl` | day | CLI-6 |
| 10 | D4 smoke suite and `just test-hw` | day | DX-1 (exists) |
| 11 | D3 Feature Map and its coverage rule | half day | DX-8 |
| 12 | E1 AGENTS.md rewrite with F1's paragraph | hour | DX-9 |
| 13 | D6 `--data-dir` | half day | CLI-7 |
| 14 | F2 published-drift check | half day | DX-10 |
| One-time, by hand | B4 GitHub settings (squash-only, delete branch on merge); E4 memory prune | minutes | none |

The single highest-leverage unit is 7, because the skill, the Feature Map, the smoke suite, the fixture loader and the manual-test handover all name `ctl`.
It is also the only unit that changes the shipped binary, so it goes out in a release.
Units 1 and 2 go first only because they are cheaper and remove the two most-repeated costs (the TODO read and the worktree setup) the same day.

## 7. Decisions

Fourteen of the questions this research raised have a defensible default or an answer from Richard, so they are decided here rather than left open.
Each lands by PR, so any of them can be reversed at review.

| # | Question | Decided | Why |
| --- | --- | --- | --- |
| 2 | Worktree and branch name (A1) | `<lowercase-id>[-words]`, no `fix/` or `docs/` prefix | the last five code PRs already use it; one style is the point |
| 3 | Does a task end with `just land`? (B2) | yes by default; the prompt says "just pr, do not land" when review is wanted | today's behaviour: 13 of the last 20 PRs merged within three minutes of opening |
| 4 | Does `ship` run through `finish`? (C2) | yes; `-n` and `--no-smoke` exist for the exceptions | every release so far merged the tap PR immediately after the hash check |
| 5 | Where DONE lives (A2) | `docs/TODO-DONE.md` | ID allocation reads it, so it is not archive material |
| 6 | Feature Map location (D3) | `docs/FEATURES.md` | the docs-lint and archive conventions apply uniformly |
| 7 | `ctl` public in the next release, first slice as in D1 (D1) | yes | the README already sends external users to install the control skill; a subcommand is the same class as `doctor`; veto at the CLI-4 PR if not |
| 9 | Em-dash rule scope (E2) | markdown, justfile and script prose only | AGENTS.md says "in prose"; 5,883 hits in code comments say the rule never meant them |
| 10 | tmux as a documented dev dependency (A4) | yes | Richard asked for tmux on 2026-08-28 |
| 11 | Shared-checkout tree guard (B5) | do not build | with `just start` and the pre-commit hook an agent is never in the main checkout to begin with; the guard's remaining cost lands on Richard every time he runs from `main`. Revisit if an agent builds there after DX-3 lands |
| 12 | Fixture loader (D7) | yes | two optional fields on a local, token-guarded API |
| 13 | Release log in its own file (C4) | yes | 9.5 KB read on every release, growing 2 KB per release |
| 14 | `--data-dir` (D6) | deferred | CLI-3's own note says "if that ever bites"; it has not |
| 8 | Squash commit message from `PR_BODY`? (B1) | no setting change; `land` passes the subject and body to `gh pr merge` | `PR_BODY` would be the entire PR description at merge time, handover section included; setting it explicitly per merge is one flag and keeps the stale worktree path out of history |
| 1b | Ruleset requiring a PR on `main`? (B4) | no, per Richard | sole collaborator on a public repo; nobody else can push, and the rule would only bind him |

The one that needs Richard, because it is an admin action on his account and binds his own pushes:

1. Change the repo settings once: squash-only and delete branch on merge (B4).
Richard decided against a ruleset on `main`: he is the sole collaborator, so nobody else can push there, and a PR requirement would only constrain him.
Nothing else in this plan waits on the two remaining settings; `just land` deletes the remote branch itself until `delete_branch_on_merge` makes that a no-op.

## 8. Measurements and open questions

- Cold debug build in a fresh worktree: 35 seconds, 290 crates, 1.7 GB `target/` (2026-09-01, this machine). Bootstrap: 8 seconds.
- Em dashes: 61 in prose paths (justfile, scripts, `docs/.planning`, `docs/superpowers/plans`); 5,883 across 378 tracked files, almost all code comments.
- Repo settings on 2026-09-01: squash, rebase and merge-commit all allowed; `delete_branch_on_merge` false; no branch protection; no rulesets; 23 merged branches still on origin.
- Does a debug binary at a new worktree path re-prompt Screen Recording? One `just dev` in a fresh worktree plus `/api/audio/permission` settles it, and decides whether `test-hw` needs a note about the first run.
- Does rust-embed pick up a rebuilt `web/dist` without touching `lib.rs`? `run-release.sh` assumes yes and seven release rows agree; a memory note assumes no. One deliberate UI edit followed by `just release` settles it.
- How many sessions run concurrently in practice? `git worktree list` shows two docs worktrees plus one foreign tree. If the honest answer is two, worktree cost stays a non-problem.
- Where does the handover requirement in `just pr` stop? Proposed trigger is any change under `crates/`, `web/src`, `justfile`, `scripts/`; docs-only and version-bump PRs are exempt. Check that against the next few PRs before hard-failing on it.
