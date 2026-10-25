# Yogurt Cloud setup guide

This is a draft.
Each SYNC ticket adds its own section as it lands, and SYNC-7 finishes the guide with a rehearsal on real accounts.
Headings marked "not written yet" belong to a later ticket.

Yogurt Cloud lets you read your meetings on your phone.
It is a webapp on your own Vercel account, backed by your own Supabase project.
Yogurt hosts nothing, and nothing leaves your Mac until you press Push meetings in Settings.
The design is in [docs/.planning/sync-cloud-mirror-design.md](.planning/sync-cloud-mirror-design.md).

## 1. What Yogurt Cloud is, and who can read it

Not written yet (SYNC-7).

## 2. Deploy your Yogurt Cloud

Not written yet (SYNC-7).
It will cover the Deploy with Vercel button, the Supabase integration, and the owner email.

## 3. First visit to the webapp

Not written yet (SYNC-3 and SYNC-7).
It will cover creating the Cloud login, choosing a PIN, and adding the webapp to your phone's home screen.

## 4. Connect each laptop

Do this on every Mac that should push its meetings.
You need the Cloud address of your webapp and the Cloud login email and password.
You must have chosen a PIN in the webapp first, or Yogurt will say "Choose a PIN in the webapp first".

1. Open Settings, then Cloud sync, then Connect.
2. Type the Cloud address, for example `richard-yogurt.vercel.app`.
   When you leave the field, Yogurt says "Found a Yogurt Cloud" if the address serves a Yogurt Cloud config.
3. Type the Cloud login email and password, and a machine name such as "Work laptop".
   The webapp's device picker shows the machine name.
4. Select Connect.
   Yogurt signs in once and keeps only a refresh token in `~/.yogurt/keys.json`.
   It never stores the password, and it never sees your PIN.

If the login does not own that Yogurt Cloud, Yogurt says "This login does not own that Yogurt Cloud".

## 5. Push your meetings

Select Push meetings on the Cloud sync page.
The first push sends every meeting, and later pushes send only the meetings you changed, plus a deletion marker for each meeting you deleted.
The meeting you are recording right now is skipped until it ends.
The page shows how many meetings are not pushed and the result of the last push.

The Library header shows a "N not pushed" pill while this Mac is connected and behind.
Select the pill to open the Cloud sync page.
You can also run `yogurt ctl cloud status` and `yogurt ctl cloud push` from a terminal.

If the page says "Your Cloud session expired", type the Cloud login password into the field under the message and select Sign in.
Your meetings are not affected.

### Push automatically

The Auto-push control on the Cloud sync page chooses how often Yogurt pushes by itself: Off, 15 min, Hourly, or Daily.
It starts Off, and a new connection starts Off again, so nothing leaves this Mac until you choose an interval.
Yogurt checks once a minute while it is running.
It pushes when the interval has passed since the last successful push, or when nothing has been pushed yet.
Automatic and manual pushes never run at the same time, and the meeting you are recording is skipped as usual.
Yogurt must be running, because there is no background service.
A push missed while Yogurt was closed happens within a minute of the next start.

If an automatic push fails, the page shows the error under the last push, like a failed manual push.
Yogurt tries again after 5 minutes.
If the error is one only you can fix, such as "Your Cloud session expired" or "This machine was removed from the Yogurt Cloud", Yogurt stops trying.
It resumes after you sign in again, connect again, or push by hand successfully.
When nothing has changed, a check costs no network call.
`yogurt ctl cloud status` prints the interval on its `auto-push:` line.

## 6. Disconnect a laptop

Select Disconnect on the Cloud sync page.
You can first run one last push that marks this machine's meetings deleted, so the webapp's current library drops them.
History in the webapp keeps them either way.
Disconnecting removes the connection from `~/.yogurt/keys.json` and signs this Mac out of the Cloud.

## 7. History and recovery

Not written yet (SYNC-5 and SYNC-7).

## 8. Troubleshooting

Not written yet (SYNC-7).
