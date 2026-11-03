#!/usr/bin/env bash
# `just cloud-dev`: a Yogurt Cloud on this Mac to try by hand, in desktop Chrome or the Android emulator.
# Its Supabase project (yogurt-cloud-dev, ports 56440-56450) is separate from the e2e stacks, so running tests never wipes it.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
WD=cloud/dev
WEB_PORT=56448
RELAY_PORT=56449
MAIL=http://localhost:56444
OWNER=${YOGURT_CLOUD_DEV_OWNER:-me@yogurt.local}

# The keys live in the main checkout's .env.local, which `just bootstrap` copies into a worktree.
MAIN="$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")"
ENV_FILE=.env.local
[ -f "$ENV_FILE" ] || ENV_FILE="$MAIN/.env.local"
[ -f "$ENV_FILE" ] || { echo "cloud-dev: no .env.local here or in $MAIN" >&2; exit 1; }
env_get() { sed -n "s/^$1=//p" "$ENV_FILE" | tail -1 | tr -d '"'"'"; }

# The LLM for enhance and chat: the first key in .env.local, in the order the Mac seeds its presets (ENV_PRESETS in bootstrap.rs).
LLM_BASE= LLM_MODEL= LLM_KEY= LLM_NAME=
while IFS='|' read -r var name base model; do
  key=$(env_get "$var")
  if [ -n "$key" ]; then LLM_BASE=$base LLM_MODEL=$model LLM_KEY=$key LLM_NAME=$name; break; fi
done < <(tr -d '\n' < crates/yogurt-server/src/bootstrap.rs \
  | grep -oE '\([[:space:]]*"YOGURT_[A-Z]+_API_KEY",[[:space:]]*"[^"]+",[[:space:]]*"[^"]+",[[:space:]]*"[^"]+",?[[:space:]]*\)' \
  | sed -E 's/\([[:space:]]*"([^"]+)",[[:space:]]*"([^"]+)",[[:space:]]*"([^"]+)",[[:space:]]*"([^"]+)",?[[:space:]]*\)/\1|\2|\3|\4/')
[ -n "$LLM_KEY" ] || echo "cloud-dev: no LLM key in $ENV_FILE, so Enhance and chat answer that they are not set up" >&2

if ! docker ps --format '{{.Names}}' | grep -qx supabase_db_yogurt-cloud-dev; then
  supabase start --workdir "$WD" -x studio,imgproxy,realtime,storage-api,edge-runtime,logflare,vector,supavisor,postgres-meta
fi
eval "$(supabase status --workdir "$WD" -o env)"
psql() { docker exec -i -e PGOPTIONS='-c client_min_messages=warning' supabase_db_yogurt-cloud-dev psql -U postgres -v ON_ERROR_STOP=1 -q "$@"; }
# Both are safe to rerun, as each Vercel build reruns them.
psql -1 < crates/yogurt-cloud/schema.sql
sed "s/[\$]1/'$OWNER'/" crates/yogurt-cloud/owner.sql | psql
for _ in $(seq 100); do
  [ "$(psql -tAc 'select yogurt_private.yogurt_search_catch_up(2000)')" = 0 ] && break
done

# localhost, not 127.0.0.1, so the emulator reaches it through adb reverse and the debug app accepts it.
SUPABASE="http://localhost:${API_URL##*:}"
pnpm --dir cloud install --frozen-lockfile >/dev/null
DIST=$ROOT/$WD/dist
YOGURT_CLOUD_DIST=$DIST SUPABASE_URL=$SUPABASE SUPABASE_PUBLISHABLE_KEY=$PUBLISHABLE_KEY pnpm --dir cloud build >/dev/null

GRANT_KEY=$(env_get YOGURT_DEEPGRAM_GRANT_KEY)
if [ -n "$GRANT_KEY" ]; then
  export YOGURT_DEEPGRAM_API_KEY=$GRANT_KEY
  DEEPGRAM="Deepgram's grant, with YOGURT_DEEPGRAM_GRANT_KEY"
else
  DG_KEY=$(env_get YOGURT_DEEPGRAM_API_KEY)
  [ -n "$DG_KEY" ] || { echo "cloud-dev: no YOGURT_DEEPGRAM_API_KEY or YOGURT_DEEPGRAM_GRANT_KEY in $ENV_FILE" >&2; exit 1; }
  YOGURT_DEEPGRAM_API_KEY=$DG_KEY node cloud/scripts/dev-deepgram-relay.mjs "$RELAY_PORT" &
  trap 'kill $(jobs -p) 2>/dev/null; wait 2>/dev/null; true' EXIT INT TERM HUP
  # The relay ignores the key the function sends it, so this only has to be set.
  export YOGURT_DEEPGRAM_API_KEY=dev-relay YOGURT_DEEPGRAM_URL=http://localhost:$RELAY_PORT
  DEEPGRAM="the dev relay on :$RELAY_PORT, with YOGURT_DEEPGRAM_API_KEY"
fi

cat <<EOF

Yogurt Cloud (dev)   http://localhost:$WEB_PORT
Mail catcher         $MAIL
Supabase             $SUPABASE
LLM                  ${LLM_NAME:-none} ${LLM_MODEL}
Transcription        $DEEPGRAM

First run:
  1. Open http://localhost:$WEB_PORT and create a login with $OWNER (any password of 6 or more characters).
  2. Open the confirmation email in $MAIL and click its link.
  3. Sign in and choose a PIN.
In the emulator: \`just android-emu\`, which opens this Cloud in the app.
Stop with Ctrl-C. The data stays until \`supabase stop --workdir $ROOT/$WD --no-backup\`.

EOF

cd cloud
SUPABASE_URL=$SUPABASE SUPABASE_PUBLISHABLE_KEY=$PUBLISHABLE_KEY \
  YOGURT_LLM_BASE_URL=$LLM_BASE YOGURT_LLM_MODEL=$LLM_MODEL YOGURT_LLM_API_KEY=$LLM_KEY \
  pnpm exec vite preview --outDir "$DIST" --host 127.0.0.1 --port "$WEB_PORT" --strictPort
