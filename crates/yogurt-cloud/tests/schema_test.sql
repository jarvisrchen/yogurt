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

truncate auth.users, yogurt_devices, yogurt_keyring, yogurt_pushes, yogurt_versions restart identity cascade;
insert into auth.users values
  ('aaaaaaaa-0000-0000-0000-000000000001'),
  ('bbbbbbbb-0000-0000-0000-000000000002');

-- User A pushes through the same grants a laptop has.
set role authenticated;
set request.jwt.claim.sub = 'aaaaaaaa-0000-0000-0000-000000000001';

insert into yogurt_devices (id, name_ct) values
  ('d0000000-0000-0000-0000-00000000000a', 'ct-a'),
  ('d0000000-0000-0000-0000-00000000000b', 'ct-b');
insert into yogurt_keyring (public_key, wrapped_private_key, kdf)
  values ('pk', 'wrapped', '{"alg":"pbkdf2-sha256","iter":600000,"salt":"s"}');

-- push 1 (device a): m1, m2, m3.  push 2 (device b): m1 updated, m2 deleted.  push 3 (device a): m3 deleted, m4.
insert into yogurt_pushes (device_id) values ('d0000000-0000-0000-0000-00000000000a');
insert into yogurt_versions (meeting_id, push_id, device_id, ct) values
  ('m1', 1, 'd0000000-0000-0000-0000-00000000000a', 'm1-v1'),
  ('m2', 1, 'd0000000-0000-0000-0000-00000000000a', 'm2-v1'),
  ('m3', 1, 'd0000000-0000-0000-0000-00000000000a', 'm3-v1');
insert into yogurt_pushes (device_id) values ('d0000000-0000-0000-0000-00000000000b');
insert into yogurt_versions (meeting_id, push_id, device_id, ct) values
  ('m1', 2, 'd0000000-0000-0000-0000-00000000000b', 'm1-v2'),
  ('m2', 2, 'd0000000-0000-0000-0000-00000000000b', null);
insert into yogurt_pushes (device_id) values ('d0000000-0000-0000-0000-00000000000a');
insert into yogurt_versions (meeting_id, push_id, device_id, ct) values
  ('m3', 3, 'd0000000-0000-0000-0000-00000000000a', null),
  ('m4', 3, 'd0000000-0000-0000-0000-00000000000a', 'm4-v1');

do $$
declare got text;
begin
  select string_agg(meeting_id || '=' || coalesce(ct, 'NULL'), ',' order by meeting_id) into got from yogurt_as_of();
  if got is distinct from 'm1=m1-v2,m4=m4-v1' then raise exception 'as_of(null): %', got; end if;

  select string_agg(meeting_id || '=' || coalesce(ct, 'NULL'), ',' order by meeting_id) into got from yogurt_as_of(1);
  if got is distinct from 'm1=m1-v1,m2=m2-v1,m3=m3-v1' then raise exception 'as_of(1): %', got; end if;

  select string_agg(meeting_id || '=' || coalesce(ct, 'NULL'), ',' order by meeting_id) into got from yogurt_as_of(2);
  if got is distinct from 'm1=m1-v2,m3=m3-v1' then raise exception 'as_of(2): %', got; end if;

  select string_agg(id || ':' || changed || '/' || deleted, ',' order by id) into got from yogurt_push_log;
  if got is distinct from '1:3/0,2:1/1,3:1/1' then raise exception 'push_log: %', got; end if;

  if (select count(*) from yogurt_devices) <> 2 then raise exception 'A should see 2 devices'; end if;
  if (select count(*) from yogurt_keyring) <> 1 then raise exception 'A should see 1 keyring row'; end if;
end $$;

-- A device id is the laptop's to choose, so an insert without one fails.
select pg_temp.expect('23502', array['insert into yogurt_devices (name_ct) values (''x'')']);

