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
  'select * from yogurt_meeting(''x'', ''00000000-0000-0000-0000-0000000000d1'', ''m1'')',
  'select * from yogurt_search(''x'', ''x'')',
  'select yogurt_search_catch_up()',
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
  'yogurt_browsers,yogurt_device_list,yogurt_device_meetings,yogurt_forget_browser,yogurt_history,yogurt_library,yogurt_lock,yogurt_meeting,yogurt_retrieve,yogurt_search,yogurt_search_catch_up,yogurt_set_passcode,yogurt_start_push,yogurt_status,yogurt_unlock',
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
  'select yogurt_search_catch_up()',
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
  'select * from yogurt_meeting(''no such token'', ''00000000-0000-0000-0000-0000000000d1'', ''m1'')',
  'select * from yogurt_search(''no such token'', ''x'', null, 25)',
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

select pg_temp.eq((select string_agg(l.device_id::text || ':' || l.meeting_id, ',' order by ord)
  from (select *, row_number() over () ord from yogurt_library(:'tok')) l),
  '00000000-0000-0000-0000-0000000000d1:m1,00000000-0000-0000-0000-0000000000d2:m1,00000000-0000-0000-0000-0000000000d1:m2,00000000-0000-0000-0000-0000000000d1:m4,00000000-0000-0000-0000-0000000000d2:m5',
  'latest library');
select pg_temp.eq((select string_agg(l.meeting_id || (case when l.device_id = :d1 then 'a' else 'b' end) || l.push_id || ':' || l.total, ',') from yogurt_library(:'tok', null, 2, 1) l),
  'm1b2:5,m2a1:5', 'a page from the middle carries the total');
select pg_temp.eq((select count(*)::text from yogurt_library(:'tok', null, 2, 5)), '0', 'a page past the end is empty');
select pg_temp.eq((select count(*)::text from yogurt_library(:'tok', null, 0, 0)) || (select count(*)::text from yogurt_library(:'tok', null, 5, -3)), '15', 'a zero limit counts as 1 and a negative offset as 0');
select pg_temp.eq((select string_agg(l.meeting_id || (case when l.device_id = :d1 then 'a' else 'b' end) || l.push_id || ':' || l.total, ',') from yogurt_library(:'tok', 1, 2, 0) l),
  'm1a1:3,m2a1:3', 'a page as of push 1');
select pg_temp.eq((select string_agg(l.meeting_id || ':' || l.total, ',') from yogurt_library(:'tok', 1, 2, 2) l), 'm3:3', 'the last page as of push 1');
select pg_temp.eq((select string_agg(k, ',' order by k collate "C") from (select jsonb_object_keys(to_jsonb(l)) k from yogurt_library(:'tok', null, 1, 0) l) x),
  'device_id,ended_at,labels,meeting_id,push_id,pushed_at,starred,started_at,title,total', 'a library row has no body');
select pg_temp.eq((select string_agg(m.push_id || ':' || (m.body->>'t'), ',') from yogurt_meeting(:'tok', :d1, 'm1') m), '3:v1b', 'the live version of a meeting');
select pg_temp.eq((select string_agg(m.push_id || ':' || (m.body->>'t'), ',') from yogurt_meeting(:'tok', :d1, 'm1', 2) m), '1:v1', 'a meeting as of push 2');
select pg_temp.eq((select string_agg(m.push_id || ':' || (m.body->>'t'), ',') from yogurt_meeting(:'tok', :d2, 'm1', 2) m), '2:v2', 'the same meeting id on another device');
select pg_temp.eq((select count(*)::text from yogurt_meeting(:'tok', :d1, 'm3')), '0', 'a deleted meeting has no body');
select pg_temp.eq((select count(*)::text from yogurt_meeting(:'tok', :d1, 'm3', 2)), '1', 'a meeting before its deletion');
select pg_temp.eq((select count(*)::text from yogurt_meeting(:'tok', :d1, 'm5') ) || (select count(*)::text from yogurt_meeting(:'tok', :d1, 'm1', 0)), '00', 'an unknown meeting, and one not yet pushed');
select pg_temp.eq((select pushed_at is not null from yogurt_meeting(:'tok', :d1, 'm1'))::text, 'true', 'meeting pushed_at present');
select pg_temp.eq((select string_agg(l.meeting_id || (case when l.device_id = :d1 then 'a' else 'b' end) || l.push_id, ',') from yogurt_library(:'tok', 1) l),
  'm1a1,m2a1,m3a1', 'library as of push 1');
select pg_temp.eq((select string_agg(l.meeting_id || (case when l.device_id = :d1 then 'a' else 'b' end) || l.push_id, ',') from yogurt_library(:'tok', 2) l),
  'm1a1,m1b2,m2a1,m3a1,m5b2', 'library as of push 2');
select pg_temp.eq((select count(*)::text from yogurt_library(:'tok', 0)), '0', 'library before any push');
select pg_temp.eq((select string_agg(l.meeting_id || ':' || l.total, ',') from yogurt_library(:'tok', null, 25, 0, :d1) l), 'm1:3,m2:3,m4:3', 'a library page for one device counts only its meetings');
select pg_temp.eq((select string_agg(l.meeting_id || ':' || l.total, ',') from yogurt_library(:'tok', null, 1, 1, :d2) l), 'm5:2', 'a later page for one device');
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

