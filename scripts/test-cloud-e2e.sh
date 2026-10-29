#!/usr/bin/env bash
# Drives the real yogurt binary through Yogurt Cloud sync against the local Supabase stack (crates/yogurt-cloud/supabase, project yogurt-cloud-test, ports 56420-56429).
# Two instances with their own HOME and data dir play Laptop 1 and Laptop 2. A loopback server stands in for the deployed webapp's config.json.
# Run it with `just test-cloud-e2e`. Needs Docker, the supabase CLI, curl, jq, python3, and a built target/debug/yogurt.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

wd=crates/yogurt-cloud
db_container=supabase_db_yogurt-cloud-test
bin="$PWD/target/debug/yogurt"
OWNER=owner@yogurt-cloud.test
STRANGER=stranger@yogurt-cloud.test
PASSWORD='e2e-password-123'
PIN=4321

if ! docker ps --format '{{.Names}}' | grep -qx "$db_container"; then
    supabase start --workdir "$wd" -x studio,imgproxy,realtime,storage-api,edge-runtime,logflare,vector,supavisor,mailpit,postgres-meta
fi
eval "$(supabase status --workdir "$wd" -o env)"

psql() { docker exec -i -e PGOPTIONS='-c client_min_messages=warning' "$db_container" psql -U postgres -v ON_ERROR_STOP=1 -q -At "$@"; }

tmp=$(mktemp -d)
pids=()
cleanup() {
    for p in "${pids[@]:-}"; do
        [ -n "$p" ] || continue
        kill "$p" 2>/dev/null || true
        wait "$p" 2>/dev/null || true
    done
    rm -rf "$tmp"
}
trap cleanup EXIT

fail() { echo "FAIL: $*" >&2; exit 1; }
expect() { [ "$2" = "$3" ] || fail "$1: expected '$3', got '$2'"; }

psql <<'SQL'
drop table if exists yogurt_versions, yogurt_pushes, yogurt_devices, yogurt_browsers, yogurt_owner cascade;
delete from auth.users;
SQL
psql < "$wd/schema.sql"
sed "s/[$]1/'$OWNER'/" "$wd/owner.sql" | psql

make_user() {
    curl -sS -o /dev/null -X POST "$API_URL/auth/v1/admin/users" -H "apikey: $SECRET_KEY" -H "Authorization: Bearer $SECRET_KEY" \
        -H 'Content-Type: application/json' -d "{\"email\":\"$1\",\"password\":\"$PASSWORD\",\"email_confirm\":true}"
}
make_user "$OWNER"
make_user "$STRANGER"

