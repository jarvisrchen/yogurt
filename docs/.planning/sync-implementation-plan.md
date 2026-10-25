# SYNC implementation plan

Companion to [sync-cloud-mirror-design.md](sync-cloud-mirror-design.md), which stays the source of truth for behavior.
This file says how to build it, in what order, and how each piece gets verified.
Rewritten 2026-10-24 for the Vercel and passcode design, against `main` at SYNC-1 (#150).

## Done means

Richard follows `docs/CLOUD-SYNC.md` from the deploy button and ends with:

- A Yogurt Cloud deployed on his Vercel account, backed by a new Supabase project.
- The phone signed in, set a passcode, and remembered the browser.
- Two machines connected with the Cloud address and pushed their own meetings.
- The phone reads both machines' meetings with the device picker, History shows an earlier push, and auto-push runs on its own.
- The Cloud login alone, used with `curl` against PostgREST, reads no meeting.

Every step in the guide is one Richard actually performed, with a screenshot from that run.

## Tickets

| Ticket | Delivers | Depends on |
| --- | --- | --- |
| SYNC-2 | `yogurt-cloud` reworked: crypto and link code removed, new `schema.sql` with the owner, passcode, and read RPCs, the REST client, the local test stack | none |
| SYNC-3 | The webapp shell on Vercel: build script, first visit, sign in, passcode, unlock, settings, empty library | SYNC-2 |
| SYNC-4 | Connect, manual push, disconnect, the `cloud_hash` migration, the Library pill, `yogurt ctl cloud`, the `AGENTS.md` edit | SYNC-2 |
| SYNC-5 | The webapp's library, device picker, meeting page, and History | SYNC-3, SYNC-4 |
| SYNC-6 | Auto-push on an interval | SYNC-4 |
| SYNC-7 | `docs/CLOUD-SYNC.md` with the deploy button, finished by a rehearsal on real accounts | SYNC-5, SYNC-6 |

SYNC-3 and SYNC-4 can run in parallel, and so can SYNC-5 and SYNC-6.
SYNC-4 is the first PR that sends data off the machine, so it carries the `AGENTS.md` hard-constraint edit and the README privacy section.
Each ticket runs through the usual `just start`, `just pr`, `just land` lifecycle.

## SYNC-2: rework the crate

The paused `sync-2` worktree (`/Users/rchen/Documents/code/yogurt-worktrees/sync-2`) holds uncommitted work built for the old design. Its `docs/.planning/sync-status.md` lists what is built.
Carry these over:

- `keys.json` following `--data-dir` and `$YOGURT_DATA_DIR`, with its `data_dir_flag_relocates_keys_json` test and the `CONTRIBUTING.md` note.
- `crates/yogurt-cloud/src/rest.rs` and `tests/rest.rs`: password sign-in, refresh, inserts, and paged selects and RPCs, with the typed `SessionExpired`, `BadCredentials`, `Http`, and `Network` errors. Drop admin user creation, which nothing needs now.
- `crates/yogurt-cloud/supabase/config.toml` and `just test-cloud`: the local Supabase on ports 56420 to 56429.

Leave the Management API setup flow, `ProjectPicker`, `Checklist`, `SetupView`, and `sync-2-http-contract.md` behind. The Cloud sync settings scaffolding (`CloudSection.tsx`, `CopyButton.tsx`, `Note.tsx`, the `/settings/:section/:view?` route) waits for SYNC-4.

Remove from `main`:

- `crypto.rs`, `link.rs`, `fixtures.rs`, `web/src/cloud/crypto.ts`, `crypto.test.ts`, and `__fixtures__/v1.json`.
- The `p256`, `hkdf`, `aes-gcm`, and `pbkdf2` dependencies, and anything else only they pulled into `Cargo.lock`.

Rewrite `schema.sql` to the design doc's Cloud schema section:

- `yogurt_versions.ct text` becomes `body jsonb`, and `yogurt_devices.name_ct` becomes `name text`. `yogurt_keyring` goes away.
- New `yogurt_owner` (one row) and `yogurt_browsers`.
- `yogurt_is_owner()`, a `security definer` helper that pins `yogurt_owner.user_id` on first use.
  The insert policies call it.
  It and the other helpers live in a `yogurt_private` schema that PostgREST does not expose, and `authenticated` may execute only the three policy helpers there.
- The RPCs from the design doc's function table, all `security definer` with `set search_path = ''` and schema-qualified names.
- Revoke `select` on every table from `authenticated`, revoke `execute` on every `yogurt_` function, and grant it back through an explicit allowlist of the RPCs.
- `yogurt_unlock` returns null on a wrong passcode and never raises, so the failure count commits.
  `yogurt_set_passcode` returns a boolean for the same reason, and a wrong `old_passcode` counts in the same counter.
- `yogurt_set_passcode` accepts only a 4 or 6 digit PIN and checks the format before `old_passcode`.
  Five wrong tries lock for 15 minutes, and each further run of five doubles it.
- `yogurt_start_push(device_id)` replaces `insert yogurt_pushes ... returning id`, which cannot work without `select`.
  Clients get no insert grant on `yogurt_pushes`.
- The policy checks that read other tables go through `security definer` helpers returning a boolean.
- A token failure raises SQLSTATE 28000 (403 from PostgREST), and a non-owner caller gets 42501.
  An unknown device raises `PT404`.
- `yogurt_start_push` raises `PT412` (HTTP 412) until a passcode exists, so no meeting data lands before the owner has chosen a PIN.

Add the build-side half of the owner step as a statement the build script can run: `insert into yogurt_owner (email) values ($1) on conflict do update`, where a changed email only applies while `user_id` is null.

Tests:

- `tests/schema_test.sql` in the `cloud-schema` CI job: schema twice, owner pinning, a non-owner account that can neither insert nor call an RPC, the Cloud login reading nothing through a table `select`, unlock with the right and wrong passcode, a 5-digit and a non-numeric passcode rejected, the lockout after five failures and its expiry, the second lockout lasting twice the first, alternating wrong `set_passcode` and `unlock` calls locking both on the fifth failure, a 12-hour token rejected after expiry (by updating `expires_at` as the test superuser), Forget, a passcode change deleting every browser so the webapp unlocks the current one again, and the as-of and history queries from SYNC-1.
- `just test-cloud` against the local Supabase: the REST client signs in as the owner, registers a device, starts a push, inserts versions in batches, sets a passcode, unlocks, and pages `yogurt_library` past 1,000 rows.
  A plain table `GET` as the owner answers 403.
- `owner.sql` is prepared as `set_owner(text)` in the schema test and applied by sed for the local Supabase.

Verify first, with pgcrypto on the local Supabase: `extensions.crypt` and `extensions.gen_salt('bf')` exist, and `extensions.gen_random_bytes` produces the token.

## SYNC-3: the webapp shell

Files:

- A new top-level `cloud/` folder: `package.json`, `index.html`, `src/`, and `vite.config.ts` building into `cloud/dist`. The config aliases `web/src` for the shared components and tokens, and sets `resolve.dedupe` for `react` and `react-dom`.
- `cloud/scripts/build.mjs`: on `VERCEL_ENV=production`, apply `../crates/yogurt-cloud/schema.sql` and the owner statement through `POSTGRES_URL_NON_POOLING` with `pg`, then write `dist/config.json`, then run the Vite build. Outside production it only writes `config.json` and builds.
- `cloud/vercel.json`: build command, output directory, and an `ignoreCommand` that skips builds when nothing under `cloud/`, `web/src/`, or `crates/yogurt-cloud/schema.sql` changed.
- CI: the web job gains a `cloud/` typecheck, test, and build step, because a change under `web/src` can break the webapp.
- Screens: first visit (create the login, check your email), sign in, choose a passcode (a 4 or 6 digit PIN, entered on a numeric keypad with `inputmode="numeric"` and `autocomplete="off"`), unlock with "Remember this browser", Lock, settings with the account, Sign out, Change passcode, and remembered browsers with Forget, and an empty library that says no machine has pushed yet.
- The token lives in `localStorage` under one key. A rejected token sends the user back to Unlock.

Richard's deployment:

- A Vercel project on the jarvisrchen Vercel account, imported from `jarvisrchen/yogurt` with root directory `cloud`, the Supabase integration added, and `YOGURT_OWNER_EMAIL` set. The local CLI switches accounts with `vercelgate switch`.
- The deploy button URL for other users waits for SYNC-7, but SYNC-3 checks it once on a throwaway Vercel project.

Tests:

- Vitest for the token storage and the status-to-screen routing.
- Playwright against the local Supabase from SYNC-2: first visit, email confirmation through the local Inbucket, passcode, Lock, unlock, wrong passcode, Forget, at phone width in light and dark.

Verify first, on a throwaway Vercel project:

- A build with root directory `cloud` can read `../crates/yogurt-cloud/schema.sql` and `../web/src`. Vercel's "Include files outside the root directory" setting should cover it.
- `POSTGRES_URL_NON_POOLING` from the integration is reachable from the Vercel build and accepts the multi-statement schema in one query.
- The integration's project has Confirm email on. Confirmation stays on in every case, because the owner email match depends on it.
- Supabase's built-in mail service sends only to the project's team members, at 2 emails an hour ([Auth SMTP docs](https://supabase.com/docs/guides/auth/auth-smtp)). Check whether a `YOGURT_OWNER_EMAIL` equal to the Vercel account email receives the confirmation, and what happens with a different email. Record the result in the design doc's Flow 1 section.

## SYNC-4: connect and push

`yogurt-db`:

- Migration `V014__meetings_cloud_hash.sql` adds `meetings.cloud_hash TEXT`.
- `MeetingRepo` gains `cloud_hashes`, `set_cloud_hash`, and `clear_cloud_hashes`. Connecting clears them all, so a new cloud gets a full push.
- `meeting_bundle(id)` returns `{"v":1,"meeting":<Meeting>,"chat":[...]}`. A test pins the serialization so the hash only changes when content does.

`yogurt-cloud`:

- `config.rs`: fetch and validate `https://<address>/config.json`. The address must be https, except plain http on loopback for tests.
- `push.rs`: a pure `plan(local: &[(id, hash)], cloud_ids) -> Plan { changed, deleted }`, unit-tested on its own, and `run`, which follows Flow 4 in batches.
- Token refresh: Supabase rotates refresh tokens on every use, so the server serializes refreshes behind one mutex and writes the new token to `keys.json` before using the session.

`yogurt-server`:

- Connect reads `yogurt_status` and refuses a login that is not the owner or an owner with no passcode yet, with "Choose a passcode in the webapp first".
- `GET /api/cloud`, `POST /api/cloud/connect`, `POST /api/cloud/push`, `POST /api/cloud/disconnect` (with `mark_deleted`).
- One `keys.json` account, `yogurt-cloud`, holding the address, login email, URL, publishable key, refresh token, device id, and machine name, so the connection is atomic.
- Two routes beyond the four above: `POST /api/cloud/check`, which backs the connect form's "Found a Yogurt Cloud" hint, and `POST /api/cloud/sign-in`, which renews a dead session from the stored email and a typed password.
- Every error carries a stable `code` (`bad_address`, `bad_config`, `bad_credentials`, `not_owner`, `passcode_not_set`, `session_expired`, `device_removed`, `network`, `cloud_error`, `not_connected`, `already_connected`, `bad_request`, `local_error`, `internal`) next to its message.
- A `keys.json` copied into a scratch data dir carries the `yogurt-cloud` account and would push as the same device, so the `CONTRIBUTING.md` copy advice must say to delete the `yogurt-cloud` entry from the copy once this stores it.
- An expired or revoked session returns a typed error the UI shows as "Sign in again", with the password field inline. A non-owner login returns its own error, "This login does not own that Yogurt Cloud".

Web:

- `"cloud"` joins `VALID_SECTIONS` and `SECTIONS`, reusing the `sync-2` worktree's scaffolding.
- Not set up (two cards), Connect, and Connected with the QR code for the address, rendered by `qrcode` (MIT).
- `components/library/Greeting.tsx` adds the "N not pushed" pill.

CLI: `yogurt ctl cloud status` and `yogurt ctl cloud push`. No ctl command sets or reveals a secret, so connecting stays in the UI.

Docs in the same PR: `AGENTS.md` (the constraint wording from the design doc), `README.md` Privacy posture, `docs/ARCHITECTURE.md` (a cloud sync section), and `docs/FEATURES.md` (the new routes).

Tests:

- `just test-cloud-e2e` (`scripts/test-cloud-e2e.sh`) drives the real binary through connect, push, an edit, a delete, a second push, a second laptop, a revoked session, a removed device, and disconnect against the local Supabase, with a stub `config.json` server on loopback, and checks the rows as the test superuser.
- Two instances on one Mac: a second `just dev-bg` with its own `YOGURT_DATA_DIR` plays Laptop 2.
- Development runs against a scratch `YOGURT_DATA_DIR`, because the shared `~/.yogurt` database would break other builds once V014 runs against it.

## SYNC-5: the webapp's content

- Library with the device picker (All devices by default), grouped by day, reusing the existing date grouping.
- A read-only meeting page reusing the markdown renderer and transcript line. Where a component reaches for the local API, the webapp passes data in instead of forking it.
- History from `yogurt_history`, and the as-of library from `yogurt_library(token, N)` under a "Viewing <time>" banner with Latest.
- Tests: Vitest for the grouping and the device filter, and Playwright against the local Supabase with two seeded devices, at phone width in light and dark.

## SYNC-6: auto-push

- A setting `cloud_push_interval` (`off`, `15m`, `1h`, `1d`) in the existing settings store, with a segmented control on the Connected page.
- A task spawned beside `detect::spawn` in `yogurt-server/src/lib.rs`, copying its `tokio::time::interval` pattern and re-reading the setting each tick.
- The manual button and the timer share one mutex, so two passes never overlap. A failed pass shows on the Connected page and retries at the next tick.
- Tests: a paused-time tokio test for the scheduling, and a 15-minute soak on the two local instances.

## SYNC-7: the setup guide

`docs/CLOUD-SYNC.md`, linked from the README, written for someone who has never opened Vercel or Supabase:

1. What Yogurt Cloud is, what leaves the machine, and who can read it.
2. Deploy: the button, the Supabase integration, the owner email, and the existing-project path. The owner email should be the Vercel and Supabase account email, so the built-in mail service delivers the confirmation. Custom SMTP, for example Resend, is the fallback for any other address.
3. First visit: create the login, confirm the email, choose a passcode, add the webapp to the phone's home screen.
4. Each laptop: Connect with the Cloud address, push.
5. Everyday use: the Push button, auto-push, what the "not pushed" pill means.
6. History and recovery: viewing an earlier push, a wiped laptop, a forgotten passcode, purging a device's data in the SQL editor.
7. Disconnecting a laptop, and forgetting a browser.
8. Troubleshooting: a network blocking `*.supabase.co` or `*.vercel.app`, the free tier pausing a quiet project, "Sign in again", keeping a deploy button copy up to date.
9. Who can read what: the table from the design doc.

Rehearsal: Richard runs the guide top to bottom while I follow along and fix the guide or the code wherever the two disagree.
Creating the Vercel and Supabase accounts and confirming the email are Richard's own steps, because those are real account credentials.
Laptop 2 is a real second machine if one is at hand, otherwise a second instance with its own `YOGURT_DATA_DIR`.
SYNC-3 to SYNC-6 each add their section of the guide as they land, so SYNC-7 finishes and checks a draft.