-- Search: one index row per live meeting, brought up to date by a catch-up after each push.
set role authenticated;
set request.jwt.claim.sub = 'aaaaaaaa-0000-0000-0000-000000000001';
\set d3 '''00000000-0000-0000-0000-0000000000d3'''
\set d4 '''00000000-0000-0000-0000-0000000000d4'''
insert into yogurt_devices (id, name) values (:d3, 'Search 3'), (:d4, 'Search 4');
select yogurt_start_push(:d3) as push_a \gset
select yogurt_start_push(:d3) as push_b \gset
select yogurt_start_push(:d4) as push_c \gset
select yogurt_unlock('4321', false, 'Search') as stok \gset

create or replace function pg_temp.ids(tok text, q text, dev uuid default null) returns text language sql as $$
  select coalesce(string_agg(meeting_id, ',' order by ord), '') from (select s.*, row_number() over () ord from yogurt_search(tok, q, dev) s) x
$$;
grant execute on function pg_temp.ids(text, text, uuid) to public;

\set enriched '''- <span data-ai-grey="">Greywords here</span> <span data-transcript-link="">↳ 00:12 linkone</span>\n- middlebullet text <span data-transcript-link="">↳ 00:34 linktwo</span>\n- tailword <span data-transcript-link="">↳ 01:02 linkthree</span>'''
insert into yogurt_versions (meeting_id, push_id, device_id, body)
select m, :push_a, :d3, jsonb_build_object('v', 1, 'meeting', b)
from (values
  ('s1', jsonb_build_object('title', 'Budget review', 'started_at', 1000, 'notes_md', E'# Plan\n\n- Ship the **pricing** page', 'enriched_md', replace(:enriched, '\n', E'\n'),
     'transcript_json', '[{"ts_ms":1,"channel":"me","text":"Who brings the kombucha?"}]')),
  ('s2', jsonb_build_object('title', 'Beta', 'started_at', 2000, 'notes_md', 'Pricing again', 'transcript_json', 'not json')),
  ('s3', jsonb_build_object('title', 'Gamma', 'started_at', 3000, 'notes_md', 'zebra crossing', 'transcript_json', '[]')),
  ('s4', jsonb_build_object('title', 'Résumé', 'started_at', 4000, 'notes_md', 'Résumé at the café, latency<target &amp; &lt;b&gt;bold&lt;/b&gt; &nbsp;done, see &lt;runbook&gt;', 'transcript_json', '[]')),
  ('s8', jsonb_build_object('title', 'Codes', 'started_at', 8000, 'notes_md', 'Order ABC-123 shipped 2024-01-15 during covid-19, call 555-0100, see https://example.com/billing/page', 'transcript_json', '[]')),
  ('s9', jsonb_build_object('title', 'Kick', 'started_at', 9000, 'notes_md', 'kickoff notes with 2*3 and snake_case_name, _italic_ and **bold**', 'transcript_json', '[]')),
  ('s10', jsonb_build_object('title', 'Links', 'started_at', 10000, 'notes_md',
     E'write to jane-doe@example.com about my-company.com\n- Ship the quokka page\nsee https://example.com/my-first-post, github.com/rchen3/rchen-vault, q4-report.pdf and https://meet.google.com/abc-defg-hij\nmail jane@acme.com',
     'transcript_json', '[]')),
  ('s11', jsonb_build_object('title', 'Deck', 'started_at', 11000, 'notes_md', 'the deck lives at docs.acme.com/q4', 'transcript_json', '[]'))
) t(m, b);
insert into yogurt_versions (meeting_id, push_id, device_id, body) values
  ('s1', :push_b, :d3, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Budget review', 'started_at', 1000, 'notes_md', 'Ship the roadmap',
     'enriched_md', replace(:enriched, '\n', E'\n'),
     'transcript_json', '[{"ts_ms":1,"channel":"me","text":"Who brings the kombucha?"}]'))),
  ('s3', :push_b, :d3, null),
  ('s7', :push_b, :d3, null),
  ('s12', :push_b, :d3, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Newer', 'started_at', 12000, 'notes_md', 'twelveword', 'transcript_json', '[]')));
insert into yogurt_versions (meeting_id, push_id, device_id, body) values
  ('s7', :push_a, :d3, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Late', 'started_at', 7000, 'notes_md', 'lateword', 'transcript_json', '[]'))),
  ('s12', :push_a, :d3, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Older', 'started_at', 12000, 'notes_md', 'twelveword', 'transcript_json', '[]'))),
  ('s5', :push_c, :d4, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Vendor', 'started_at', 5000, 'notes_md', 'vendor contract renewal, pears and apples, follow along and update', 'transcript_json', '[]'))),
  ('s6', :push_c, :d4, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Q4', 'started_at', 6000, 'notes_md', 'bananas for q4-plan, follow-up and api_k', 'transcript_json', '[]')));
insert into yogurt_versions (meeting_id, push_id, device_id, body)
select 'g' || lpad(n::text, 2, '0'), :push_c, :d4, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'bulk ' || n, 'started_at', 500, 'notes_md', '', 'transcript_json', '[]'))
from generate_series(1, 30) n;
insert into yogurt_versions (meeting_id, push_id, device_id, body)
select 'h' || lpad(n::text, 2, '0'), :push_c, :d4, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'exact ' || n, 'started_at', 400, 'notes_md', '', 'transcript_json', '[]'))
from generate_series(1, 25) n;

select pg_temp.eq(pg_temp.ids(:'stok', 'roadmap'), '', 'a push alone indexes nothing');
select pg_temp.eq(yogurt_search_catch_up()::text, '0', 'the catch-up after a push indexes everything');

select pg_temp.eq(pg_temp.ids(:'stok', 'PRICING'), 's2', 'an edited-away word matches only what is live, case-insensitively');
select pg_temp.eq(pg_temp.ids(:'stok', 'roadmap'), 's1', 'the edited text matches');
select pg_temp.eq(pg_temp.ids(:'stok', 'zebra'), '', 'a deleted meeting is excluded');
select pg_temp.eq(pg_temp.ids(:'stok', 'lateword'), '', 'a version older than a tombstone is ignored');
select pg_temp.eq((select title from yogurt_search(:'stok', 'twelveword')), 'Newer', 'an older push that arrives late never replaces a newer one');
select pg_temp.eq(pg_temp.ids(:'stok', 'kombucha'), 's1', 'transcript text matches');
select pg_temp.eq(pg_temp.ids(:'stok', 'greywords'), 's1', 'enhanced text matches inside spans');
select pg_temp.eq(pg_temp.ids(:'stok', 'middlebullet'), 's1', 'text between several link spans is kept');
select pg_temp.eq(pg_temp.ids(:'stok', 'tailword'), 's1', 'text after the last link span is kept');
select pg_temp.eq(pg_temp.ids(:'stok', 'data-ai-grey'), '', 'markup does not match');
select pg_temp.eq(pg_temp.ids(:'stok', 'span'), '', 'tag names do not match');
select pg_temp.eq(pg_temp.ids(:'stok', 'linkone'), '', 'a transcript link label does not match');
select pg_temp.eq(pg_temp.ids(:'stok', 'linktwo'), '', 'a second transcript link label does not match');
select pg_temp.eq(pg_temp.ids(:'stok', 'linkthree'), '', 'a third transcript link label does not match');
select pg_temp.eq(pg_temp.ids(:'stok', 'plan'), 's6', 'heading markers are not words');
select pg_temp.eq(pg_temp.ids(:'stok', 'cafe'), 's4', 'accents fold in the text');
select pg_temp.eq(pg_temp.ids(:'stok', 'résumé'), 's4', 'accents fold in the query');
select pg_temp.eq(pg_temp.ids(:'stok', 'RESUME'), 's4', 'accents and case fold together');
select pg_temp.eq((select snippet like '%' || chr(57344) || 'café' || chr(57345) || '%' from yogurt_search(:'stok', 'cafe'))::text, 'true', 'the snippet shows the original accented word');
select pg_temp.eq((select title from yogurt_search(:'stok', 'resume')), 'Résumé', 'the title keeps its accents');
select pg_temp.eq((select snippet like '%' || chr(57344) || 'latency' || chr(57345) || '<target%' from yogurt_search(:'stok', 'latency'))::text, 'true', 'text with a bare angle bracket survives');
reset role;
select pg_temp.eq((select doc from yogurt_private.yogurt_search_index where meeting_id = 's4'), 'Résumé at the café, latency<target & <b>bold</b> done, see <runbook>', 'entities are decoded for display');
set role authenticated;
select pg_temp.eq(pg_temp.ids(:'stok', '2*3'), 's9', 'a star inside a word is kept');
select pg_temp.eq(pg_temp.ids(:'stok', 'snake_case_name'), 's9', 'underscores inside a word are kept');
reset role;
select pg_temp.eq((select doc from yogurt_private.yogurt_search_index where meeting_id = 's9'), 'kickoff notes with 2*3 and snake_case_name, italic and bold', 'emphasis markers go and word characters stay');
set role authenticated;
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
select pg_temp.eq(pg_temp.ids(:'stok', 'follow-up'), 's6', 'a hyphenated last word is a phrase, not its parts anywhere');
select pg_temp.eq(pg_temp.ids(:'stok', 'runbook'), 's4', 'a tag-shaped word is indexed');
select pg_temp.eq(pg_temp.ids(:'stok', 'ABC-123'), 's8', 'a code with a digit after the hyphen');
select pg_temp.eq(pg_temp.ids(:'stok', '2024-01-15'), 's8', 'a date');
select pg_temp.eq(pg_temp.ids(:'stok', 'covid-19'), 's8', 'a word then a number');
select pg_temp.eq(pg_temp.ids(:'stok', '555-0100'), 's8', 'a phone number');
select pg_temp.eq(pg_temp.ids(:'stok', 'vendor - contract'), 's5', 'a spaced hyphen separates words');
select pg_temp.eq(pg_temp.ids(:'stok', E'vendor – contract'), 's5', 'an en dash separates words');
select pg_temp.eq(pg_temp.ids(:'stok', E'vendor — contract'), 's5', 'an em dash separates words');
select pg_temp.eq(pg_temp.ids(:'stok', '- Ship the quokka page'), 's10', 'a pasted bullet');
select pg_temp.eq(pg_temp.ids(:'stok', '- - quokka'), 's10', 'two pasted dashes');
select pg_temp.eq(pg_temp.ids(:'stok', 'https://example.com/billing/page'), 's8', 'a pasted URL matches the same URL in a note');
select pg_temp.eq(pg_temp.ids(:'stok', 'jane-doe@example.com'), 's10', 'a hyphenated email');
select pg_temp.eq(pg_temp.ids(:'stok', 'my-company.com'), 's10', 'a hyphenated host');
select pg_temp.eq(pg_temp.ids(:'stok', 'write to jane-doe@example.com'), 's10', 'words, then an email');
select pg_temp.eq(pg_temp.ids(:'stok', 'https://example.com/my-first-post'), 's10', 'a URL with a hyphenated path');
select pg_temp.eq(pg_temp.ids(:'stok', '(see example.com/my-first-post).'), 's10', 'a URL in parentheses');
select pg_temp.eq(pg_temp.ids(:'stok', 'github.com/rchen3/rchen-vault'), 's10', 'a URL without a scheme');
select pg_temp.eq(pg_temp.ids(:'stok', 'q4-report.pdf'), 's10', 'a hyphenated file name');
select pg_temp.eq(pg_temp.ids(:'stok', 'https://meet.google.com/abc-defg-hij'), 's10', 'a meeting link');
select pg_temp.eq(pg_temp.ids(:'stok', 'acme'), 's11,s10', 'a part of an email or a host');
select pg_temp.eq(pg_temp.ids(:'stok', 'acm'), 's11,s10', 'a half-typed part');
select pg_temp.eq(pg_temp.ids(:'stok', 'rchen-vault'), 's10', 'a part of a URL path');
select pg_temp.eq(pg_temp.ids(:'stok', 'apples -pears'), '', 'a minus excludes');
select pg_temp.eq(pg_temp.ids(:'stok', 'cafe -follow-up'), 's4', 'a minus excludes a hyphenated word');
select pg_temp.eq(pg_temp.ids(:'stok', 'cafe -"follow up"'), 's4', 'a minus excludes a phrase');
select pg_temp.eq(pg_temp.ids(:'stok', '-follow-up'), '', 'only an excluded hyphenated word returns nothing');
select pg_temp.eq((pg_temp.ids(:'stok', 'kickoff or -zebra') like '%s9%')::text, 'true', 'or with an exclusion still matches the positive word');
select pg_temp.eq(pg_temp.ids(:'stok', '-zzz'), '', 'only an exclusion returns nothing');
select pg_temp.eq(pg_temp.ids(:'stok', '-zzz -yyy'), '', 'only exclusions return nothing');
select pg_temp.eq(pg_temp.ids(:'stok', '   '), '', 'a blank query');
select pg_temp.eq(pg_temp.ids(:'stok', null), '', 'a null query');
select pg_temp.eq(pg_temp.ids(:'stok', '-'), '', 'a lone dash');
select pg_temp.eq(pg_temp.ids(:'stok', '"'), '', 'a lone quote');
select pg_temp.eq(pg_temp.ids(:'stok', 'review', :d3), 's1', 'a device filter keeps its meetings');
select pg_temp.eq(pg_temp.ids(:'stok', 'review', :d4), '', 'a device filter drops the others');
select pg_temp.eq((select snippet from yogurt_search(:'stok', 'budget')), null, 'a title match has no snippet');
select pg_temp.eq((select snippet like '%' || chr(57344) || 'kombucha' || chr(57345) || '%' and snippet not like '%<%' from yogurt_search(:'stok', 'kombucha'))::text, 'true', 'the snippet marks the match');
select pg_temp.eq((select title || ':' || started_at || ':' || push_id from yogurt_search(:'stok', 'budget')), 'Budget review:1000:' || :push_b, 'summary columns');

select pg_temp.eq((select count(*)::text from yogurt_search(:'stok', 'bulk')), '25', 'the cap is 25');
select pg_temp.eq((select string_agg(meeting_id, ',' order by meeting_id) from yogurt_search(:'stok', 'bulk') where meeting_id in ('g01', 'g25', 'g26')), 'g01,g25', 'the cap keeps the first by meeting id among equal times');
select pg_temp.eq((select bool_and(more)::text from yogurt_search(:'stok', 'bulk')), 'true', 'more when there were more');
select pg_temp.eq((select count(*)::text || ':' || bool_or(more)::text from yogurt_search(:'stok', 'exact')), '25:false', 'no more at exactly the cap');
select pg_temp.eq((select min(meeting_id) || ':' || count(*) || ':' || bool_or(more) from yogurt_search(:'stok', 'bulk', null, 4)), 'g05:25:true', 'an offset skips rows and still reports more');
select pg_temp.eq((select min(meeting_id) || ':' || count(*) || ':' || bool_or(more) from yogurt_search(:'stok', 'bulk', null, 5)), 'g06:25:false', 'the last full page has no more');
select pg_temp.eq((select min(meeting_id) || ':' || count(*) || ':' || bool_or(more) from yogurt_search(:'stok', 'bulk', null, 25)), 'g26:5:false', 'a short last page');
select pg_temp.eq((select count(*)::text from yogurt_search(:'stok', 'bulk', null, 30)), '0', 'an offset past the end is empty');
select pg_temp.eq((select count(*)::text from yogurt_search(:'stok', 'bulk', null, -7)), '25', 'a negative offset counts as 0');
select pg_temp.eq((select count(*)::text from yogurt_search(:'stok', 'bulk', :d3, 0)), '0', 'an offset call still filters by device');
-- A deletion pushed after the index last caught up hides the meeting on every page.
select yogurt_start_push(:d4) as push_d \gset
insert into yogurt_versions (meeting_id, push_id, device_id, body) values ('g30', :push_d, :d4, null), ('g29', :push_d, :d4, null);
select pg_temp.eq((select count(*)::text || ':' || bool_or(more) from yogurt_search(:'stok', 'bulk', null, 25)), '3:false', 'deleted meetings are hidden on a later page before the catch-up');
select pg_temp.eq((select count(*)::text || ':' || bool_or(more) from yogurt_search(:'stok', 'bulk', null, 0)), '25:true', 'and the first page still has more');
select pg_temp.eq(yogurt_search_catch_up()::text, '0', 'the catch-up after the deletions');
select pg_temp.eq((select count(*)::text from yogurt_search(:'stok', 'bulk', null, 25)), '3', 'still hidden once caught up');

-- The library lists summaries newest first, with the labels and star, and deleted meetings drop out of the rows and the total.
insert into yogurt_versions (meeting_id, push_id, device_id, body)
values ('star1', :push_a, :d3, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Zq starred', 'started_at', 99999, 'ended_at', 100999, 'starred', true,
  'labels', jsonb_build_array(jsonb_build_object('id', 'l1', 'name', 'Team', 'color', 'blue')), 'notes_md', repeat('x', 5000), 'transcript_json', '[]')));
