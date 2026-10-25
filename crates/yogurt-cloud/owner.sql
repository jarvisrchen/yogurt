-- Run by the build with $1 = the owner email. A changed email applies only until the owner signs in and user_id is pinned.
insert into public.yogurt_owner (email) values (trim($1))
on conflict (id) do update set email = excluded.email where public.yogurt_owner.user_id is null;
