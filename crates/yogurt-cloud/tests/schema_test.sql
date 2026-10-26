-- The just recipe prepares set_owner(text) from owner.sql ahead of this file.
\set ON_ERROR_STOP on
\set QUIET on
\o /dev/null

-- Asserts that each statement fails with the given sqlstate when run as the current role.
create or replace function pg_temp.expect(want text, stmts text[]) returns void language plpgsql as $$
declare
  stmt text;
  state text;
begin
  foreach stmt in array stmts loop
    state := null;
    begin
      execute stmt;
    exception when others then
      state := sqlstate;
    end;
    if state is distinct from want then
      raise exception 'expected % for "%", got %', want, stmt, coalesce(state, 'success');
    end if;
  end loop;
end $$;
grant execute on function pg_temp.expect(text, text[]) to public;

create or replace function pg_temp.eq(got text, want text, what text) returns void language plpgsql as $$
begin
  if got is distinct from want then raise exception '%: expected %, got %', what, want, got; end if;
end $$;
grant execute on function pg_temp.eq(text, text, text) to public;

\set o '''aaaaaaaa-0000-0000-0000-000000000001'''
\set n '''bbbbbbbb-0000-0000-0000-000000000002'''
\set u '''cccccccc-0000-0000-0000-000000000003'''
\set dup '''dddddddd-0000-0000-0000-000000000004'''
\set d1 '''00000000-0000-0000-0000-0000000000d1'''
\set d2 '''00000000-0000-0000-0000-0000000000d2'''

truncate auth.users, yogurt_owner, yogurt_browsers, yogurt_devices, yogurt_pushes, yogurt_versions restart identity cascade;

-- Ownership: only a confirmed user with the owner email counts, and the first one is pinned.
insert into auth.users values (:u, 'owner@example.com', null), (:n, 'someone@example.com', now());
execute set_owner('other@example.com');
select pg_temp.eq((select email from yogurt_owner), 'other@example.com', 'email before pinning');
execute set_owner('Owner@Example.com');
select pg_temp.eq((select email from yogurt_owner), 'Owner@Example.com', 'email before pinning');

set role authenticated;
set request.jwt.claim.sub = 'cccccccc-0000-0000-0000-000000000003';
select pg_temp.eq((yogurt_status()->>'is_owner'), 'false', 'unconfirmed match is not owner');
select pg_temp.eq((yogurt_status()->>'owner_email_set'), 'true', 'owner email set');
set request.jwt.claim.sub = 'bbbbbbbb-0000-0000-0000-000000000002';
select pg_temp.eq((yogurt_status()->>'is_owner'), 'false', 'other user is not owner');
reset role;
select pg_temp.eq((select user_id::text from yogurt_owner), null, 'nothing pinned yet');

insert into auth.users values (:o, 'owner@example.com', now());
set role authenticated;
set request.jwt.claim.sub = 'aaaaaaaa-0000-0000-0000-000000000001';
select pg_temp.eq((yogurt_status()->>'is_owner'), 'true', 'case-insensitive confirmed match is owner');
select pg_temp.eq((yogurt_status()->>'passcode_set'), 'false', 'no passcode yet');
reset role;
select pg_temp.eq((select user_id::text from yogurt_owner), 'aaaaaaaa-0000-0000-0000-000000000001', 'pinned');
execute set_owner('changed@example.com');
select pg_temp.eq((select email from yogurt_owner), 'Owner@Example.com', 'email is fixed once pinned');

insert into auth.users values (:dup, 'OWNER@example.com', now());
set role authenticated;
set request.jwt.claim.sub = 'dddddddd-0000-0000-0000-000000000004';
select pg_temp.eq((yogurt_status()->>'is_owner'), 'false', 'a later confirmed twin is not owner');

-- A non-owner can neither insert nor call anything except yogurt_status.
set request.jwt.claim.sub = 'bbbbbbbb-0000-0000-0000-000000000002';
select pg_temp.expect('42501', array[
  'select yogurt_set_passcode(''long enough passcode'')',
  'select yogurt_unlock(''long enough passcode'')',
  'select yogurt_lock(''x'')',
  'select yogurt_forget_browser(''x'', 1)',
  'select * from yogurt_browsers(''x'')',
  'select yogurt_start_push(''00000000-0000-0000-0000-0000000000d1'')',
  'select * from yogurt_device_meetings(''00000000-0000-0000-0000-0000000000d1'')',
  'select * from yogurt_library(''x'')',
  'select * from yogurt_search(''x'', ''x'')',
  'select * from yogurt_history(''x'')',
  'select * from yogurt_device_list(''x'')',
  'insert into yogurt_devices (id, name) values (gen_random_uuid(), ''evil'')',
  'insert into yogurt_versions (meeting_id, push_id, device_id, body) values (''m1'', 1, ''00000000-0000-0000-0000-0000000000d1'', ''{}'')'
]);