reset role;
select count(*)::text as live from (select distinct on (device_id, meeting_id) body from yogurt_versions order by device_id, meeting_id, push_id desc) l where body is not null \gset
set role authenticated;
select pg_temp.eq((select total::text from yogurt_library(:'stok', null, 1, 0)), :'live', 'the total counts live meetings only');
select pg_temp.eq((select title || ':' || started_at || ':' || ended_at || ':' || starred || ':' || labels || ':' || total from yogurt_library(:'stok', null, 1, 0)),
  'Zq starred:99999:100999:true:[{"id": "l1", "name": "Team", "color": "blue"}]:' || :'live', 'the newest meeting leads with its summary fields');
select pg_temp.eq((select string_agg(meeting_id || device_id, ',' order by o) from (select l.*, row_number() over () o from yogurt_library(:'stok', null, 10, 0) l) x),
  (select string_agg(meeting_id || device_id, ',' order by o) from (select l.*, row_number() over () o from yogurt_library(:'stok', null, 100, 0) l limit 10) x), 'a page of 10 is the head of a page of 100');
select pg_temp.eq((select string_agg(meeting_id || device_id, ',' order by o) from (select l.*, row_number() over () o from yogurt_library(:'stok', null, 10, 10) l) x),
  (select string_agg(meeting_id || device_id, ',' order by o) from (select l.*, row_number() over () o from yogurt_library(:'stok', null, 100, 0) l offset 10 limit 10) x), 'the next page continues it');