# The stub webapp: only /config.json matters to a laptop.
mkdir "$tmp/site"
printf '{"v":1,"supabase_url":"%s","publishable_key":"%s"}' "$API_URL" "$PUBLISHABLE_KEY" > "$tmp/site/config.json"
free_port() { python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'; }
site_port=$(free_port)
python3 -m http.server "$site_port" --bind 127.0.0.1 --directory "$tmp/site" >/dev/null 2>&1 &
pids+=($!)
ADDRESS="http://127.0.0.1:$site_port"

# Each laptop gets its own HOME, so notes, the session token, and keys.json never touch ~/.yogurt.
start_laptop() { # name -> sets <name>_port
    local name=$1 port
    port=$(free_port)
    mkdir -p "$tmp/$name/home" "$tmp/$name/data"
    HOME="$tmp/$name/home" YOGURT_DATA_DIR="$tmp/$name/data" "$bin" start --port "$port" --no-open >"$tmp/$name/log" 2>&1 &
    pids+=($!)
    for _ in $(seq 100); do curl -fs "http://127.0.0.1:$port/api/health" >/dev/null 2>&1 && break; sleep 0.2; done
    curl -fs "http://127.0.0.1:$port/api/health" >/dev/null || { cat "$tmp/$name/log"; fail "$name did not start"; }
    eval "${name}_port=$port"
    eval "${name}_token=\$(cat \"$tmp/$name/data/session-token\")"
}
start_laptop a
start_laptop b

# api <laptop> <method> <path> [json]: prints the status code, leaves the body in $tmp/body.json.
api() {
    local name=$1 method=$2 path=$3 body=${4:-} port token
    eval "port=\$${name}_port token=\$${name}_token"
    curl -sS -o "$tmp/body.json" -w '%{http_code}' -X "$method" "http://127.0.0.1:$port$path" -H "Authorization: Bearer $token" \
        ${body:+-H 'Content-Type: application/json' -d "$body"}
    cat "$tmp/body.json" >> "$tmp/responses.txt"
}
# push_wait <laptop>: starts a push, prints the start request's status code, then waits for the background push to end and leaves the final GET /api/cloud in $tmp/body.json.
push_wait() {
    local name=$1 code
    code=$(api "$name" POST /api/cloud/push)
    [ "$code" = 202 ] || { echo "$code"; return; }
    for _ in $(seq 300); do
        api "$name" GET /api/cloud >/dev/null
        [ "$(jq -r .push "$tmp/body.json")" = null ] && break
        sleep 0.2
    done
    echo "$code"
}
field() { jq -r "$1" "$tmp/body.json"; }
connect_body() { printf '{"address":"%s","email":"%s","password":"%s","machine_name":"%s"}' "$ADDRESS" "$1" "$PASSWORD" "$2"; }
ctl() { local name=$1; shift; eval "HOME=\$tmp/$name/home YOGURT_DATA_DIR=\$tmp/$name/data \"\$bin\" ctl --port \$${name}_port \"\$@\""; }

echo "== connect is refused until the owner has a PIN"
expect "connect before PIN" "$(api a POST /api/cloud/connect "$(connect_body "$OWNER" 'Laptop 1')")" 412
expect "code" "$(field .code)" passcode_not_set
expect "message" "$(field .error)" "Choose a PIN in the webapp first"

owner_token=$(curl -fsS -X POST "$API_URL/auth/v1/token?grant_type=password" -H "apikey: $PUBLISHABLE_KEY" -H 'Content-Type: application/json' \
    -d "{\"email\":\"$OWNER\",\"password\":\"$PASSWORD\"}" | jq -r .access_token)
expect "set_passcode" "$(curl -fsS -X POST "$API_URL/rest/v1/rpc/yogurt_set_passcode" -H "apikey: $PUBLISHABLE_KEY" -H "Authorization: Bearer $owner_token" \
    -H 'Content-Type: application/json' -d "{\"new_passcode\":\"$PIN\"}")" true

echo "== a login that does not own the cloud is refused"
expect "stranger" "$(api a POST /api/cloud/connect "$(connect_body "$STRANGER" 'Laptop 1')")" 403
expect "code" "$(field .code)" not_owner
expect "message" "$(field .error)" "This login does not own that Yogurt Cloud"
expect "wrong password" "$(api a POST /api/cloud/connect "$(connect_body "$OWNER" 'Laptop 1' | sed "s/$PASSWORD/nope/")")" 401
expect "code" "$(field .code)" bad_credentials
expect "no device rows yet" "$(psql -c 'select count(*) from yogurt_devices')" 0

echo "== two laptops connect"
mk_meeting() { # laptop title -> prints id
    local name=$1 title=$2 notes="$tmp/notes-$RANDOM.md"
    echo "Notes for $title" > "$notes"
    ctl "$name" --json meeting new --title "$title" --notes-file "$notes" 2>/dev/null | jq -r '.id // .meeting.id'
}
a1=$(mk_meeting a "Standup A"); a2=$(mk_meeting a "Retro A"); b1=$(mk_meeting b "Planning B")
[ -n "$a1" ] && [ -n "$a2" ] && [ -n "$b1" ] || fail "could not seed meetings"

expect "connect a" "$(api a POST /api/cloud/connect "$(connect_body "$OWNER" 'Laptop 1')")" 200
expect "connected" "$(field .connected)" true
expect "machine" "$(field .machine_name)" "Laptop 1"
expect "not pushed" "$(field .not_pushed)" 2
expect "connect b" "$(api b POST /api/cloud/connect "$(connect_body "$OWNER" 'Laptop 2')")" 200
expect "connect twice" "$(api a POST /api/cloud/connect "$(connect_body "$OWNER" 'Laptop 1')")" 409
expect "devices" "$(psql -c 'select string_agg(name, $$,$$ order by name) from yogurt_devices')" "Laptop 1,Laptop 2"

echo "== keys.json holds one yogurt-cloud account and no secret leaks elsewhere"
keys="$tmp/a/data/keys.json"
expect "keys account" "$(jq -r '."yogurt-cloud" | fromjson | keys | join(",")' "$keys")" "address,device_id,email,machine_name,publishable_key,refresh_token,supabase_url"
expect "keys mode" "$(stat -f %Lp "$keys" 2>/dev/null || stat -c %a "$keys")" 600
refresh_a=$(jq -r '."yogurt-cloud" | fromjson | .refresh_token' "$keys")
sqlite3 "$tmp/a/data/db.sqlite" .dump > "$tmp/a.dump"
grep -qF "Standup A" "$tmp/a.dump" || fail "the SQLite dump is missing the seeded meeting, so the secret check would pass vacuously"
! grep -qF -e "$PASSWORD" -e "$refresh_a" "$tmp/a.dump" || fail "a secret reached SQLite"

echo "== first push"
expect "push a" "$(push_wait a)" 202
expect "changed" "$(field .last_push.changed)" 2
expect "deleted" "$(field .last_push.deleted)" 0
expect "nothing left running" "$(field .push)" null
expect "versions" "$(psql -c "select count(*) from yogurt_versions v join yogurt_devices d on d.id=v.device_id where d.name='Laptop 1'")" 2
expect "body shape" "$(psql -c "select body->>'v' || ':' || (body->'meeting'->>'title') from yogurt_versions where meeting_id='$a1'")" "1:Standup A"
expect "the push indexed both for search" "$(psql -c "select count(*) from yogurt_private.yogurt_search_index i join yogurt_devices d on d.id=i.device_id where d.name='Laptop 1'")" 2
expect "not pushed after push" "$(field .not_pushed)" 0
expect "last push" "$(field '.last_push.changed')" 2
[ "$(jq -r '."yogurt-cloud" | fromjson | .refresh_token' "$keys")" != "$refresh_a" ] || fail "refresh token did not rotate"

echo "== a pass that changes nothing writes nothing"
pushes=$(psql -c 'select count(*) from yogurt_pushes')
expect "push again" "$(push_wait a)" 202
expect "changed" "$(field .last_push.changed)" 0
expect "pushes unchanged" "$(psql -c 'select count(*) from yogurt_pushes')" "$pushes"

echo "== edit one meeting (with a NUL character), delete another, push"
expect "patch" "$(api a PATCH "/api/meetings/$a1" '{"notes_md":"edited\u0000 notes"}')" 200
expect "pill" "$(api a GET /api/cloud)" 200
expect "not pushed after edit" "$(field .not_pushed)" 1
[ "$(field .not_pushed_bytes)" -gt 0 ] || fail "no size for the edited meeting"
expect "ctl shows the size and time" "$(ctl a cloud status | grep -o 'KB, under a minute to push')" "KB, under a minute to push"
ctl a meeting delete "$a2" --yes >/dev/null
expect "pill" "$(api a GET /api/cloud)" 200
expect "a local deletion counts as not pushed" "$(field .not_pushed)" 2
expect "push edit+delete" "$(push_wait a)" 202
expect "changed" "$(field .last_push.changed)" 1
expect "deleted" "$(field .last_push.deleted)" 1
expect "edited body, NUL stripped" "$(psql -c "select body->'meeting'->>'notes_md' from yogurt_versions where meeting_id='$a1' order by push_id desc limit 1")" "edited notes"
expect "pill after the deletion marker went out" "$(api a GET /api/cloud)" 200
expect "not pushed" "$(field .not_pushed)" 0
expect "deletion marker" "$(psql -c "select count(*) from yogurt_versions where meeting_id='$a2' and body is null")" 1
expect "the index follows the edit and the deletion" "$(psql -c "select string_agg(meeting_id || ':' || (doc ~ 'edited notes'), ',') from yogurt_private.yogurt_search_index where meeting_id in ('$a1', '$a2')")" "$a1:true"

echo "== two devices push into one cloud, and two overlapping pushes both succeed"
expect "push b" "$(push_wait b)" 202
expect "b versions" "$(psql -c "select count(*) from yogurt_versions v join yogurt_devices d on d.id=v.device_id where d.name='Laptop 2' and body is not null")" 1
expect "devices that pushed" "$(psql -c 'select count(distinct device_id) from yogurt_pushes')" 2
expect "patch" "$(api a PATCH "/api/meetings/$a1" '{"notes_md":"edited again"}')" 200
push_code() { curl -sS -o /dev/null -w '%{http_code}' -X POST "http://127.0.0.1:$a_port/api/cloud/push" -H "Authorization: Bearer $a_token"; }
push_code > "$tmp/c1" & p1=$!
push_code > "$tmp/c2" & p2=$!
wait "$p1" "$p2"
# The second request is refused with 409 while the first still runs, or starts a no-op push if the first already ended.
case "$(sort "$tmp/c1" "$tmp/c2" | tr '\n' ' ')" in "202 202 " | "202 409 ") ;; *) fail "overlapping pushes answered $(cat "$tmp/c1" "$tmp/c2")" ;; esac
for _ in $(seq 300); do
    api a GET /api/cloud >/dev/null
    [ "$(field .push)" = null ] && break
    sleep 0.2