select pg_temp.expect('42883', array[
  'select public.yogurt_is_owner()', 'select public.yogurt_note_failure()', 'select public.yogurt_check_token(''x'')',
  'select public.yogurt_require_owner()'
]);
select pg_temp.expect('42501', array[
  'select yogurt_private.yogurt_note_failure()', 'select yogurt_private.yogurt_check_token(''x'')',
  'select yogurt_private.yogurt_require_owner()'
]);

-- The API roles can execute exactly the public RPCs and the three policy helpers.
reset role;
select pg_temp.eq((select string_agg(proname, ',' order by proname) from pg_proc
  where pronamespace = 'public'::regnamespace and proname like 'yogurt\_%' and has_function_privilege('authenticated', oid, 'execute')),
  'yogurt_browsers,yogurt_device_list,yogurt_device_meetings,yogurt_forget_browser,yogurt_history,yogurt_library,yogurt_lock,yogurt_search,yogurt_set_passcode,yogurt_start_push,yogurt_status,yogurt_unlock',
  'public allowlist');
select pg_temp.eq((select string_agg(proname, ',' order by proname) from pg_proc
  where pronamespace = 'yogurt_private'::regnamespace and has_function_privilege('authenticated', oid, 'execute')),
  'yogurt_is_owner,yogurt_owns_device,yogurt_push_open', 'private allowlist');
select pg_temp.eq((select count(*)::text from pg_proc
  where pronamespace in ('public'::regnamespace, 'yogurt_private'::regnamespace) and proname like 'yogurt\_%'
    and has_function_privilege('anon', oid, 'execute')), '0', 'anon executes nothing');

-- anon can do nothing, not even status.
reset role;
set role anon;
select pg_temp.expect('42501', array[
  'select yogurt_status()',
  'select yogurt_unlock(''x'')',
  'select yogurt_private.yogurt_is_owner()',
  'select yogurt_private.yogurt_note_failure()',
  'select count(*) from yogurt_owner',
  'insert into yogurt_devices (id, name) values (gen_random_uuid(), ''evil'')'
]);

-- The owner: passcode, devices, and pushes.
reset role;
set role authenticated;
set request.jwt.claim.sub = 'aaaaaaaa-0000-0000-0000-000000000001';
select pg_temp.expect('22023', array[
  'select yogurt_set_passcode(''123'')', 'select yogurt_set_passcode(''12345'')', 'select yogurt_set_passcode(''1234567'')',
  'select yogurt_set_passcode(''abcdef'')', 'select yogurt_set_passcode(''12 456'')', 'select yogurt_set_passcode(null)'
]);
select pg_temp.expect('PT412', array['select yogurt_start_push(gen_random_uuid())']);
select pg_temp.eq(yogurt_set_passcode('123456')::text, 'true', 'first passcode');
select pg_temp.eq((yogurt_status()->>'passcode_set'), 'true', 'passcode set');
select pg_temp.eq((yogurt_status()->>'passcode_length'), '6', 'passcode length recorded');

insert into yogurt_devices (id, name) values (:d1, 'Laptop 1'), (:d2, 'Laptop 2');
select pg_temp.expect('23502', array['insert into yogurt_devices (name) values (''x'')']);

select yogurt_start_push(:d1) as p1 \gset
select pg_temp.eq(:p1::text, '1', 'first push id');
insert into yogurt_versions (meeting_id, push_id, device_id, body) values
  ('m1', 1, :d1, '{"t":"v1"}'), ('m2', 1, :d1, '{"t":"v1"}'), ('m3', 1, :d1, '{"t":"v1"}');
select yogurt_start_push(:d2);
insert into yogurt_versions (meeting_id, push_id, device_id, body) values
  ('m1', 2, :d2, '{"t":"v2"}'), ('m5', 2, :d2, '{"t":"v1"}');
select yogurt_start_push(:d1);
insert into yogurt_versions (meeting_id, push_id, device_id, body) values
  ('m1', 3, :d1, '{"t":"v1b"}'), ('m3', 3, :d1, null), ('m4', 3, :d1, '{"t":"v1"}');
select yogurt_start_push(:d1);  -- push 4 stays empty