select pg_temp.eq((select (array_agg(started_at) = array_agg(started_at order by started_at desc))::text from yogurt_library(:'stok', null, 100, 0)), 'true', 'newest first');
insert into yogurt_versions (meeting_id, push_id, device_id, body)
select 'fill' || n, :push_a, :d3, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Fill ' || n, 'started_at', 50000 + n, 'notes_md', '', 'transcript_json', '[]'))
from generate_series(1, 110) n;
select pg_temp.eq((select count(*)::text from yogurt_library(:'stok', null, 1000, 0)), '100', 'a page is capped at 100 rows');
select pg_temp.eq((select count(*)::text from yogurt_library(:'stok', null, 100, 0) where meeting_id in ('g29', 'g30')), '0', 'a deleted meeting is not listed');
select pg_temp.eq(((select length(string_agg(to_jsonb(l)::text, '')) from yogurt_library(:'stok', null, 100, 0) l) < 30000)::text, 'true', 'a page of summaries stays small');
reset role;
delete from yogurt_versions where meeting_id like 'fill%' or meeting_id = 'star1';
set role authenticated;

-- A huge multibyte body is indexed within bounds, and a long run without spaces indexes quickly.
insert into yogurt_versions (meeting_id, push_id, device_id, body)
values ('big', :push_a, :d3, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Big', 'started_at', 1, 'notes_md', repeat('日本語 é€ ', 400000) || ' needleword', 'transcript_json', '[]')));
select pg_temp.eq(yogurt_search_catch_up()::text, '0', 'a huge body is indexed');
reset role;
select pg_temp.eq((select (octet_length(doc) <= 400000)::text from yogurt_private.yogurt_search_index where meeting_id = 'big'), 'true', 'the indexed text is byte-capped');
select pg_temp.eq((select (tsv is not null)::text from yogurt_private.yogurt_search_index where meeting_id = 'big'), 'true', 'a huge body still gets a tsvector');
delete from yogurt_versions where meeting_id = 'big';
select pg_temp.eq((length(yogurt_private.yogurt_plain('é' || repeat('abcd ', 80000))) > 399990)::text, 'true', 'one byte over the cap drops about one character');
insert into yogurt_versions (meeting_id, push_id, device_id, user_id, body)
values ('dense', :push_a, :d3, :o, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Dense hosts', 'started_at', 1, 'notes_md', repeat('srv-1a2b3.prod.example.net/abc123 ', 12000), 'transcript_json', '[]')));
select pg_temp.eq(yogurt_private.yogurt_search_catch_up(30000)::text, '0', 'text past the tsvector limit does not block the catch-up');
set role authenticated;
select pg_temp.eq(pg_temp.ids(:'stok', 'dense'), 'dense', 'it still indexes its title');
reset role;
delete from yogurt_versions where meeting_id = 'dense';
set role authenticated;
insert into yogurt_versions (meeting_id, push_id, device_id, body)
values ('run', :push_a, :d3, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Run', 'started_at', 1, 'notes_md', 'runword ' || repeat('ab_', 34000) || ' tailrun ' || repeat('x-1-', 25000), 'transcript_json', '[]')));
select clock_timestamp() as t0 \gset
select pg_temp.eq(yogurt_search_catch_up()::text, '0', 'a body of long runs without spaces is indexed');
select pg_temp.eq((clock_timestamp() - :'t0'::timestamptz < interval '1 second')::text, 'true', 'within a second');
select pg_temp.eq(pg_temp.ids(:'stok', 'tailrun'), 'run', 'a word after a long run matches');

-- A non-numeric or overflowing time still indexes the meeting.
reset role;
insert into yogurt_versions (meeting_id, push_id, device_id, user_id, body)
values ('oddtime', :push_a, :d3, :o, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Odd time', 'started_at', 'soon', 'ended_at', '1.5e3', 'notes_md', 'oddword', 'transcript_json', '[]'))),
       ('hugetime', :push_a, :d3, :o, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Huge time', 'started_at', 1e30::numeric, 'ended_at', 12.6, 'notes_md', 'hugeword', 'transcript_json', '[]')));
select pg_temp.eq(yogurt_private.yogurt_search_catch_up(2000)::text, '0', 'odd times are indexed');
select pg_temp.eq((select started_at::text || '/' || coalesce(ended_at::text, 'null') from yogurt_private.yogurt_search_index where meeting_id = 'oddtime'), '0/null', 'a non-numeric time indexes with defaults');
select pg_temp.eq((select started_at::text || '/' || coalesce(ended_at::text, 'null') from yogurt_private.yogurt_search_index where meeting_id = 'hugetime'), '0/13', 'an overflowing time indexes with defaults and a fraction rounds');
delete from yogurt_versions where meeting_id in ('oddtime', 'hugetime', 'run');
select pg_temp.eq((select count(*)::text from yogurt_private.yogurt_search_index where meeting_id in ('oddtime', 'hugetime', 'run')), '0', 'deleting a version removes its text');

-- A catch-up leaves a current row alone, and repairs a missing, outdated, or extra row and one built with another signature.
select xmin::text as x0 from yogurt_private.yogurt_search_index where meeting_id = 's1' \gset
select pg_temp.eq(yogurt_private.yogurt_search_catch_up(2000)::text, '0', 'nothing to repair');
select pg_temp.eq((select xmin::text from yogurt_private.yogurt_search_index where meeting_id = 's1'), :'x0', 'a current row is left alone');
delete from yogurt_private.yogurt_search_index where meeting_id = 's1';
select pg_temp.eq(yogurt_private.yogurt_search_catch_up(2000)::text, '0', 'a missing row is repaired');
select pg_temp.eq((select count(*)::text from yogurt_private.yogurt_search_index where meeting_id = 's1'), '1', 'a missing row is written');
update yogurt_private.yogurt_search_index set push_id = :push_a, title = 'outdated' where meeting_id = 's1';
select yogurt_private.yogurt_search_catch_up(2000);
select pg_temp.eq((select push_id::text || ':' || title from yogurt_private.yogurt_search_index where meeting_id = 's1'), :'push_b' || ':Budget review', 'an outdated row is replaced');
insert into yogurt_private.yogurt_search_index (device_id, meeting_id, user_id, push_id, title, tsv, sig) values (:d3, 's3', :o, :push_a, 'extra', to_tsvector('ghostword'), yogurt_private.yogurt_search_signature());
set role authenticated;
select pg_temp.eq(pg_temp.ids(:'stok', 'ghostword'), '', 'a meeting deleted since the last catch-up is hidden');
reset role;
select yogurt_private.yogurt_search_catch_up(2000);
select pg_temp.eq((select count(*)::text from yogurt_private.yogurt_search_index where meeting_id = 's3'), '0', 'an extra row for a deleted meeting is removed');
update yogurt_private.yogurt_search_index set sig = 'older', user_id = :n;
select pg_temp.eq(yogurt_private.yogurt_search_catch_up(2000)::text, '0', 'a changed signature is caught up');
select pg_temp.eq((select count(*)::text from yogurt_private.yogurt_search_index where sig is distinct from yogurt_private.yogurt_search_signature()), '0', 'every row is rebuilt');
set role authenticated;
select pg_temp.eq(pg_temp.ids(:'stok', 'budget'), 's1', 'the rebuild restores the owner');
reset role;

-- The signature follows the configuration, and a deploy puts the mapping back.
select yogurt_private.yogurt_search_signature() as sig0 \gset
alter text search configuration yogurt_private.yogurt_ts alter mapping for asciiword, word with pg_catalog.simple;
select pg_temp.eq((yogurt_private.yogurt_search_signature() <> :'sig0')::text, 'true', 'a changed mapping changes the signature');
select pg_temp.eq((yogurt_private.yogurt_search_catch_up(0) > 0)::text, 'true', 'and makes rows stale');
\i /schema.sql
select pg_temp.eq(yogurt_private.yogurt_search_signature(), :'sig0', 'a deploy restores the mapping');
select pg_temp.eq(yogurt_private.yogurt_search_catch_up(0)::text, '0', 'so rows built before the change are current again');
set role authenticated;
select pg_temp.eq(pg_temp.ids(:'stok', 'cafe'), 's4', 'accents fold again');
reset role;

-- A catch-up stops after about max_ms and says how many rows are left, and later calls finish them.
insert into yogurt_versions (meeting_id, push_id, device_id, user_id, body)
select 'long' || n, :push_c, :d4, :o, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Long ' || n, 'started_at', 1, 'notes_md', repeat('lorem ipsum dolor sit amet consectetur ', 1550), 'transcript_json', '[]'))
from generate_series(1, 40) n;
select clock_timestamp() as t0 \gset
select yogurt_private.yogurt_search_catch_up(100) as left1 \gset
select pg_temp.eq((clock_timestamp() - :'t0'::timestamptz < interval '600 milliseconds')::text, 'true', 'a catch-up stops near its limit');
select pg_temp.eq((:left1 between 1 and 39)::text, 'true', 'and reports what is left after making progress');
select pg_temp.eq(yogurt_private.yogurt_search_catch_up(10000)::text, '0', 'a later call finishes');
delete from yogurt_versions where meeting_id like 'long%';

-- A meeting deleted while a catch-up runs stays deleted, because each write rereads the meeting's latest version.
-- The catch-up rebuilds every row in descending key order, so a lock on the first one holds it until the deletion has committed.
create extension if not exists dblink schema extensions;
update yogurt_private.yogurt_search_index set sig = 'older';
select extensions.dblink_connect('holder', 'dbname=' || current_database());
select extensions.dblink_connect('runner', 'dbname=' || current_database());
select extensions.dblink_exec('holder', 'begin');
select * from extensions.dblink('holder', format('select 1 from yogurt_private.yogurt_search_index where (device_id, meeting_id) = (%L, %L) for update', :d4, 's6')) as t(x int);
select extensions.dblink_send_query('runner', 'select yogurt_private.yogurt_search_catch_up(10000)');
do $$ begin
  for i in 1..200 loop
    perform pg_stat_clear_snapshot();
    exit when exists (select from pg_stat_activity where pid <> pg_backend_pid() and wait_event_type = 'Lock' and query like '%yogurt_search_catch_up(10000)%');
    perform pg_sleep(0.02);
  end loop;
  if not exists (select from pg_stat_activity where pid <> pg_backend_pid() and wait_event_type = 'Lock' and query like '%yogurt_search_catch_up(10000)%') then
    raise exception 'the catch-up never waited on the held row';
  end if;
end $$;
set role authenticated;
select yogurt_start_push(:d3) as push_d \gset
insert into yogurt_versions (meeting_id, push_id, device_id, body) values ('s4', :push_d, :d3, null);
reset role;
select extensions.dblink_exec('holder', 'commit');
select pg_temp.eq((select n::text from extensions.dblink_get_result('runner') as t(n int)), '0', 'the concurrent catch-up finishes');
select extensions.dblink_disconnect('holder'), extensions.dblink_disconnect('runner');
select pg_temp.eq((select count(*)::text from yogurt_private.yogurt_search_index where meeting_id = 's4'), '0', 'the deleted meeting did not come back');
set role authenticated;
select pg_temp.eq(pg_temp.ids(:'stok', 'cafe'), '', 'and is not found');

-- Retrieval for cross-meeting chat: ORed prefix terms over the live index, with the notes and a transcript excerpt.
select yogurt_start_push(:d3) as push_r \gset
insert into yogurt_versions (meeting_id, push_id, device_id, body) values
  ('r1', :push_r, :d3, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Fruit sync', 'started_at', 20000, 'notes_md', 'raw notes',
     'enriched_md', '- <span data-ai-grey="">notesonly plan</span>',
     'transcript_json', jsonb_build_array(jsonb_build_object('ts_ms', 1, 'channel', 'me', 'text', repeat('filler ', 80) || 'the kiwi order slipped ' || repeat('padding ', 80)))::text))),
  ('r2', :push_r, :d3, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Mango', 'started_at', 21000, 'notes_md', 'kiwi kiwi kiwi', 'enriched_md', '  ', 'transcript_json', '[]'))),
  ('r3', :push_r, :d3, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Berry', 'started_at', 22000, 'notes_md', 'kiwiberry and notesonly', 'transcript_json', 'not json'))),
  ('r4', :push_r, :d3, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Gone', 'started_at', 23000, 'notes_md', 'kiwi', 'transcript_json', '[]'))),
  ('r6', :push_r, :d3, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'साप्ताहिक मिटिंग', 'started_at', 18500, 'notes_md', 'ประชุม ทีม', 'transcript_json', '[]')));
select pg_temp.eq(yogurt_search_catch_up()::text, '0', 'retrieval rows are indexed');
select yogurt_start_push(:d3) as push_r2 \gset
insert into yogurt_versions (meeting_id, push_id, device_id, body) values ('r4', :push_r2, :d3, null);

create or replace function pg_temp.rids(terms text[], from_ms bigint default null, to_ms bigint default null, k int default 6) returns text language sql as $$
  select coalesce(string_agg(meeting_id, ',' order by ord), '') from (select r.*, row_number() over () ord from yogurt_retrieve(current_setting('t.tok'), terms, from_ms, to_ms, k) r) x
$$;
grant execute on function pg_temp.rids(text[], bigint, bigint, int) to public;
select set_config('t.tok', :'stok', false);

select pg_temp.eq(pg_temp.rids('{kiwi}'), 'r2,r3,r1', 'terms match as prefixes, ranked, and a deletion pushed since the catch-up is hidden');
select pg_temp.eq(pg_temp.rids('{mango,notesonly}', 15000, 30000), 'r3,r2,r1', 'terms are ORed');
select pg_temp.eq(pg_temp.rids('{kiwi}', 20500, 22000), 'r2', 'the range is [from, to)');
select pg_temp.eq(pg_temp.rids('{}', 19000, 30000), 'r3,r2,r1', 'no terms is the newest in the range');
select pg_temp.eq(pg_temp.rids('{"!!", ""}', 19000, 30000, 2), 'r3,r2', 'terms with no letters count as none, and k limits');
select pg_temp.eq(pg_temp.rids('{"kiwi'':*!|", "&|:*"}', 19000, 30000), 'r2,r3,r1', 'tsquery syntax in a term is stripped');
select pg_temp.eq(pg_temp.rids('{kiwi}', null, null, 0), 'r2', 'k is at least 1');
select pg_temp.eq((select notes from yogurt_retrieve(:'stok', '{kiwi}') where meeting_id = 'r1'), '- <span data-ai-grey="">notesonly plan</span>', 'notes are the enhanced notes');
select pg_temp.eq((select notes from yogurt_retrieve(:'stok', '{kiwi}') where meeting_id = 'r2'), 'kiwi kiwi kiwi', 'else the user''s notes');
select pg_temp.eq((select excerpt ~ '^filler .* the kiwi order slipped .* padding$' and excerpt !~ 'notesonly' and array_length(regexp_split_to_array(excerpt, ' '), 1) <= 64 from yogurt_retrieve(:'stok', '{kiwi}') where meeting_id = 'r1')::text, 'true', 'the excerpt is transcript around the term');
select pg_temp.eq((select string_agg(excerpt, '|') from yogurt_retrieve(:'stok', '{kiwi}') where meeting_id in ('r2', 'r3')), '|', 'no transcript, or a malformed one, is an empty excerpt');
select pg_temp.eq((select string_agg(excerpt, '|') from yogurt_retrieve(:'stok', '{}', 19000, 21000)), '', 'no terms, no excerpt');
select pg_temp.expect('28000', array['select * from yogurt_retrieve(''wrong'', ''{kiwi}'')']);
select pg_temp.eq(pg_temp.rids('{मिटिंग}') || ' ' || pg_temp.rids('{ประชุม}'), 'r6 r6', 'a term keeps its combining marks');
select pg_temp.eq(pg_temp.rids('{zzzz}', 19000, 30000), 'r3,r2,r1', 'terms that match nothing fall back to the newest in the range');
-- A push the index has not caught up to: an edit answers with its new text, and a new meeting counts among the newest.
select yogurt_start_push(:d3) as push_r3 \gset
insert into yogurt_versions (meeting_id, push_id, device_id, body) values
  ('r2', :push_r3, :d3, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Mango v2', 'started_at', 21000, 'notes_md', 'kiwi changed', 'transcript_json', '[]'))),
  ('r5', :push_r3, :d3, jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Fresh', 'started_at', 25000, 'notes_md', 'fresh', 'transcript_json', '[]')));
select pg_temp.eq((select title || ': ' || notes from yogurt_retrieve(:'stok', '{kiwi}') where meeting_id = 'r2'), 'Mango v2: kiwi changed', 'an edit not yet indexed answers with its new text');
select pg_temp.eq(pg_temp.rids('{}', 19000, 30000), 'r5,r3,r2,r1', 'a meeting not yet indexed counts among the newest');

-- The index is private, and each owner sees only their own rows.
select pg_temp.expect('42501', array['select * from yogurt_private.yogurt_search_index', 'insert into yogurt_private.yogurt_search_index (device_id, meeting_id, user_id, push_id) values (gen_random_uuid(), ''x'', gen_random_uuid(), 1)']);
reset role;
update yogurt_private.yogurt_search_index set user_id = :n;
set role authenticated;
select pg_temp.eq(pg_temp.ids(:'stok', 'budget'), '', 'only the caller''s rows');
select pg_temp.eq((select excerpt from yogurt_retrieve(:'stok', '{kiwi}', 19000, 21000)), '', 'chat retrieval reads only the caller''s index rows, so its terms fall back to the newest');
reset role;

-- The guide's purge removes a device's text from search along with its versions.
update yogurt_private.yogurt_search_index set sig = 'older';
select yogurt_private.yogurt_search_catch_up(2000);
set role authenticated;
select pg_temp.eq(pg_temp.ids(:'stok', 'vendor'), 's5', 'before the purge');
reset role;
delete from yogurt_versions where device_id = :d4;
delete from yogurt_pushes where device_id = :d4;
delete from yogurt_devices where id = :d4;
set role authenticated;
select pg_temp.eq(pg_temp.ids(:'stok', 'vendor'), '', 'a purged device is gone from search');
select pg_temp.eq(pg_temp.ids(:'stok', 'bulk'), '', 'every purged meeting is gone from search');
reset role;
delete from yogurt_versions where device_id = :d3;
delete from yogurt_pushes where device_id = :d3;
delete from yogurt_devices where id = :d3;
select pg_temp.eq((select count(*)::text from yogurt_private.yogurt_search_index where device_id in (:d3, :d4)), '0', 'no text is left once every device is purged');

\o
\echo schema_test: ok