-- Nothing beyond select and insert on the granted columns.
select pg_temp.expect('42501', array[
  'update yogurt_devices set name_ct = ''x''',
  'delete from yogurt_devices',
  'update yogurt_keyring set public_key = ''x''',
  'delete from yogurt_keyring',
  'update yogurt_pushes set pushed_at = now()',
  'delete from yogurt_pushes',
  'update yogurt_versions set ct = ''x''',
  'delete from yogurt_versions',
  'truncate yogurt_devices',
  'truncate yogurt_keyring',
  'truncate yogurt_pushes',
  'truncate yogurt_versions',
  'insert into yogurt_pushes (id, device_id) overriding system value values (99, ''d0000000-0000-0000-0000-00000000000a'')',
  'insert into yogurt_pushes (device_id, pushed_at) values (''d0000000-0000-0000-0000-00000000000a'', now() - interval ''5 hours'')',
  'insert into yogurt_keyring (id, public_key, wrapped_private_key, kdf) overriding system value values (99, ''p'', ''w'', ''{}'')',
  'insert into yogurt_devices (id, name_ct, created_at) values (gen_random_uuid(), ''x'', now())',
  'insert into yogurt_versions (meeting_id, push_id, device_id, ct, user_id) values (''m9'', 3, ''d0000000-0000-0000-0000-00000000000a'', ''x'', ''bbbbbbbb-0000-0000-0000-000000000002'')'
]);

-- A version cannot join a push older than an hour, a push of another device, or a missing push.
reset role;
insert into yogurt_pushes (id, device_id, user_id, pushed_at) overriding system value
  values (100, 'd0000000-0000-0000-0000-00000000000a', 'aaaaaaaa-0000-0000-0000-000000000001', now() - interval '2 hours');
set role authenticated;
select pg_temp.expect('42501', array[
  'insert into yogurt_versions (meeting_id, push_id, device_id, ct) values (''m1'', 100, ''d0000000-0000-0000-0000-00000000000a'', ''forged'')',
  'insert into yogurt_versions (meeting_id, push_id, device_id, ct) values (''m1'', 3, ''d0000000-0000-0000-0000-00000000000b'', ''forged'')',
  'insert into yogurt_versions (meeting_id, push_id, device_id, ct) values (''m1'', 4242, ''d0000000-0000-0000-0000-00000000000a'', ''forged'')'
]);

-- User B sees nothing of A's and cannot write into A's rows.
set request.jwt.claim.sub = 'bbbbbbbb-0000-0000-0000-000000000002';
do $$
declare n bigint;
begin
  select (select count(*) from yogurt_devices) + (select count(*) from yogurt_keyring)
       + (select count(*) from yogurt_pushes) + (select count(*) from yogurt_versions)
       + (select count(*) from yogurt_as_of()) + (select count(*) from yogurt_as_of(1))
       + (select count(*) from yogurt_push_log) into n;
  if n <> 0 then raise exception 'user B sees % rows', n; end if;
end $$;
select pg_temp.expect('42501', array[
  'insert into yogurt_devices (id, name_ct, user_id) values (gen_random_uuid(), ''evil'', ''aaaaaaaa-0000-0000-0000-000000000001'')',
  'insert into yogurt_keyring (public_key, wrapped_private_key, kdf, user_id) values (''p'', ''w'', ''{}'', ''aaaaaaaa-0000-0000-0000-000000000001'')',
  'insert into yogurt_pushes (device_id) values (''d0000000-0000-0000-0000-00000000000a'')',
  'insert into yogurt_versions (meeting_id, push_id, device_id, ct) values (''m1'', 3, ''d0000000-0000-0000-0000-00000000000a'', ''evil'')'
]);

-- anon has no access at all.
reset role;
set role anon;
select pg_temp.expect('42501', array[
  'select count(*) from yogurt_devices',
  'select count(*) from yogurt_keyring',
  'select count(*) from yogurt_pushes',
  'select count(*) from yogurt_versions',
  'select count(*) from yogurt_push_log',
  'select count(*) from yogurt_as_of()',
  'insert into yogurt_pushes (device_id) values (''d0000000-0000-0000-0000-00000000000a'')',
  'truncate yogurt_versions'
]);

reset role;
\o
\echo schema_test: ok