done
expect "ctl status" "$(ctl a --json cloud status | jq -r .not_pushed)" 0
expect "ctl push" "$(ctl a --json cloud push | jq -r .last_push.changed)" 0

echo "== rename this Mac"
expect "rename" "$(api a POST /api/cloud/machine-name '{"machine_name":"  Work Mac "}')" 200
expect "renamed" "$(field .machine_name)" "Work Mac"
expect "device row" "$(psql -c 'select string_agg(name, $$,$$ order by name) from yogurt_devices')" "Laptop 2,Work Mac"
expect "keys.json" "$(jq -r '."yogurt-cloud" | fromjson | .machine_name' "$keys")" "Work Mac"
expect "blank name" "$(api a POST /api/cloud/machine-name '{"machine_name":" "}')" 400
expect "ctl rename" "$(ctl a --json cloud rename 'Laptop 1' | jq -r .machine_name)" "Laptop 1"

echo "== auto-push interval"
expect "default" "$(ctl a --json cloud status | jq -r .push_interval)" off
expect "bad interval" "$(api a POST /api/cloud/push-interval '{"interval":"5m"}')" 400
expect "set" "$(api a POST /api/cloud/push-interval '{"interval":"1h"}')" 200
expect "returned status" "$(field .push_interval)" 1h
expect "ctl shows it" "$(ctl a cloud status | grep '^auto-push:' | sed 's/, next push in .*//')" "auto-push: every hour"

