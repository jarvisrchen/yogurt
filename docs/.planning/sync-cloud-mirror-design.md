# SYNC: push meetings to a self-hosted cloud for reading on a phone

Design, not yet implemented.
Tickets: `docs/TODO.md` SYNC-1 through SYNC-3.
Review surface with diagrams and UI mocks: [../.lavish/sync-cloud-mirror-design.html](../.lavish/sync-cloud-mirror-design.html).

jarvisrchen runs yogurt on more than one laptop and wants to read every meeting from his phone.
Yogurt stays a local app with no login.
A new, optional Yogurt Cloud is a mirror that local installs push into and a phone reads from.
Each user hosts their Yogurt Cloud in their own Supabase project, so no central yogurt server or database exists.

## Rules

- A local install works exactly as today with no account, and sync stays off until the user turns it on in Settings.
- Yogurt Cloud is the user's own Supabase project. Yogurt ships the setup automation, a SQL file, and a static viewer, and hosts no data.
- Sync is one-way. A laptop pushes and never reads meeting content back. The phone is read-only.
- Supabase stores ciphertext only.
- Laptops can encrypt but never decrypt. Only the viewer, after the passphrase, can read.
- Every push is kept. The cloud is an append-only history, so any earlier push can be viewed again.
- Every meeting is pushed. There is no per-meeting exclusion.
- Out of scope: editing on the phone, LLM chat on the phone, recording on the phone, sharing between people, conflict resolution, restoring from the cloud back onto a laptop.

## The cast

The walkthrough below uses one person with three machines and a phone.

- **Laptop 1** is the personal laptop. It creates Yogurt Cloud once, and may also push its own meetings.
- **Laptop 2** and **Laptop 3** are other machines, possibly work laptops, each with its own local meetings to push.
- **The phone** reads everything.

Three credentials are involved, and each one shows up in as few places as possible.

| Credential | What it is | Where it is used |
| --- | --- | --- |
| Supabase account | The supabase.com login that owns the project and its billing. | Once, during setup on Laptop 1, through a personal access token held in memory. |
| Yogurt Cloud login | An email and password user inside the Supabase project. | Every laptop and the phone sign in with it. |
| Passphrase | Unlocks the private key that decrypts meetings. | Typed during setup on Laptop 1, and in the viewer. Never stored on a laptop. |

Local yogurt keeps no account of its own, as today.

## Architecture

```mermaid
flowchart LR
  subgraph L1["Laptop 1"]
    Y1["yogurt server<br/>public key only"]
  end
  subgraph L2["Laptop 2"]
    Y2["yogurt server<br/>public key only"]
  end
  subgraph L3["Laptop 3"]
    Y3["yogurt server<br/>public key only"]
  end
  subgraph SB["User's Supabase project = Yogurt Cloud"]
    AUTH["Auth<br/>Yogurt Cloud login"]
    DB[("Postgres + RLS<br/>devices, keyring, pushes, versions")]
  end
  MGMT["Supabase Management API"]
  GH["GitHub Pages<br/>static viewer build"]
  PH["Phone browser<br/>viewer + private key"]

  Y1 -- "setup, once, access token" --> MGMT
  MGMT -- "create project, schema, auth config" --> SB
  Y1 -- "push ciphertext" --> DB
  Y2 -- "push ciphertext" --> DB
  Y3 -- "push ciphertext" --> DB
  Y1 & Y2 & Y3 -. "sign in" .-> AUTH
  GH -- "serves JS, no data" --> PH
  PH -- "sign in, read ciphertext" --> SB
```

Every arrow into Supabase carries ciphertext or credentials, never readable meeting content.
GitHub Pages serves code only and never sees the project details, because they travel in the link's `#fragment`.

## Encryption

Setup creates one P-256 key pair per Yogurt Cloud.
Laptops hold the public key and can only encrypt.
The private key lives in Supabase wrapped by the passphrase, and only the viewer unwraps it.
A stolen or IT-managed work laptop therefore exposes its own local meetings, which it already had, and nothing from the other machines.

