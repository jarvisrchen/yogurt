# SYNC implementation plan

Companion to [sync-cloud-mirror-design.md](sync-cloud-mirror-design.md), which stays the source of truth for behavior.
This file says how to build it, in what order, and how each piece gets verified.
Written 2026-10-24 against `main` at 1.6.0.

## Done means

Richard follows `docs/CLOUD-SYNC.md` from a fresh Supabase account and ends with:

- Laptop 1 created a Yogurt Cloud and pushed its meetings.
- A second machine connected with the link code and pushed its own meetings.
- The phone scanned the QR, unlocked once, and reads both machines' meetings with the device picker.
- History shows an earlier push, and auto-push runs on its own.

Every step in the guide is one Richard actually performed, with a screenshot from that run.

## Tickets

Six PRs, each shippable on its own, in this order.
The IDs replace the three placeholder tickets on this branch, which never reached `main`.

| Ticket | Delivers | Depends on |
| --- | --- | --- |
| SYNC-1 | `yogurt-cloud` crate: crypto, link code, schema, cross-language fixtures | none |
| SYNC-2 | Laptop 1 setup through the Management API, with the project picker and SQL fallback | SYNC-1 |
| SYNC-3 | Connect, manual push, disconnect, the `cloud_hash` migration, the Library pill, `yogurt ctl cloud` | SYNC-2 |
| SYNC-4 | The phone viewer with settings and History, the QR code, the GitHub Pages deploy | SYNC-3 |
| SYNC-5 | Auto-push on an interval | SYNC-3 |
| SYNC-6 | `docs/CLOUD-SYNC.md`, finished by a rehearsal on real accounts | SYNC-4, SYNC-5 |

SYNC-3 is the first PR that sends data off the machine, so it carries the `AGENTS.md` hard-constraint edit and the README privacy section.
SYNC-4 and SYNC-5 can run in parallel.
Each ticket runs through the usual `just start`, `just pr`, `just land` lifecycle.

## SYNC-1: the crate

New crate `crates/yogurt-cloud`, so the crypto and the Supabase clients sit behind one `thiserror` boundary and the server only calls a small API.

Files:

- `crypto.rs`: `generate_keypair`, `wrap_private_key(secret, passphrase) -> (wrapped, kdf)`, `seal(public_key, aad, plaintext) -> String`. A test-only `open` and `unwrap_private_key` exist for round trips. The laptop build never decrypts.
- `link.rs`: `LinkCode { url, publishable_key, public_key }` with `encode` and `decode` for the `yogurt-cloud:` form and the `#c=` fragment.
- `schema.sql`: the SQL from the design doc, made idempotent (`create table if not exists`, `create or replace`, `drop policy if exists`) so a setup interrupted after the schema step can simply run again. Embedded with `include_str!`.
- `fixtures.rs` (test-only): writes `web/src/cloud/__fixtures__/v1.json` with a wrapped private key, its passphrase, and one sealed meeting.

New dependencies, all MIT or Apache-2.0: `p256` (feature `ecdh`), `hkdf`, `aes-gcm`, `pbkdf2`. Already in the tree: `sha2`, `rand`, `base64`, `reqwest` with rustls.

Tests:

- Rust: seal then open round-trips, a wrong AAD fails, a wrong passphrase fails, the link code round-trips and rejects garbage.
- Vitest (`web/src/cloud/crypto.test.ts`, node environment for WebCrypto): decrypts the Rust fixture with the browser-side code SYNC-4 will use. Lands now so the formats are locked before any data exists.
- Schema: a new `cloud-schema` CI job on ubuntu with a `postgres:16` service runs `schema.sql` twice (idempotence) and the history test from the design review (as-of queries, push log counts, user isolation, update and delete blocked).

## SYNC-2: setup on Laptop 1

Server, in `yogurt-cloud`:

- `mgmt.rs`: Management API client for `GET /v1/organizations`, `GET /v1/projects`, `POST /v1/projects`, `GET /v1/projects/{ref}`, `POST .../database/query`, `PATCH .../config/auth`, `GET .../api-keys?reveal=true`.
- `rest.rs`: project client for `POST /auth/v1/admin/users`, `POST /auth/v1/token`, and PostgREST inserts and selects.
- `setup.rs`: the Flow 1 steps as a list. Each step checks whether it is already done before acting (project exists, `yogurt_keyring` has a row, login exists), which makes a re-run resume where the last one stopped.

