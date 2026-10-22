# SYNC: push meetings to a self-hosted cloud for reading on a phone

Design, not yet implemented.
Tickets: `docs/TODO.md` SYNC-1 through SYNC-3.

jarvisrchen runs yogurt on more than one laptop and wants to read every meeting from his phone.
Yogurt stays a local app with no login.
A new, optional cloud account is a mirror that local installs push into and a phone reads from.
Each user hosts that cloud in their own Supabase project, so no central yogurt server or database exists.

## Rules

- A local install works exactly as today with no account, and sync is off until the user links one in Settings.
- The cloud is the user's own Supabase project. Yogurt ships a SQL file and a static viewer, and hosts no data.
- Sync is one-way. A laptop pushes and never reads meeting content back. The phone is read-only.
- Supabase stores ciphertext only. The passphrase and the data key never leave the user's devices.
- Out of scope: editing on the phone, LLM chat on the phone, recording on the phone, sharing between people, conflict resolution.

## Concepts

**Cloud account.** A Supabase Auth user in the user's own project.
Row Level Security isolates accounts, so one project can hold several, but the expected case is one person with one account.

**Device.** A linked yogurt install with a user-chosen name such as "Laptop 1".
Linking creates a row in `devices`, and every meeting that install pushes carries its device id.
The phone filters by device and defaults to "All devices".

**Data key.** One random 256-bit key per cloud account, which encrypts every meeting.
The cloud only ever holds it wrapped by a key derived from the user's passphrase.

## One-time setup in Supabase

1. Create a Supabase project.
2. Paste `cloud/schema.sql` (below) into the SQL editor and run it.
3. Under Authentication, create the account with email and password. Google sign-in is optional and only matters for the phone.
4. Turn off public sign-ups. The publishable key is public by design, so with sign-ups on, anyone holding it could register an account in the project. RLS would still keep them out of your rows.
5. Copy the project URL and the publishable (anon) key.

The publishable key is safe in a browser and in `~/.yogurt/` because RLS, not key secrecy, protects the rows.
The secret `service_role` key bypasses RLS and is never asked for anywhere.

## Cloud schema

```sql
create table devices (
  id         uuid primary key default gen_random_uuid(),
  user_id    uuid not null default auth.uid() references auth.users on delete cascade,
  name_ct    text not null,
  created_at timestamptz not null default now()
);

create table keyring (
  user_id     uuid primary key default auth.uid() references auth.users on delete cascade,
  kdf         jsonb not null,  -- {"alg":"pbkdf2-sha256","iter":600000,"salt":"<b64>"}
  wrapped_key text  not null
);

create table meetings (
  id        text primary key,  -- the local ULID
  device_id uuid not null references devices on delete cascade,
  user_id   uuid not null default auth.uid() references auth.users on delete cascade,
  ct        text not null,
  pushed_at timestamptz not null default now()
);

alter table devices  enable row level security;
alter table keyring  enable row level security;
alter table meetings enable row level security;
create policy own on devices  for all using (user_id = auth.uid()) with check (user_id = auth.uid());
create policy own on keyring  for all using (user_id = auth.uid()) with check (user_id = auth.uid());
create policy own on meetings for all using (user_id = auth.uid()) with check (user_id = auth.uid());
```

The only plaintext the cloud holds is the account email, row ids, `pushed_at`, and blob sizes.
Titles, dates, notes, transcripts, labels, chat, and device names are all inside ciphertext.

## Encryption

AES-256-GCM everywhere, with a fresh random 96-bit nonce per encryption.
The laptop side uses the `aes-gcm` crate (MIT or Apache-2.0), and the phone side uses WebCrypto, so the browser needs no crypto library.

- **Key derivation.** PBKDF2-HMAC-SHA256, 600,000 iterations, 16-byte random salt, all stored in `keyring.kdf`. WebCrypto implements PBKDF2 natively. Argon2id is stronger but would need WebAssembly on the phone, so `kdf.alg` exists to allow that upgrade later without breaking old accounts.
- **Wrapping.** The passphrase-derived key encrypts the data key with AES-GCM, and the result goes in `keyring.wrapped_key`. Changing the passphrase re-wraps one value and re-encrypts no meetings.
- **Meeting blob.** `ct` is base64 of `nonce || ciphertext`. The plaintext is one JSON document `{"v":1,"meeting":{...},"labels":[...],"chat":[...]}` holding every `meetings` column, the meeting's label names and colors, and its chat messages.
- **Binding.** The associated data is `"<meeting id>|<device id>"`, so a server that swaps blobs between rows produces a decryption failure, not a wrong meeting.
- **Lost passphrase.** The cloud copy becomes unreadable. Because it is only a mirror, recovery is to wipe the account's rows, set a new passphrase, and push again from each laptop.

One fixed test vector (key, nonce, AAD, plaintext, expected ciphertext) is checked by both a Rust unit test and a vitest test, so the two implementations cannot drift.

## Linking a laptop

Settings gains a Cloud sync section with project URL, publishable key, email, password, device name, and passphrase.