-- Not even the owner reads a table, and nothing is writable beyond the granted columns.
select pg_temp.expect('42501', array[
  'select count(*) from yogurt_owner',
  'select count(*) from yogurt_browsers',
  'select count(*) from yogurt_devices',
  'select count(*) from yogurt_pushes',
  'select count(*) from yogurt_versions',
  'insert into yogurt_devices (id, name) values (gen_random_uuid(), ''x'') returning id',
  'insert into yogurt_pushes (device_id) values (''00000000-0000-0000-0000-0000000000d1'')',
  'insert into yogurt_pushes (id, device_id) overriding system value values (99, ''00000000-0000-0000-0000-0000000000d1'')',
  'insert into yogurt_devices (id, name, created_at) values (gen_random_uuid(), ''x'', now())',
  'insert into yogurt_devices (id, name, user_id) values (gen_random_uuid(), ''x'', ''bbbbbbbb-0000-0000-0000-000000000002'')',
  'insert into yogurt_versions (meeting_id, push_id, device_id, body, user_id) values (''m9'', 3, ''00000000-0000-0000-0000-0000000000d1'', ''{}'', ''bbbbbbbb-0000-0000-0000-000000000002'')',
  'insert into yogurt_owner (email) values (''x'')',
  'update yogurt_owner set passcode_hash = null',
  'update yogurt_devices set name = ''x''',
  'delete from yogurt_devices',
  'update yogurt_pushes set pushed_at = now()',
  'delete from yogurt_pushes',
  'update yogurt_versions set body = null',
  'delete from yogurt_versions',
  'delete from yogurt_browsers',
  'truncate yogurt_versions',
  'truncate yogurt_devices',
  'select yogurt_private.yogurt_note_failure()',
  'select yogurt_private.yogurt_check_token(''x'')',
  'select yogurt_private.yogurt_require_owner()'
]);
select pg_temp.expect('PT404', array[
  'select yogurt_start_push(gen_random_uuid())',
  'select * from yogurt_device_meetings(gen_random_uuid())'
]);
select pg_temp.expect('42883', array[
  'select public.yogurt_note_failure()',
  'select public.yogurt_check_token(''x'')',
  'select public.yogurt_require_owner()',
  'select public.yogurt_is_owner()',
  'select public.yogurt_owns_device(gen_random_uuid())',
  'select public.yogurt_push_open(1, gen_random_uuid())'
]);

-- A version needs a push of its own device from the last hour.
reset role;
insert into yogurt_pushes (id, device_id, user_id, pushed_at) overriding system value
  values (100, :d1, :o, now() - interval '2 hours');
set role authenticated;
select pg_temp.expect('42501', array[
  'insert into yogurt_versions (meeting_id, push_id, device_id, body) values (''m1'', 100, ''00000000-0000-0000-0000-0000000000d1'', ''{"forged":1}'')',
  'insert into yogurt_versions (meeting_id, push_id, device_id, body) values (''m1'', 3, ''00000000-0000-0000-0000-0000000000d2'', ''{"forged":1}'')',
  'insert into yogurt_versions (meeting_id, push_id, device_id, body) values (''m1'', 4242, ''00000000-0000-0000-0000-0000000000d1'', ''{"forged":1}'')'
]);

select pg_temp.eq((select string_agg(x, ',') from yogurt_device_meetings(:d1) x), 'm1,m2,m4', 'device 1 meetings');
select pg_temp.eq((select string_agg(x, ',') from yogurt_device_meetings(:d2) x), 'm1,m5', 'device 2 meetings');

-- Unlock.
select pg_temp.expect('28000', array[
  'select * from yogurt_library(''no such token'')',
  'select * from yogurt_library(null)',
  'select * from yogurt_search(''no such token'', ''x'')',
  'select * from yogurt_search(null, ''x'')',
  'select * from yogurt_history(''no such token'')',
  'select * from yogurt_device_list(''no such token'')',
  'select * from yogurt_browsers(''no such token'')',
  'select yogurt_forget_browser(''no such token'', 1)'
]);
select pg_temp.eq(yogurt_unlock('wrong passcode', false, 'Safari'), null, 'wrong passcode');
select yogurt_unlock('123456', false, 'Safari on iPhone') as tok \gset
select pg_temp.eq(length(:'tok')::text, '64', 'token is 32 bytes of hex');

select pg_temp.eq((select string_agg(l.device_id::text || ':' || l.meeting_id || ':' || (l.body->>'t'), ',' order by ord)
  from (select *, row_number() over () ord from yogurt_library(:'tok')) l),
  '00000000-0000-0000-0000-0000000000d1:m1:v1b,00000000-0000-0000-0000-0000000000d2:m1:v2,00000000-0000-0000-0000-0000000000d1:m2:v1,00000000-0000-0000-0000-0000000000d1:m4:v1,00000000-0000-0000-0000-0000000000d2:m5:v1',
  'latest library');