echo "== disconnect with mark_deleted"
expect "disconnect a" "$(api a POST /api/cloud/disconnect '{"mark_deleted":true}')" 200
expect "disconnected" "$(field .connected)" false
expect "marked deleted" "$(psql -c "select count(*) from yogurt_versions v join yogurt_devices d on d.id=v.device_id where d.name='Laptop 1' and v.meeting_id='$a1' and v.body is null")" 1
expect "history keeps it" "$(psql -c "select count(*) from yogurt_versions where meeting_id='$a1' and body is not null")" 3
expect "disconnect resets the interval" "$(sqlite3 "$tmp/a/data/db.sqlite" "select value from settings where key='cloud.push_interval'")" off
expect "keys cleared" "$(jq -r 'has("yogurt-cloud")' "$keys")" false
expect "status" "$(api a GET /api/cloud)" 200
expect "not connected" "$(field .connected)" false
expect "push when disconnected" "$(api a POST /api/cloud/push)" 409
expect "interval while disconnected" "$(api a POST /api/cloud/push-interval '{"interval":"1h"}')" 409

echo "== a revoked session asks to sign in again, and sign-in restores it"
psql -c "delete from auth.sessions" >/dev/null
expect "push revoked" "$(push_wait b)" 202
expect "recorded" "$(field .last_error.code)" session_expired
expect "the earlier success is kept" "$(field '.last_push.changed')" 1
expect "sign-in wrong" "$(api b POST /api/cloud/sign-in '{"password":"nope"}')" 401
expect "sign-in" "$(api b POST /api/cloud/sign-in "{\"password\":\"$PASSWORD\"}")" 200
expect "sign-in clears the error and keeps the success" "$(field '[.last_error, .last_push.changed] | @json')" "[null,1]"
expect "push after sign-in" "$(push_wait b)" 202
expect "pushed" "$(field .last_error)" null

echo "== a removed device is reported"
psql -c "delete from yogurt_versions where device_id=(select id from yogurt_devices where name='Laptop 2'); delete from yogurt_pushes where device_id=(select id from yogurt_devices where name='Laptop 2'); delete from yogurt_devices where name='Laptop 2'" >/dev/null
expect "push removed" "$(push_wait b)" 202
expect "code" "$(field .last_error.code)" device_removed

echo "== no secret appeared in a response or a log"
refresh_b=$(jq -r '."yogurt-cloud" | fromjson | .refresh_token' "$tmp/b/data/keys.json")
for f in "$tmp/responses.txt" "$tmp/a/log" "$tmp/b/log"; do
    [ -s "$f" ] || fail "$f is empty, so the secret check would pass vacuously"
done
grep -qF '"connected"' "$tmp/responses.txt" || fail "responses.txt holds no cloud response"
for secret in "$PASSWORD" "$refresh_a" "$refresh_b"; do
    [ "${#secret}" -ge 8 ] || fail "secret under test is empty or short"
    ! grep -qF -- "$secret" "$tmp/responses.txt" "$tmp/a/log" "$tmp/b/log" || fail "a secret appeared in a response or log"
done
echo "test-cloud-e2e: ok"
