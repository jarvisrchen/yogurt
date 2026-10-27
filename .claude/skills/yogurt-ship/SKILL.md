---
name: yogurt-ship
description: Take a yogurt change from a request to a merged PR - allocate a ticket ID, open a worktree off main, implement and verify end to end, review the diff with ponytail-review, open the PR, and land it. Use when asked to "ship", "ticket and implement", "make a PR for", or "/yogurt-ship" a fix or feature in this repo, especially while another session works in the same checkout.
---

# Ship a yogurt change

AGENTS.md owns the lifecycle and its rules; this is the order of operations and the traps between them.

1. **Allocate an ID.**
   `just ticket next <PREFIX>` also skips IDs in open PR titles and branches.
   If it warns that it could not read open PRs, check `gh pr list --state open` by hand.

2. **Open a worktree.**
   `just start <id> <words>` with the lowercase slug form (`just start ui-18 short-model-pills`), since a new ID is not on main yet.
   Never edit the shared main checkout: another session may be building there.

3. **Research when it is not obvious.**
   Read the code the change touches and grep every caller first.
   A bug starts with a reproduction in the real app.

4. **Implement and test.**
   Run the narrow suite while iterating, then `just test-web` or `just test`.

5. **Verify end to end.**
   `just dev-bg` in the worktree and drive the real UI.
   To show data without touching `~/.yogurt`, copy the DB into a scratch dir with `sqlite3 ~/.yogurt/db.sqlite ".backup <dir>/db.sqlite"`, copy `keys.json` beside it, and run with `YOGURT_DATA_DIR=<dir>`.
   The notes folder and session token live in that dir too, and `yogurt ctl` needs the same variable exported.
   For UI work, take Playwright screenshots of the changed surface in light and dark (`colorScheme`) at a real viewport, and look at them.
   Skip `fullPage`, which misplaces the fixed Ask bar.
   Screenshots for the PR show test meetings in a fresh `YOGURT_DATA_DIR`, never real ones, because the repo is public.

6. **Register and close the ticket in the branch.**
   Add the `- [ ] **ID**` entry under its section in `docs/TODO.md`, then `just ticket done <ID> --note-file <f>`.

7. **Review.**
   Run the `ponytail:ponytail-review` skill on `git diff origin/main...` plus the uncommitted diff, and apply the cuts worth taking.
   Re-run the tests after.

8. **Commit.**
   Read `git status` before staging: build output such as a stray `tsbuildinfo` must not ride along.
   One-line subject `<ID>: <what changed>`.

9. **Check for conflicts.**
   `gh pr diff <n> --name-only` for each open PR.
   Overlap in code means coordinate before landing.
   Overlap only in `docs/TODO-DONE.md` is an append, and whoever lands second rebases.

10. **Open the PR and land it.**
    `just pr "<ID>: <title>" --body-file <f>` with Why, What, and an absolute-path Manual test section.
    A UI change also gets a Screenshots section: a table with a Light and a Dark column.
    Push the PNGs to the `pr-screenshots` branch under `<slug>/` from a detached worktree of `origin/pr-screenshots` (`git push origin HEAD:pr-screenshots`), and embed them as `https://raw.githubusercontent.com/jarvisrchen/yogurt/pr-screenshots/<slug>/<file>.png`.
    `just land` waits for CI, squash-merges, and removes the worktree.

End the reply with the Manual test commands and the PR link.