| Piece | Choice |
| --- | --- |
| Account key pair | ECDH P-256. WebCrypto supports it in every current browser, and RustCrypto's `p256` crate covers the laptop. |
| Per-meeting encryption | A fresh ephemeral P-256 key, ECDH with the account public key, HKDF-SHA256 (`info = "yogurt-sync v1"`) to an AES-256-GCM key, and a random 96-bit nonce. |
| Blob format | `ct` is base64 of `ephemeral public key (65 bytes) || nonce (12 bytes) || ciphertext`. |
| Associated data | `"<meeting id>|<device id>"`, so a blob swapped between rows fails to decrypt rather than showing the wrong meeting. |
| Private key at rest | PKCS#8 bytes, AES-256-GCM under a key from PBKDF2-HMAC-SHA256 (600,000 iterations, 16-byte salt), stored in `yogurt_keyring`. |
| Passphrase | 12 characters minimum, because the wrapped key sits in the database and anyone holding the login can attack it offline. |
| Plaintext | One JSON document per meeting: `{"v":1,"meeting":{...},"labels":[...],"chat":[...]}` with every `meetings` column, label names and colors, and chat messages. Device names are encrypted the same way, with the device id as associated data. |

The Rust side uses RustCrypto's `p256`, `hkdf`, `aes-gcm`, and `pbkdf2` crates, all MIT or Apache-2.0.
The viewer uses WebCrypto only, with no crypto library.
Checked-in fixtures produced by Rust (a wrapped private key and one encrypted meeting) are decrypted by a vitest test, so the two sides cannot drift.

`kdf` carries an `alg` field so a later move to Argon2id does not break existing clouds.
Changing the passphrase inserts a new `yogurt_keyring` row that wraps the same private key, and re-encrypts no meetings.
The viewer tries keyring rows newest first, so a junk row cannot lock it out.
A lost passphrase makes the cloud copy unreadable, history included.
Recovery is to drop the `yogurt_` tables in the Supabase SQL editor, run setup again on that project, and push again from each laptop, since every laptop still has its own local copy.

## Cloud schema

`cloud/schema.sql`, which setup runs automatically and which the manual fallback pastes into the SQL editor:

```sql
create table yogurt_devices (
  id         uuid primary key default gen_random_uuid(),
  user_id    uuid not null default auth.uid() references auth.users on delete cascade,
  name_ct    text not null,
  created_at timestamptz not null default now()
);

create table yogurt_keyring (
  id                  bigint generated always as identity primary key,
  user_id             uuid  not null default auth.uid() references auth.users on delete cascade,
  public_key          text  not null,  -- base64 raw P-256 point, not secret
  wrapped_private_key text  not null,
  kdf                 jsonb not null,  -- {"alg":"pbkdf2-sha256","iter":600000,"salt":"<b64>"}
  created_at          timestamptz not null default now()
);

-- Ids are global across devices, so "as of push N" covers every device at that moment.
create table yogurt_pushes (
  id        bigint generated always as identity primary key,
  device_id uuid not null references yogurt_devices,
  user_id   uuid not null default auth.uid() references auth.users on delete cascade,
  pushed_at timestamptz not null default now()
);

-- One row per meeting a push changed. A null ct marks the meeting deleted in that push.
create table yogurt_versions (
  meeting_id text   not null,  -- the local ULID
  push_id    bigint not null references yogurt_pushes,
  device_id  uuid   not null references yogurt_devices,
  user_id    uuid   not null default auth.uid() references auth.users on delete cascade,
  ct         text,
  primary key (meeting_id, push_id)
);

-- The library as of a push, or the latest library when at_push is null.
create function yogurt_as_of(at_push bigint default null)
returns setof yogurt_versions language sql stable security invoker as $$
  select * from (
    select distinct on (meeting_id) * from yogurt_versions
    where at_push is null or push_id <= at_push
    order by meeting_id, push_id desc
  ) latest
  where ct is not null
$$;

create view yogurt_push_log with (security_invoker = true) as
  select p.id, p.device_id, p.pushed_at,
         count(v.ct) as changed,
         count(*) filter (where v.ct is null) as deleted
  from yogurt_pushes p join yogurt_versions v on v.push_id = p.id
  group by p.id;

alter table yogurt_devices  enable row level security;
alter table yogurt_keyring  enable row level security;
alter table yogurt_pushes   enable row level security;
alter table yogurt_versions enable row level security;

-- Select and insert only: no client can update or delete, so history survives a compromised laptop.
create policy read on yogurt_devices  for select using (user_id = auth.uid());
create policy add  on yogurt_devices  for insert with check (user_id = auth.uid());
create policy read on yogurt_keyring  for select using (user_id = auth.uid());
create policy add  on yogurt_keyring  for insert with check (user_id = auth.uid());
create policy read on yogurt_pushes   for select using (user_id = auth.uid());
create policy add  on yogurt_pushes   for insert with check (user_id = auth.uid());
create policy read on yogurt_versions for select using (user_id = auth.uid());
create policy add  on yogurt_versions for insert with check (user_id = auth.uid());
```

