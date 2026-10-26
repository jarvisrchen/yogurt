-- What the build does on a deploy: the schema in one transaction, then the search index ensure as its own statement.
begin;
\i /schema.sql
commit;
select yogurt_private.yogurt_search_ensure() as ensured \gset