1. The server signs in through `POST <url>/auth/v1/token?grant_type=password` and keeps the refresh token.
2. If `keyring` has no row, the server generates the data key, wraps it, and inserts the row. This makes it the first device. Otherwise the server unwraps the existing key, and a wrong passphrase fails here with a clear error before anything is pushed.
3. It inserts the `devices` row with the encrypted name.
4. It stores the refresh token, data key, device id, URL, and publishable key in `~/.yogurt/keys.json`, mode 0600 through `FileKeyStore`. Storing the data key means pushes never ask for the passphrase again on that laptop. Nothing here touches SQLite or a log.

Unlink deletes those entries locally and offers to delete the device row, which cascades to every meeting it pushed.

All calls are plain HTTPS through the `reqwest` the server already depends on, with no Supabase SDK in Rust.

## Pushing

A "Push meetings" button in the Cloud sync section runs one push pass.
The section shows the last push time, how many meetings went up, and the last error if any.

A push pass:

1. Builds each local meeting's plaintext JSON and hashes it with `sha2`.
2. Encrypts and upserts (`POST /rest/v1/meetings` with `Prefer: resolution=merge-duplicates`) every meeting whose hash differs from the one recorded at its last push, then records the new hash.
3. Lists this device's cloud ids (`select id`, no content) and deletes any id that no longer exists locally, so a local delete reaches the phone on the next pass.
4. Skips the meeting that is currently recording.

Hashing the plaintext catches every kind of change, including label and chat edits that do not touch `meetings.updated_at`.
The recorded hash lives in a new nullable `meetings.cloud_hash` column, which means one migration and the usual shared-DB migration care while the branch is unmerged.

Size check against the current `~/.yogurt/db.sqlite`: 45 meetings total 630 KB, and the largest meeting (1 hour 58 minutes) is 148 KB.
Base64 adds a third, so the free tier's 500 MB holds a few thousand two-hour meetings.

## Automatic sync (SYNC-3)

A toggle plus an interval (15 minutes, 1 hour, or daily) runs the same push pass on a timer inside the server.
It runs only while yogurt is running, which is the same assumption every other yogurt feature makes.
Manual push stays available with the toggle on.

## The phone viewer

A second Vite entry in `web/` (`web/cloud.html`) builds a static, read-only viewer.
It shares the existing markdown and transcript components and leaves out every call to the local API.
It talks to Supabase with `@supabase/supabase-js` (MIT), which handles session refresh.

Screens:

1. **Connect.** Project URL and publishable key, remembered in the browser.
2. **Sign in.** Email and password, or Google if the project enabled it.
3. **Unlock.** The passphrase unwraps the data key into a non-extractable WebCrypto key held in IndexedDB, so the passphrase is asked once per browser. Sign out clears it.
4. **Library.** Meetings decrypted on the device and grouped by day, a device picker defaulting to "All devices", and search over the decrypted text.
5. **Meeting.** Notes, enhanced notes, transcript, and chat, read-only.

The viewer downloads and decrypts every row on open.
That is fine at personal scale. The upgrade path, when it gets slow, is an IndexedDB cache that fetches only rows with a newer `pushed_at`.

**Hosting.** One public build on GitHub Pages, deployed from `main`, is the default.
It holds no data and no keys, and works against any user's project.
Whoever controls that hosting controls the JS that sees the decrypted data, so the docs also give a one-click deploy of the same build for anyone who wants their own copy.

## Constraint changes

`AGENTS.md`'s first hard constraint becomes: audio never leaves the machine unless the user opted into cloud STT (then only audio), and notes and transcripts leave only when the user links a cloud account in a Supabase project they host, and only as ciphertext.
The rest stay as they are. Sync sends nothing to yogurt or any third party the user did not configure, so zero telemetry holds. It runs inside the one process, so the no-subprocess rule holds. The new crate is MIT-compatible.
The archived PRD's "There is no Yogurt cloud" still holds, because no yogurt-hosted cloud exists.

## Supabase free tier

Third-party sources (not Supabase's own pricing page) agree on 500 MB of database per project, 2 free projects, and a pause after one week of inactivity that needs a manual restore from the dashboard.
Automatic sync on a laptop used weekly probably keeps the project awake, but whether API writes count as activity is unverified.
Pro ($25 a month) removes the pause.
Verify against Supabase's pricing page before SYNC-1 lands.

## Phasing

1. **SYNC-1.** `cloud/schema.sql`, crypto with the shared test vector, link and unlink, manual push, the `cloud_hash` migration, and the constraint edit in `AGENTS.md`.
2. **SYNC-2.** The phone viewer and its GitHub Pages deploy.
3. **SYNC-3.** Automatic sync on an interval.

## Open questions

- Should a meeting be excludable from push, for example a sensitive 1:1? The default pushes everything.
- After a local reset of `~/.yogurt/db.sqlite` with the link still in `keys.json`, the next pass would delete that device's whole cloud copy. Is that right for a mirror, or should a pass refuse to delete when the local library is empty?
- Should the phone remember the unlocked key per browser (the default) or ask for the passphrase every session?