The `yogurt_` prefix keeps these tables apart from anything else in a reused project.
The only plaintext in the cloud is the login email, row ids, push times, blob sizes, and the public key.
A schema without a version marker is v1. A later schema adds one when it needs it.

## The link code

Once a Yogurt Cloud exists, Settings shows a link code and a QR code.
Both carry the same three non-secret values: the project URL, the publishable key, and the public key.

- The link code is `yogurt-cloud:` followed by base64url JSON of those values. Laptops 2 and 3 paste it.
- The QR code is the viewer URL with the same payload in the fragment, `https://<viewer>/#c=<payload>`. Browsers never send the fragment to GitHub Pages.

The viewer's own settings page shows the link code too, so a new laptop can be connected from the phone without Laptop 1 nearby.

## Flow 1: Laptop 1 creates Yogurt Cloud (once)

The user signs up at supabase.com and creates a personal access token on the account tokens page.
Everything after that happens in yogurt's Settings, under Cloud sync, then "Set up a new Yogurt Cloud".

```mermaid
sequenceDiagram
  autonumber
  actor U as User
  participant Y as yogurt on Laptop 1
  participant M as Management API
  participant P as Supabase project

  U->>Y: access token
  Y->>M: GET /v1/organizations
  Y->>M: GET /v1/projects
  Y-->>U: existing projects + "Create a new project"
  U->>Y: project choice, login email + password, passphrase
  alt new project
    Y->>M: POST /v1/projects {name, organization_slug, db_pass: random}
    loop until status = ACTIVE_HEALTHY
      Y->>M: GET /v1/projects/{ref}
    end
  end
  Y->>M: POST /v1/projects/{ref}/database/query (schema.sql)
  opt turn off public sign-ups (checked by default)
    Y->>M: PATCH /v1/projects/{ref}/config/auth {disable_signup: true}
  end
  Y->>M: GET /v1/projects/{ref}/api-keys?reveal=true
  Y->>P: POST /auth/v1/admin/users {email, password, email_confirm: true} (secret key)
  Y->>P: POST /auth/v1/token?grant_type=password
  Y->>Y: generate P-256 key pair, wrap private key with passphrase
  Y->>P: insert yogurt_keyring row
  Y->>Y: forget token, secret key, passphrase, private key
  Y-->>U: link code + QR, "you can revoke the token now"
```

All endpoints above appear in the Management API's OpenAPI spec (`https://api.supabase.com/api/v1-json`, checked 2026-10-21).
`database/query` is marked Beta there, so if it fails, setup shows `schema.sql` with a copy button and a link to the project's SQL editor, then continues once the user confirms.
The random database password is never stored, because yogurt never connects to Postgres directly. The dashboard can reset it.

Setup always offers reuse, so the free plan's two-project cap needs no separate fallback.
On a reused project, setup first checks for `yogurt_keyring`. If it exists, the project already holds a Yogurt Cloud, and setup points to Connect instead.
Turning off public sign-ups applies to the whole project, which could break another app that lives there.
On a new project the step just runs. On a reused one it shows as a checkbox, checked by default, with that warning.
Leaving sign-ups on keeps the data safe, because RLS keeps every other user out of these rows. It only lets strangers sign up and use the project's storage.

If Laptop 1 also records meetings, the same screen offers "Also push this laptop's meetings" with a device name, which runs Flow 3's last steps.

## Flow 2: the phone becomes the reader

```mermaid
sequenceDiagram
  autonumber
  actor U as User
  participant V as Viewer in phone browser
  participant P as Supabase project

  U->>V: scan QR on Laptop 1
  V->>V: read #c= (URL, publishable key, public key), save in browser
  U->>V: login email + password
  V->>P: sign in
  V->>P: select yogurt_keyring, newest first
  U->>V: passphrase
  V->>V: PBKDF2, unwrap private key, store non-extractable in IndexedDB
  V->>P: select yogurt_devices, rpc yogurt_as_of()
  V->>V: decrypt, group by day, device picker = All devices
```