select pg_temp.eq((select string_agg(l.meeting_id || (case when l.device_id = :d1 then 'a' else 'b' end) || l.push_id, ',') from yogurt_library(:'tok', 1) l),
  'm1a1,m2a1,m3a1', 'library as of push 1');
select pg_temp.eq((select string_agg(l.meeting_id || (case when l.device_id = :d1 then 'a' else 'b' end) || l.push_id, ',') from yogurt_library(:'tok', 2) l),
  'm1a1,m1b2,m2a1,m3a1,m5b2', 'library as of push 2');
select pg_temp.eq((select count(*)::text from yogurt_library(:'tok', 0)), '0', 'library before any push');
select pg_temp.eq((select pushed_at is not null from yogurt_library(:'tok', 1) limit 1)::text, 'true', 'pushed_at present');
select pg_temp.eq((select string_agg(h.push_id || ':' || h.device_name || ':' || h.changed || '/' || h.deleted, ',') from yogurt_history(:'tok') h),
  '3:Laptop 1:2/1,2:Laptop 2:2/0,1:Laptop 1:3/0', 'history skips empty pushes, newest first');
select pg_temp.eq((select string_agg(d.name, ',') from yogurt_device_list(:'tok') d), 'Laptop 1,Laptop 2', 'device list');

-- Remember lasts 90 days, a plain unlock 12 hours, and only a hash is stored.
select yogurt_unlock('123456', true, 'Chrome on Mac') as tok2 \gset
reset role;
select pg_temp.eq((select count(*)::text from yogurt_browsers where token_hash = sha256(convert_to(:'tok', 'UTF8'))), '1', 'token hash stored');
select pg_temp.eq((select (expires_at - created_at between interval '11 hours 59 minutes' and interval '12 hours')::text from yogurt_browsers where label = 'Safari on iPhone'), 'true', '12 hours');
select pg_temp.eq((select (expires_at - created_at between interval '89 days 23 hours' and interval '90 days')::text from yogurt_browsers where label = 'Chrome on Mac'), 'true', '90 days');
set role authenticated;
select pg_temp.eq((select string_agg(b.label || ':' || b.current, ',' order by b.id) from yogurt_browsers(:'tok') b), 'Safari on iPhone:true,Chrome on Mac:false', 'browser list');

-- Lock, Forget, and expiry.
select pg_temp.eq(yogurt_lock(:'tok2')::text, 'true', 'lock');
select pg_temp.expect('28000', array[format('select * from yogurt_library(%L)', :'tok2')]);
select yogurt_unlock('123456', false, 'Firefox') as tok3 \gset
select pg_temp.eq(yogurt_forget_browser(:'tok', (select b.id from yogurt_browsers(:'tok') b where b.label = 'Firefox'))::text, 'true', 'forget');
select pg_temp.expect('28000', array[format('select * from yogurt_library(%L)', :'tok3')]);
reset role;
update yogurt_browsers set expires_at = now() - interval '1 second';
set role authenticated;
select pg_temp.expect('28000', array[format('select * from yogurt_library(%L)', :'tok')]);

-- Five wrong passcodes lock unlocking for 15 minutes, even against the right one, and a success resets the count.
select yogurt_unlock('nope 1'), yogurt_unlock('nope 2'), yogurt_unlock('nope 3'), yogurt_unlock('nope 4');
select pg_temp.eq((yogurt_status()->>'locked_until'), null, 'four failures do not lock');
select yogurt_unlock('123456', false, 'Safari') as tok \gset
select yogurt_unlock('nope 1'), yogurt_unlock('nope 2'), yogurt_unlock('nope 3'), yogurt_unlock('nope 4');
select pg_temp.eq((yogurt_status()->>'locked_until'), null, 'a success reset the count');
select yogurt_unlock('nope 5');
select pg_temp.eq(((yogurt_status()->>'locked_until') is not null)::text, 'true', 'five failures lock');
select pg_temp.eq(yogurt_unlock('123456', false, 'Safari'), null, 'right passcode during lockout');
reset role;
select pg_temp.eq(((select locked_until from yogurt_owner) between now() + interval '14 minutes' and now() + interval '15 minutes')::text, 'true', '15 minutes');
update yogurt_owner set locked_until = now() - interval '1 second';
set role authenticated;
select pg_temp.eq((yogurt_status()->>'locked_until'), null, 'lockout expired');
select yogurt_unlock('nope 1'), yogurt_unlock('nope 2'), yogurt_unlock('nope 3'), yogurt_unlock('nope 4'), yogurt_unlock('nope 5');
reset role;
select pg_temp.eq(((select locked_until from yogurt_owner) between now() + interval '29 minutes' and now() + interval '30 minutes')::text, 'true', 'the second lockout is twice as long');
select pg_temp.eq((select consecutive_lockouts::text from yogurt_owner), '2', 'two lockouts in a row');
update yogurt_owner set locked_until = now() - interval '1 second';
set role authenticated;
select yogurt_unlock('123456', false, 'Safari') as tok \gset
select pg_temp.eq((:'tok' <> '')::text, 'true', 'unlock after lockout expiry');
reset role;
select pg_temp.eq((select consecutive_lockouts || '/' || failed_attempts from yogurt_owner), '0/0', 'a correct passcode resets both counters');
set role authenticated;

