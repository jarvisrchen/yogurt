-- Run after this branch's schema is applied over main's, after main's is applied over this branch's (a revert), and after this branch's is applied again.
-- Variables: phase (upgrade, revert, or reupgrade) and search (true when this branch's schema is the one applied).
\set ON_ERROR_STOP on
\set QUIET on
\o /dev/null
create or replace function pg_temp.eq(got text, want text, what text) returns void language plpgsql as $$
begin
  if got is distinct from want then raise exception '%: expected %, got %', what, want, got; end if;
end $$;

set role authenticated;
set request.jwt.claim.sub = 'aaaaaaaa-0000-0000-0000-000000000001';
select yogurt_start_push('00000000-0000-0000-0000-0000000000d1') as push \gset
-- A laptop can push a huge multibyte body, and its insert succeeds whichever schema is applied.
insert into yogurt_versions (meeting_id, push_id, device_id, body)
values (:'phase' || '-big', :push, '00000000-0000-0000-0000-0000000000d1',
  jsonb_build_object('v', 1, 'meeting', jsonb_build_object('title', 'Pushed after ' || :'phase', 'started_at', 3000, 'notes_md', :'phase' || 'word ' || repeat('日本語 é€ ', 400000), 'transcript_json', '[]')));
\if :search
select pg_temp.eq(yogurt_search_catch_up()::text, '0', 'the laptop catches search up after its push');
\endif
reset role;
select pg_temp.eq((select count(*)::text from yogurt_versions where meeting_id = :'phase' || '-big'), '1', 'the laptop insert landed');
\if :search
select pg_temp.eq((select extnamespace::regnamespace::text from pg_extension where extname = 'unaccent'), 'extensions', 'unaccent installed in public moves to extensions');
select pg_temp.eq((select count(*)::text from yogurt_private.yogurt_search_index), (select count(*)::text from (select distinct on (device_id, meeting_id) body from yogurt_versions order by device_id, meeting_id, push_id desc) l where body is not null), 'the index holds every live meeting');
set role authenticated;
select yogurt_unlock('123456') as tok \gset
select pg_temp.eq((select string_agg(meeting_id, ',' order by started_at desc) from yogurt_search(:'tok', 'upgradeword')), 'upgrade-big,u1', 'search finds the old and the new meeting, and leaves out the archived one');
select pg_temp.eq((select count(*)::text from yogurt_search(:'tok', 'revertword')), case when :'phase' = 'reupgrade' then '1' else '0' end, 'a meeting pushed during the revert is searchable once re-upgraded');
select pg_temp.eq((select string_agg(meeting_id, ',') from yogurt_search(:'tok', 'hugeword')), 'u2', 'search finds the huge meeting written before the upgrade');
select pg_temp.eq((select string_agg(meeting_id, ',' order by meeting_id) from yogurt_retrieve(:'tok', '{upgradeword}')), 'u1,upgrade-big', 'chat retrieval finds the old and the new meeting, and leaves out the archived one');
select pg_temp.eq((select string_agg(meeting_id, ',') from yogurt_search(:'tok', 'upgradeword', only_archived => true)) || ' ' || (select string_agg(meeting_id || ':' || total, ',') from yogurt_library(:'tok', only_archived => true)), 'u4 u4:1', 'a meeting archived before the upgrade is in the archive');
select pg_temp.eq((select string_agg(oid::regprocedure::text, ',' order by proname) from pg_proc where pronamespace = 'public'::regnamespace and proname in ('yogurt_library', 'yogurt_search')),
  'yogurt_library(text,bigint,integer,integer,uuid,boolean),yogurt_search(text,text,uuid,integer,boolean)', 'only the current signatures are left');
reset role;
\endif
\o
\echo upgrade_test :phase: ok
