-- Rows written under main's schema, before this branch's schema is applied.
insert into auth.users values ('aaaaaaaa-0000-0000-0000-000000000001', 'owner@example.com', now());
insert into yogurt_owner (email, user_id, passcode_hash) values ('owner@example.com', 'aaaaaaaa-0000-0000-0000-000000000001', extensions.crypt('123456', extensions.gen_salt('bf')));
insert into yogurt_devices (id, user_id, name) values ('00000000-0000-0000-0000-0000000000d1', 'aaaaaaaa-0000-0000-0000-000000000001', 'Laptop');
insert into yogurt_pushes (device_id, user_id) values ('00000000-0000-0000-0000-0000000000d1', 'aaaaaaaa-0000-0000-0000-000000000001');
insert into yogurt_versions (meeting_id, push_id, device_id, user_id, body)
select m, 1, '00000000-0000-0000-0000-0000000000d1', 'aaaaaaaa-0000-0000-0000-000000000001', b
from (values
  ('u1', jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Before upgrade', 'started_at', 1000, 'starred', true,
    'labels', jsonb_build_array(jsonb_build_object('id', 'lu', 'name', 'Upgraded', 'color', 'blue')), 'notes_md', 'upgradeword', 'transcript_json', '[]'))),
  ('u2', jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Huge', 'started_at', 2000, 'notes_md', 'hugeword ' || repeat('日本語 é€ ', 400000), 'transcript_json', '[]'))),
  ('u4', jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Archived', 'started_at', 500, 'archived_at', '2026-10-27T08:00:00Z', 'notes_md', 'upgradeword', 'transcript_json', '[]'))),
  ('u3', null)
) t(m, b);