-- A rejected format never touches the failure counter, even with a wrong old passcode.
select pg_temp.expect('22023', array['select yogurt_set_passcode(''12345'', ''wrong old'')', 'select yogurt_set_passcode(''abcdef'', ''wrong old'')']);
reset role;
select pg_temp.eq((select failed_attempts::text from yogurt_owner), '0', 'format errors are not failures');
set role authenticated;

-- A wrong old passcode counts toward the same lockout, and set_passcode respects it.
select pg_temp.eq(yogurt_set_passcode('654321', 'wrong old')::text, 'false', 'wrong old passcode');
select pg_temp.eq(yogurt_set_passcode('654321')::text, 'false', 'missing old passcode');
select pg_temp.eq((select count(*)::text from yogurt_browsers(:'tok')), '2', 'failed change keeps browsers');
select yogurt_set_passcode('654321', 'wrong 3'), yogurt_set_passcode('654321', 'wrong 4'), yogurt_set_passcode('654321', 'wrong 5');
select pg_temp.eq(((yogurt_status()->>'locked_until') is not null)::text, 'true', 'wrong old passcodes lock');
select pg_temp.eq(yogurt_set_passcode('654321', '123456')::text, 'false', 'right old passcode during lockout');
select pg_temp.eq(yogurt_unlock('123456', false, 'Safari'), null, 'unlock during set_passcode lockout');
reset role;
update yogurt_owner set locked_until = null, failed_attempts = 0;
set role authenticated;

-- The two functions share one counter, so alternating between them locks both on the fifth failure.
select yogurt_unlock('x'), yogurt_set_passcode('654321', 'x'), yogurt_unlock('x'), yogurt_set_passcode('654321', 'x');
select pg_temp.eq((yogurt_status()->>'locked_until'), null, 'four mixed failures do not lock');
select yogurt_unlock('x');
select pg_temp.eq(((yogurt_status()->>'locked_until') is not null)::text, 'true', 'fifth mixed failure locks');
select pg_temp.eq(yogurt_unlock('123456', false, 'Safari'), null, 'unlock is locked');
select pg_temp.eq(yogurt_set_passcode('654321', '123456')::text, 'false', 'set_passcode is locked');
reset role;
update yogurt_owner set locked_until = null, failed_attempts = 0;
set role authenticated;

-- Changing the passcode forgets every browser.
select yogurt_unlock('123456', true, 'Remembered') as tok \gset
select pg_temp.eq(yogurt_set_passcode('654321', '123456')::text, 'true', 'change passcode');
select pg_temp.expect('28000', array[format('select * from yogurt_library(%L)', :'tok')]);
reset role;
select pg_temp.eq((select count(*)::text from yogurt_browsers), '0', 'all browsers forgotten');
set role authenticated;
select pg_temp.eq(yogurt_unlock('123456'), null, 'old passcode no longer works');
select yogurt_unlock('654321', false, 'Safari') as tok \gset
select pg_temp.eq((select count(*)::text from yogurt_library(:'tok')), '5', 'new passcode works');

-- The PIN's length: recorded on set, backfilled by a correct unlock when it is missing, and hidden from non-owners.
select pg_temp.eq(yogurt_set_passcode('4321', '654321')::text, 'true', 'change to four digits');
select pg_temp.eq((yogurt_status()->>'passcode_length'), '4', 'length follows the new passcode');
reset role;
update yogurt_owner set passcode_length = null;
set role authenticated;
select pg_temp.eq((yogurt_status()->>'passcode_length'), null, 'no length before a backfill');
select pg_temp.eq(yogurt_unlock('0000'), null, 'wrong passcode does not backfill');
select pg_temp.eq((yogurt_status()->>'passcode_length'), null, 'still no length after a wrong passcode');
select yogurt_unlock('4321', false, 'Safari') as tok \gset
select pg_temp.eq((yogurt_status()->>'passcode_length'), '4', 'unlock backfills the length');
set request.jwt.claim.sub = 'bbbbbbbb-0000-0000-0000-000000000002';
select pg_temp.eq((yogurt_status()->>'passcode_length'), null, 'non-owner sees no length');
select pg_temp.eq((yogurt_status()->>'passcode_set'), 'false', 'non-owner sees no passcode');