Any browser can open the same viewer, Laptop 1's included.
The viewer remembers the unlocked key in that browser until Lock in the viewer's settings, so later visits skip the passphrase.

## Flow 3: Laptops 2 and 3 connect

In yogurt's Settings, under Cloud sync, then "Connect to a Yogurt Cloud":

```mermaid
sequenceDiagram
  autonumber
  actor U as User
  participant Y as yogurt on Laptop 2
  participant P as Supabase project

  U->>Y: link code, login email + password, device name "Laptop 2"
  Y->>P: POST /auth/v1/token?grant_type=password
  Y->>P: select yogurt_keyring.public_key, compare with link code
  Y->>P: insert yogurt_devices {name_ct}
  Y->>Y: keys.json: refresh token, URL, publishable key, public key, device id
  U->>Y: Push meetings
```

The public key check catches a link code from a different Yogurt Cloud than the login belongs to.
Laptop 2 never sees a Supabase token, a secret key, or the passphrase.

Disconnect removes those `keys.json` entries.
It first offers one last push that marks every meeting from this device deleted, so the current library drops them while history keeps them.

## Flow 4: pushing

The Push meetings button in Settings runs one pass. SYNC-3 adds an interval timer that runs the same pass.

```mermaid
sequenceDiagram
  autonumber
  participant Y as yogurt on Laptop 2
  participant DB as Local SQLite
  participant P as Supabase project

  Y->>P: refresh session if needed
  Y->>DB: every meeting except the one recording
  Y->>Y: plaintext JSON + sha256 each, keep those that differ from cloud_hash
  Y->>P: rpc yogurt_as_of(), select meeting_id where device_id = me
  Y->>Y: deleted = cloud ids missing locally
  alt anything changed or deleted
    Y->>P: insert yogurt_pushes {device_id}, returning id
    loop batches
      Y->>Y: encrypt changes to public key, AAD = id|device
      Y->>P: insert yogurt_versions (null ct for each deletion)
      Y->>DB: store the batch's new cloud_hash values
    end
  end
```

Hashing the plaintext catches every change, including label and chat edits that do not touch `meetings.updated_at`.
`meetings.cloud_hash` is one new nullable column, so it needs the usual shared-DB migration care while the branch is unmerged.
The id listing reads metadata only and no content.
A pass that changes nothing writes nothing.
If a batch fails, its meetings keep their old `cloud_hash`, so the next pass sends them again. A push row with no versions never shows in `yogurt_push_log`.

## History and reverting

The cloud keeps every push instead of one overwritten copy.
A push stores only the meetings that changed since that device's last push, plus a deletion marker for each meeting the device no longer has.
The library as of push N is the latest version of each meeting at or before N, which `yogurt_as_of(N)` returns.
Push ids are global across devices, so picking a push shows every device's library at that moment.

A full snapshot per push gives the same result at far higher cost.
With hourly auto-push, a 630 KB snapshot every hour is about 15 MB a day, which would fill the free tier's 500 MB in about a month.

Reverting is read-only, because sync is one-way.
The viewer's History list shows each push with its device, time, and changed and deleted counts, and picking one shows the library as it was then.
Nothing flows back to a laptop, and the next push continues from the laptop's local state.
That covers a local reset: a wiped `~/.yogurt/db.sqlite` with the connection intact makes the next pass mark every meeting from that device deleted, and they stay readable from any earlier push.

The policies allow select and insert only, so no laptop or browser can update or delete a row.
Purging data for real, for example a work laptop's meetings, is done in the Supabase SQL editor.

Size check against the current `~/.yogurt/db.sqlite`: 45 meetings total 630 KB, and the largest (1 hour 58 minutes) is 148 KB.
With base64 and the 77-byte header, an average version is about 19 KB, so the free tier's 500 MB holds about 26,000 versions.
A meeting gets a new version only when it changes after its first push, so that is thousands of meetings with their edit history.
SYNC-1 prunes nothing. If the database nears the cap, the upgrade is a `pg_cron` job that drops versions older than a set age while keeping each meeting's latest.

## What each place holds

| | Supabase access token | Cloud login | Public key | Private key |
| --- | --- | --- | --- | --- |
| Laptop 1 | in memory during setup only | refresh token, if it pushes | yes | in memory during setup only |
| Laptop 2, Laptop 3 | never | refresh token | yes | never |
| Viewer (phone or any browser) | never | session | yes | unwrapped, non-extractable, after the passphrase |
| Supabase | n/a | stores the login | stores it | wrapped by the passphrase |