Server, in `yogurt-server`:

- `api/cloud.rs`: `POST /api/cloud/setup/projects` (token in, organizations and projects out) and `POST /api/cloud/setup` (starts the run), plus `GET /api/cloud` for status and progress.
- Setup runs as a server task holding the token, the secret key, and the passphrase in memory only. Closing the browser tab does not stop it. Reopening Settings shows the checklist where it is. Quitting yogurt drops the secrets, and a re-run resumes.
- On success it writes one `keys.json` account, `yogurt-cloud`, holding a JSON string with the URL, publishable key, public key, refresh token, device id, and device name. One entry keeps the connection atomic.

Web:

- `"cloud"` joins `VALID_SECTIONS` in `routes/Settings.tsx` and `SECTIONS` in `components/settings/SidebarNav.tsx`.
- `components/settings/cloud/`: the not-set-up page (mock A), the setup form with project picker (B, B3), the checklist, and the SQL fallback (B2).
- `lib/api/cloud.ts`: TanStack Query hooks in the shape of `lib/api/settings.ts`.

Tests:

- A fake Management API in a Rust test (an axum router on an ephemeral port) drives setup through success, the Beta query failing (fallback path), the two-project cap, a reused project that already has a keyring, and resume after a failure at each step.
- Playwright: the setup screens against the mocked backend, with screenshots attached to the PR.

Verify first, with Richard's token on a throwaway project, before writing `mgmt.rs`:

- `database/query` accepts the whole multi-statement `schema.sql` in one call.
- `api-keys?reveal=true` returns the new `sb_publishable_` and `sb_secret_` keys, and `/auth/v1/admin/users` accepts the secret key.
- How long after `ACTIVE_HEALTHY` the project's Auth answers. Setup retries with backoff either way.
- The free tier facts in the design doc, on Supabase's own pricing page.

## SYNC-3: connect and push

`yogurt-db`:

- Migration `V014__meetings_cloud_hash.sql` adds `meetings.cloud_hash TEXT`.
- `MeetingRepo` gains `cloud_hashes`, `set_cloud_hash`, and `clear_cloud_hashes`. Connecting to a cloud clears them all, so a new cloud gets a full push.
- `meeting_bundle(id)` returns the plaintext JSON `{"v":1,"meeting":<Meeting>,"chat":[...]}`. The `Meeting` struct already carries labels and matches the web `Meeting` type, so the viewer renders it directly. A test pins the serialization (field order, label order) so the hash only changes when content does.

`yogurt-cloud`:

- `push.rs`: a pure `plan(local: &[(id, hash)], cloud_ids) -> Plan { changed, deleted }`, unit-tested on its own, and `run`, which follows Flow 4 in batches.
- Token refresh: Supabase rotates refresh tokens on every use, so the server serializes refreshes behind one mutex and writes the new token to `keys.json` before using the session.

`yogurt-server`:

- `POST /api/cloud/connect`, `POST /api/cloud/push`, `POST /api/cloud/disconnect` (with `mark_deleted`).
- `GET /api/cloud` adds device name, meetings on this Mac, in the cloud, changed since the last push, the last push result, and the link code.
- An expired or revoked session returns a typed error the UI shows as "Sign in again", with the password field inline.

Web:

- Connected page (mock C) with the Push button and the link code. The QR waits for SYNC-4, which knows the viewer URL.
- Connect page (mock D).
- `components/library/Greeting.tsx` adds the "N not pushed" pill (mock E).

CLI: `yogurt ctl cloud status` and `yogurt ctl cloud push`. Following `commands/ctl/settings.rs`, no ctl command sets or reveals a secret, so connect and setup stay in the UI.

Docs in the same PR:

- `AGENTS.md`, the hard constraint wording from the design doc.
- `README.md`, Privacy posture.
- `docs/ARCHITECTURE.md`, a cloud sync section.
- `docs/FEATURES.md`, the new routes.

Tests:

- Local Supabase: `supabase start` gives real Auth and PostgREST on `127.0.0.1:54321`. A `just test-cloud` recipe applies `schema.sql`, creates a login, seeds a keyring through the SYNC-2 code, then drives the real binary through connect, push, an edit, a delete, a second push, and disconnect. It checks the rows through PostgREST and decrypts them with the test-only `open`. It needs Docker, so it stays local like `just test-hw`.
- Two instances on one Mac: a second `just dev-bg` with its own `YOGURT_DATA_DIR` plays Laptop 2, which proves that two devices push into one cloud without touching each other.
- Development runs against a scratch `YOGURT_DATA_DIR`, because the shared `~/.yogurt` database would break other builds once V014 runs against it.

## SYNC-4: the viewer

- `web/cloud.html` and `web/src/cloud/`: unlock (F), library with the device picker (G), meeting (H), settings (I), History (J), and as-of library (K).
- It reuses the existing markdown renderer, transcript line, and date grouping. Where a component reaches for the local API, the viewer passes data in instead of forking it.
- `@supabase/supabase-js` (MIT) handles sign-in and session refresh. WebCrypto handles every decrypt. The unwrapped key is stored non-extractable in IndexedDB until Lock.
- A separate `web/vite.cloud.config.ts` builds into `web/dist-cloud` with `base: "/yogurt/"`, which keeps the viewer out of the binary's embedded `web/dist`.
- `.github/workflows/pages.yml` deploys `dist-cloud` on pushes to `main` that touch `web/`. The repo is public, so the viewer lands at `https://jarvisrchen.github.io/yogurt/cloud.html`. Richard turns on Pages with "GitHub Actions" as the source in the repo settings, which is one click.
- The laptop's connected page gains the QR, rendered by the `qrcode` npm package (MIT) from the viewer URL plus `#c=`.

Tests:

- Vitest for the crypto, the as-of grouping, and the device filter.
- Playwright against the local Supabase from SYNC-3: unlock, both devices listed, open a meeting, Lock, History, and back to latest.
- The mocks' visual details, checked at phone width in light and dark.

## SYNC-5: auto-push

- A setting `cloud_push_interval` (`off`, `15m`, `1h`, `1d`) in the existing settings store, with the segmented control from mock C.
- A task spawned beside `detect::spawn` in `yogurt-server/src/lib.rs`, copying its `tokio::time::interval` pattern and re-reading the setting each tick.
- The manual button and the timer share one mutex, so two passes never overlap. A failed pass shows on the connected page and retries at the next tick.
- Tests: a paused-time tokio test for the scheduling, and a 15-minute soak on the two local instances.

## SYNC-6: the setup guide

`docs/CLOUD-SYNC.md`, linked from the README, written for someone who has never opened Supabase:

1. What Yogurt Cloud is, what leaves the machine, and the three credentials.
2. Laptop 1: create the Supabase account, create the access token, run setup in yogurt, revoke the token afterward.
3. Phone: scan the QR, sign in, unlock, add the viewer to the home screen.
4. Laptops 2 and 3: copy the link code from Laptop 1 or the phone, connect, push.
5. Everyday use: the Push button, auto-push, what the "not pushed" pill means.
6. History and recovery: viewing an earlier push, a wiped laptop, a lost passphrase, purging a device's data in the SQL editor.
7. Disconnecting a laptop.
8. Troubleshooting: a corporate network blocking `*.supabase.co`, the free tier pausing a quiet project, the schema fallback, "Sign in again".
9. What sits where: the table from the design doc.

Rehearsal: Richard runs the guide top to bottom while I follow along and fix the guide or the code wherever the two disagree.
Two of the steps are Richard's own by necessity, signing up at supabase.com and pasting the access token, because those are real account credentials.
Laptop 2 is a real second machine if one is at hand, otherwise a second instance with its own `YOGURT_DATA_DIR`.
Screenshots come from that run.
SYNC-2 to SYNC-5 each add their section of the guide as they land, so SYNC-6 finishes and checks a draft rather than starting from nothing.

## Open items to settle during the work

- The viewer's Pages URL replaces `yogurt.github.io/cloud` in the mocks once the real one exists.
- If the Beta `database/query` endpoint rejects multi-statement SQL, setup sends the schema one statement at a time before falling back to the manual path.