-- Search: one index row per live meeting, kept by a trigger as the laptop inserts versions.
set role authenticated;
set request.jwt.claim.sub = 'aaaaaaaa-0000-0000-0000-000000000001';
\set d3 '''00000000-0000-0000-0000-0000000000d3'''
\set d4 '''00000000-0000-0000-0000-0000000000d4'''
insert into yogurt_devices (id, name) values (:d3, 'Search 3'), (:d4, 'Search 4');
select yogurt_start_push(:d3) as push_a \gset
select yogurt_start_push(:d3) as push_b \gset
select yogurt_start_push(:d4) as push_c \gset
select yogurt_unlock('4321', false, 'Search') as stok \gset

insert into yogurt_versions (meeting_id, push_id, device_id, body)
select m, :push_a, :d3, jsonb_build_object('v', 1, 'meeting', b)
from (values
  ('s1', jsonb_build_object('title', 'Budget review', 'started_at', 1000, 'notes_md', E'# Plan\n\n- Ship the **pricing** page', 'enriched_md', '<span data-ai-grey="">Greywords here</span> and <span data-transcript-link="">↳ 00:12 linklabel</span>',
     'transcript_json', '[{"ts_ms":1,"channel":"me","text":"Who brings the kombucha?"}]')),
  ('s2', jsonb_build_object('title', 'Beta', 'started_at', 2000, 'notes_md', 'Pricing again', 'transcript_json', 'not json')),
  ('s3', jsonb_build_object('title', 'Gamma', 'started_at', 3000, 'notes_md', 'zebra crossing', 'transcript_json', '[]')),
  ('s4', jsonb_build_object('title', 'Cafe', 'started_at', 4000, 'notes_md', 'Résumé at the café, latency<target', 'transcript_json', '[]')),
  ('f1', jsonb_build_object('title', 'Filler one', 'started_at', 100, 'notes_md', '', 'transcript_json', '[]')),
  ('f2', jsonb_build_object('title', 'Filler two', 'started_at', 200, 'notes_md', '', 'transcript_json', '[]')),
  ('f3', jsonb_build_object('title', 'Filler three', 'started_at', 200, 'notes_md', '', 'transcript_json', '[]'))
) t(m, b);
insert into yogurt_versions (meeting_id, push_id, device_id, body) values
  ('s1', :push_b, :d3, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Budget review', 'started_at', 1000, 'notes_md', 'Ship the roadmap',
     'enriched_md', '<span data-ai-grey="">Greywords here</span> and <span data-transcript-link="">↳ 00:12 linklabel</span>',
     'transcript_json', '[{"ts_ms":1,"channel":"me","text":"Who brings the kombucha?"}]'))),
  ('s3', :push_b, :d3, null),
  ('s7', :push_b, :d3, null);