Laptop secrets live in `~/.yogurt/keys.json` (mode 0600, `FileKeyStore`), never in SQLite or a log.

A compromised laptop still holds a cloud login and the public key, and it cannot read anything.
It can add junk rows, deletion markers, or forged meetings that decrypt, because the public key is all an encrypter needs.
It cannot erase history, so picking a push from before the damage shows the real library.
Per-device signing would catch forged meetings, and is left out until that threat matters.

Some corporate networks block `*.supabase.co`, which would stop a work laptop from pushing, and nothing on yogurt's side fixes that.

## UI

The mocks are in the review surface linked at the top.

- **Settings, Cloud sync, nothing set up.** Two cards: "Set up a new Yogurt Cloud" and "Connect to a Yogurt Cloud".
- **Setup.** Token, then a project picker (existing projects or a new one), login, passphrase, then a checklist that ticks off each Flow 1 step, with the SQL fallback inline.
- **Connected.** Device name, Push meetings, last push result, link code with copy, QR for the phone, Disconnect.
- **Connect.** Link code, login, device name.
- **Library subtitle.** "46 meetings · all on this Mac" gains "· 3 not pushed" when connected and behind.
- **Viewer.** Unlock, library with the device picker, a read-only meeting page, and History. A clock button in the library header opens the list of pushes, and picking one shows the library as of that push under a "Viewing <time>" banner with a Latest button.

## Viewer

A second Vite entry in `web/` (`web/cloud.html`) builds the static viewer.
It reuses the existing markdown and transcript components and makes no calls to the local API.
It uses `@supabase/supabase-js` (MIT) for sign-in and session refresh.
It downloads and decrypts the current library from `yogurt_as_of()` on open. That is fine at personal scale. When it gets slow, the upgrade is an IndexedDB cache that fetches only versions with a higher push id.
History reads `yogurt_push_log`, and picking a push calls `yogurt_as_of(N)`. A banner shows the push being viewed, with a button back to the latest.

The default host is one public GitHub Pages build deployed from `main`.
Whoever controls that host controls the JS that sees decrypted meetings, so the docs also give a one-click deploy of the same build for anyone who wants their own copy.

Email and password is the only sign-in in v1.
Supabase's Google provider needs a Google Cloud OAuth client that setup cannot create, so it stays a manual, optional step in the docs.

## Constraint changes

`AGENTS.md`'s first hard constraint becomes: audio never leaves the machine unless the user opted into cloud STT (then only audio), and notes and transcripts leave only when the user connects a Yogurt Cloud in a Supabase project they own, and only as ciphertext the laptop itself cannot decrypt.
The other constraints are unchanged:

- Zero telemetry still holds, because sync sends nothing to yogurt or to any third party the user did not configure.
- The one-process rule still holds, because sync runs inside the server.
- The license rule still holds, because the new crates are MIT or Apache-2.0.

The archived PRD's "There is no Yogurt cloud" still holds too, because yogurt hosts nothing.

## Supabase free tier

Third-party sources agree on 500 MB of database per project, 2 free projects, and a pause after one week of inactivity that needs a manual restore.
Whether API writes count as activity is unverified.
Pro ($25 a month) removes the pause.
Verify against Supabase's own pricing page before SYNC-1 lands.

## Phasing

1. **SYNC-1.** `cloud/schema.sql` with the versioned tables, the crypto with its fixtures, Flow 1 setup with the project picker and the SQL fallback, Flow 3 connect and disconnect, the link code, manual push, the `cloud_hash` migration, and the `AGENTS.md` edit.
2. **SYNC-2.** The viewer with History, the QR code, and the GitHub Pages deploy.
3. **SYNC-3.** Automatic push on an interval (15 minutes, 1 hour, or daily).

Later and unplanned: a hosted Yogurt Cloud, so other people could push without their own Supabase project.
Nothing in SYNC-1 to SYNC-3 blocks it, because the laptops and the viewer only need a URL and a public key.

## Decisions

Settled 2026-10-24.

- Every meeting is pushed, with no per-meeting exclusion.
- A local reset marks that device's meetings deleted in the next push, and history keeps them readable (see History and reverting).
- The viewer remembers the unlocked key per browser until Lock.
- Setup offers every existing project for reuse, alongside creating a new one.