insert into yogurt_versions (meeting_id, push_id, device_id, body) values
  ('s7', :push_a, :d3, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Late', 'started_at', 7000, 'notes_md', 'lateword', 'transcript_json', '[]'))),
  ('s5', :push_c, :d4, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Vendor', 'started_at', 5000, 'notes_md', 'vendor contract renewal, pears and apples', 'transcript_json', '[]'))),
  ('s6', :push_c, :d4, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Q4', 'started_at', 6000, 'notes_md', 'bananas for q4-plan, follow-up and api_k', 'transcript_json', '[]')));

create or replace function pg_temp.ids(tok text, q text, dev uuid default null, lim int default 100) returns text language sql as $$
  select coalesce(string_agg(meeting_id, ',' order by ord), '') from (select s.*, row_number() over () ord from yogurt_search(tok, q, dev, lim) s) x
$$;
grant execute on function pg_temp.ids(text, text, uuid, int) to public;

select pg_temp.eq(pg_temp.ids(:'stok', 'PRICING'), 's2', 'an edited-away word matches only what is live, case-insensitively');
select pg_temp.eq(pg_temp.ids(:'stok', 'roadmap'), 's1', 'the edited text matches');
select pg_temp.eq(pg_temp.ids(:'stok', 'zebra'), '', 'a deleted meeting is excluded');
select pg_temp.eq(pg_temp.ids(:'stok', 'lateword'), '', 'a version older than a tombstone is ignored');
select pg_temp.eq(pg_temp.ids(:'stok', 'kombucha'), 's1', 'transcript text matches');
select pg_temp.eq(pg_temp.ids(:'stok', 'greywords'), 's1', 'enhanced text matches inside spans');
select pg_temp.eq(pg_temp.ids(:'stok', 'data-ai-grey'), '', 'markup does not match');
select pg_temp.eq(pg_temp.ids(:'stok', 'span'), '', 'tag names do not match');
select pg_temp.eq(pg_temp.ids(:'stok', 'linklabel'), '', 'a transcript link label does not match');
select pg_temp.eq(pg_temp.ids(:'stok', 'plan'), 's6', 'heading markers are not words');
select pg_temp.eq(pg_temp.ids(:'stok', 'cafe'), 's4', 'accents fold in the text');
select pg_temp.eq(pg_temp.ids(:'stok', 'résumé'), 's4', 'accents fold in the query');
select pg_temp.eq((select snippet like '%' || chr(57344) || 'latency' || chr(57345) || '<target%' from yogurt_search(:'stok', 'latency'))::text, 'true', 'text with a bare angle bracket survives');
select pg_temp.eq(pg_temp.ids(:'stok', 'pric'), 's2', 'the last word matches as a prefix');
select pg_temp.eq(pg_temp.ids(:'stok', 'ship the road'), 's1', 'earlier words match whole');
select pg_temp.eq(pg_temp.ids(:'stok', 'ship the pric'), '', 'a prefix needs every word');
select pg_temp.eq(pg_temp.ids(:'stok', 'apples or bananas'), 's6,s5', 'or, newest first');
select pg_temp.eq(pg_temp.ids(:'stok', '"vendor contract" or pears'), 's5', 'a phrase, then or');
select pg_temp.eq(pg_temp.ids(:'stok', E'“vendor contract”'), 's5', 'curly quotes make a phrase');
select pg_temp.eq(pg_temp.ids(:'stok', '"vendor contract'), 's5', 'an unbalanced quote does not raise');
select pg_temp.eq(pg_temp.ids(:'stok', '"contract vendor"'), '', 'a phrase keeps its order');
select pg_temp.eq(pg_temp.ids(:'stok', 'q4-plan'), 's6', 'a hyphenated word');
select pg_temp.eq(pg_temp.ids(:'stok', 'api_k'), 's6', 'an underscored word');
select pg_temp.eq(pg_temp.ids(:'stok', 'follow-u'), 's6', 'a half-typed hyphenated word');
select pg_temp.eq(pg_temp.ids(:'stok', 'apples -pears'), '', 'a minus excludes');
select pg_temp.eq(pg_temp.ids(:'stok', '-zzz'), '', 'only an exclusion returns nothing');
select pg_temp.eq(pg_temp.ids(:'stok', '   '), '', 'a blank query');
select pg_temp.eq(pg_temp.ids(:'stok', null), '', 'a null query');
select pg_temp.eq(pg_temp.ids(:'stok', 'review', :d3), 's1', 'a device filter keeps its meetings');
select pg_temp.eq(pg_temp.ids(:'stok', 'review', :d4), '', 'a device filter drops the others');
select pg_temp.eq((select snippet from yogurt_search(:'stok', 'budget')), null, 'a title match has no snippet');
select pg_temp.eq((select snippet like '%' || chr(57344) || 'kombucha' || chr(57345) || '%' and snippet not like '%<%' from yogurt_search(:'stok', 'kombucha'))::text, 'true', 'the snippet marks the match');
select pg_temp.eq((select title || ':' || started_at || ':' || push_id from yogurt_search(:'stok', 'budget')), 'Budget review:1000:' || :push_b, 'summary columns');

select pg_temp.eq(pg_temp.ids(:'stok', 'filler', null, 2), 'f2,f3', 'the cap keeps the newest, ties by meeting id');
select pg_temp.eq((select bool_and(more)::text from yogurt_search(:'stok', 'filler', null, 2)), 'true', 'more when the cap is hit');
select pg_temp.eq((select bool_or(more)::text from yogurt_search(:'stok', 'filler', null, 3)), 'false', 'no more at exactly the cap');
select pg_temp.eq(pg_temp.ids(:'stok', 'filler', null, 0), 'f2', 'a cap below one is one');

-- A laptop's insert never fails because of search, and a huge multibyte body is indexed within bounds.
insert into yogurt_versions (meeting_id, push_id, device_id, body)
values ('big', :push_a, :d3, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Big', 'started_at', 1, 'notes_md', repeat('日本語 é€ ', 400000) || ' needleword', 'transcript_json', '[]')));
reset role;
select pg_temp.eq((select (octet_length(doc) <= 400000)::text from yogurt_private.yogurt_search_index where meeting_id = 'big'), 'true', 'the indexed text is byte-capped');
select pg_temp.eq((select (tsv is not null)::text from yogurt_private.yogurt_search_index where meeting_id = 'big'), 'true', 'a huge body still gets a tsvector');
delete from yogurt_versions where meeting_id = 'big';
delete from yogurt_private.yogurt_search_index where meeting_id = 'big';

-- A non-numeric time still indexes the meeting, so the count check never sees a permanent gap.
insert into yogurt_versions (meeting_id, push_id, device_id, user_id, body)
values ('oddtime', :push_a, :d3, :o, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Odd time', 'started_at', 'soon', 'ended_at', '1.5e3', 'notes_md', 'oddword', 'transcript_json', '[]')));
select pg_temp.eq((select started_at::text || '/' || coalesce(ended_at::text, 'null') from yogurt_private.yogurt_search_index where meeting_id = 'oddtime'), '0/null', 'a non-numeric time indexes with defaults');
delete from yogurt_versions where meeting_id = 'oddtime';
delete from yogurt_private.yogurt_search_index where meeting_id = 'oddtime';

-- The rebuild restores the index from versions, with the current extractor.
select count(*) as n_before from yogurt_private.yogurt_search_index \gset
update yogurt_private.yogurt_search_index set doc = 'stale', tsv = null;
delete from yogurt_private.yogurt_search_index where meeting_id = 's2';
select yogurt_private.yogurt_search_rebuild();
select pg_temp.eq((select count(*)::text from yogurt_private.yogurt_search_index), :'n_before', 'rebuild restores every live meeting');
select pg_temp.eq((select count(*)::text from yogurt_private.yogurt_search_index where meeting_id = 's3'), '0', 'rebuild skips deleted meetings');
select pg_temp.eq((select doc from yogurt_private.yogurt_search_index where meeting_id = 's1'), 'Ship the roadmap Greywords here and Who brings the kombucha?', 'rebuild rewrites the text');

-- A deploy that changed nothing leaves the index alone. A changed signature, a lost row, or a recreated table rebuilds it.
select xmin::text as x0 from yogurt_private.yogurt_search_index where meeting_id = 's1' \gset
\i /schema.sql
select pg_temp.eq((select xmin::text from yogurt_private.yogurt_search_index where meeting_id = 's1'), :'x0', 'a second apply does not rebuild');
update yogurt_private.yogurt_meta set value = 'an older extractor' where key = 'search_signature';
\i /schema.sql
select pg_temp.eq((select (xmin::text <> :'x0')::text from yogurt_private.yogurt_search_index where meeting_id = 's1'), 'true', 'a changed signature rebuilds');
select pg_temp.eq((select (value = yogurt_private.yogurt_search_signature())::text from yogurt_private.yogurt_meta where key = 'search_signature'), 'true', 'the new signature is stored');
select xmin::text as x1 from yogurt_private.yogurt_search_index where meeting_id = 's1' \gset
\i /schema.sql
select pg_temp.eq((select xmin::text from yogurt_private.yogurt_search_index where meeting_id = 's1'), :'x1', 'an unchanged apply after a rebuild leaves it alone');
delete from yogurt_private.yogurt_search_index where meeting_id = 's1';
\i /schema.sql
select pg_temp.eq((select count(*)::text from yogurt_private.yogurt_search_index where meeting_id = 's1'), '1', 'a lost row is rebuilt');
drop table yogurt_private.yogurt_search_index;
\i /schema.sql
select pg_temp.eq((select count(*)::text from yogurt_private.yogurt_search_index where meeting_id = 's1'), '1', 'a recreated table is rebuilt');
delete from yogurt_private.yogurt_meta;
select xmin::text as x2 from yogurt_private.yogurt_search_index where meeting_id = 's1' \gset
\i /schema.sql
select pg_temp.eq((select (xmin::text <> :'x2')::text from yogurt_private.yogurt_search_index where meeting_id = 's1'), 'true', 'a missing signature rebuilds');

-- The index is private, and each owner sees only their own rows.
set role authenticated;
select pg_temp.expect('42501', array['select * from yogurt_private.yogurt_search_index', 'insert into yogurt_private.yogurt_search_index (device_id, meeting_id, user_id, push_id) values (gen_random_uuid(), ''x'', gen_random_uuid(), 1)']);
reset role;
update yogurt_private.yogurt_search_index set user_id = :n;
set role authenticated;
select pg_temp.eq(pg_temp.ids(:'stok', 'budget'), '', 'only the caller''s rows');
reset role;
select yogurt_private.yogurt_search_rebuild();
set role authenticated;
select pg_temp.eq(pg_temp.ids(:'stok', 'budget'), 's1', 'rebuild restores the owner');
reset role;
delete from yogurt_versions where device_id in (:d3, :d4);
delete from yogurt_pushes where device_id in (:d3, :d4);
delete from yogurt_devices where id in (:d3, :d4);
delete from yogurt_private.yogurt_search_index;

\o
\echo schema_test: ok
