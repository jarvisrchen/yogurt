#!/usr/bin/env bash
# yogurt task — task lifecycle: create/resume a worktree, list worktrees,
# run/stop a background dev server in tmux.
#
# BSD awk / BSD sed only (no gawk, no GNU-only flags) - stock macOS 13+
# toolchain. See docs/.planning/agent-workflow.md section 4A (A1, A3, A4)
# for the design this implements.
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: task.sh start <ID|slug> [words...]   create/resume a worktree + branch
       task.sh worktrees [--no-pr]          list every worktree
       task.sh dev-bg [name]                run `just dev` in a tmux window
       task.sh dev-stop [name]               stop that tmux window
EOF
}

die() {
  echo "error: $1" >&2
  [ -n "${2:-}" ] && echo "help: $2" >&2
  exit "${3:-1}"
}

# The main checkout, from any worktree - same trick `just bootstrap` uses.
# YOGURT_MAIN overrides it, for tests that run against a throwaway repo.
resolve_main() {
  local raw
  if [ -n "${YOGURT_MAIN:-}" ]; then
    raw="$YOGURT_MAIN"
  else
    raw="$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")"
  fi
  # -P: physical path, matching what `git worktree list` reports.
  (cd "$raw" && pwd -P)
}

worktrees_root() {
  # -P: physical path, matching what `git worktree list` reports (macOS
  # resolves /var -> /private/var, so a logical pwd would never match).
  printf '%s/yogurt-worktrees\n' "$(cd "$1/.." && pwd -P)"
}

# ── start ────────────────────────────────────────────────────────────

cmd_start() {
  local first="${1:-}"
  [ -z "$first" ] && { usage; exit 2; }
  shift

  local no_bootstrap=false
  local words=()
  local a
  for a in "$@"; do
    case "$a" in
      --no-bootstrap) no_bootstrap=true ;;
      *) words+=("$a") ;;
    esac
  done

  local main wroot
  main="$(resolve_main)"
  wroot="$(worktrees_root "$main")"

  git -C "$main" fetch origin --prune

  local id="" base name
  if [[ "$first" =~ ^[A-Z]{2,4}-[0-9]+$ ]]; then
    id="$first"
    "$main/scripts/ticket.sh" 2>/dev/null | cut -f1 | grep -qx "$id" ||
      die "$id is not an open ticket" "just ticket" 2
    base="$(printf '%s' "$id" | tr '[:upper:]' '[:lower:]')"
  elif [[ "$first" =~ ^[a-z0-9]+(-[a-z0-9]+)*$ ]]; then
    base="$first"
  else
    die "bad name '$first': give a ticket ID (XX-123) or a lowercase-and-dashes slug" "" 2
  fi

  name="$base"
  for a in "${words[@]:-}"; do
    [ -z "$a" ] && continue
    name="$name-$(printf '%s' "$a" | tr '[:upper:]' '[:lower:]')"
  done

  local dir="$wroot/$name"
  local branch_ref="refs/heads/$name"
  local dir_exists=false branch_exists=false
  [ -d "$dir" ] && dir_exists=true
  git -C "$main" show-ref --verify --quiet "$branch_ref" && branch_exists=true

  if $dir_exists && $branch_exists; then
    : # resume - bootstrap and reprint below
  elif $dir_exists && ! $branch_exists; then
    die "$name is claimed: $dir exists but branch $name does not" \
      "worktree cleanup was skipped without deleting the branch - remove the directory, or pick a different slug" 1
  elif ! $dir_exists && $branch_exists; then
    local hint="pick a different slug"
    git -C "$main" branch --merged origin/main --format='%(refname:short)' | grep -qx "$name" &&
      hint="git branch -D $name"
    die "$name is claimed: branch $name exists but $dir does not" "$hint" 1
  else
    git -C "$main" worktree add "$dir" -b "$name" origin/main
  fi

  if ! $no_bootstrap; then
    (cd "$dir" && just bootstrap)
  fi

  if [ -n "$id" ]; then
    "$main/scripts/ticket.sh" "$id"
    echo
  fi
  echo "cd $dir && just dev"
  echo "$dir"
}

# ── worktrees ────────────────────────────────────────────────────────

# Emits "cwd_path<TAB>port" for every process holding a LISTENing TCP
# socket, matched by its cwd. One `lsof` call for all listeners, one more
# per distinct holder pid.
build_port_map() {
  local listeners
  listeners="$(lsof -nP -iTCP -sTCP:LISTEN -F pcn 2>/dev/null || true)"
  [ -z "$listeners" ] && return 0

  local pid_ports
  pid_ports="$(printf '%s\n' "$listeners" | awk '
    /^p/ { pid = substr($0, 2); next }
    /^n/ {
      port = $0
      sub(/^n/, "", port)
      sub(/.*:/, "", port)
      if (pid != "" && port ~ /^[0-9]+$/) print pid "\t" port
    }
  ')"
  [ -z "$pid_ports" ] && return 0

  local pid cwd
  printf '%s\n' "$pid_ports" | cut -f1 | sort -un | while read -r pid; do
    [ -z "$pid" ] && continue
    cwd="$(lsof -a -p "$pid" -d cwd -F n 2>/dev/null | awk '/^n/{sub(/^n/,"");print;exit}')"
    [ -z "$cwd" ] && continue
    printf '%s\n' "$pid_ports" | awk -F'\t' -v pid="$pid" -v cwd="$cwd" '$1==pid{print cwd "\t" $2}'
  done
  true # a killed pid mid-loop must not trip `set -e` via the loop's status
}

# Runs "$@" with stdout captured, killed after $1 seconds if it hangs.
_bounded() {
  local secs="$1"; shift
  local out pid i max status
  out="$(mktemp)"
  ( "$@" >"$out" 2>/dev/null ) &
  pid=$!
  i=0
  max=$((secs * 10))
  while kill -0 "$pid" 2>/dev/null; do
    i=$((i + 1))
    if [ "$i" -ge "$max" ]; then
      kill -9 "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
      rm -f "$out"
      return 1
    fi
    sleep 0.1
  done
  wait "$pid"
  status=$?
  cat "$out"
  rm -f "$out"
  return $status
}

# "branch<TAB>number<TAB>state" for every PR gh knows about, one call.
fetch_pr_map() {
  local out
  out="$(_bounded 6 gh pr list --state all -L 100 --json number,state,headRefName)" || return 1
  [ -z "$out" ] && return 1
  printf '%s' "$out" | jq -r '.[] | "\(.headRefName)\t\(.number)\t\(.state)"'
}

# "number/state" for one branch not covered by the batch window.
pr_lookup_fallback() {
  local out
  out="$(_bounded 4 gh pr list --head "$1" --state all --json number,state)" || return 1
  [ -z "$out" ] || [ "$out" = "[]" ] && return 1
  printf '%s' "$out" | jq -r '.[0] | "\(.number)/\(.state)"'
}

cmd_worktrees() {
  local no_pr=false a
  for a in "$@"; do
    case "$a" in
      --no-pr) no_pr=true ;;
      *) usage; exit 2 ;;
    esac
  done

  local main wroot
  main="$(resolve_main)"
  wroot="$(worktrees_root "$main")"

  local list
  list="$(git -C "$main" worktree list --porcelain | awk '
    /^worktree / { if (path != "") print path "\t" branch; path = substr($0, 10); branch = "detached"; next }
    /^branch /   { b = substr($0, 8); sub(/^refs\/heads\//, "", b); branch = b; next }
    /^bare$/     { branch = "bare"; next }
    END { if (path != "") print path "\t" branch }
  ')"

  local have_gh=false
  command -v gh >/dev/null 2>&1 && have_gh=true
  local pr_map=""
  if ! $no_pr && $have_gh; then
    pr_map="$(fetch_pr_map || true)"
  fi

  local port_map
  port_map="$(build_port_map)"

  {
    printf '%s\t%s\t%s\t%s\t%s\n' path branch pr ports flags
    local path branch
    while IFS=$'\t' read -r path branch; do
      [ -z "$path" ] && continue

      local foreign=false
      case "$path" in
        "$wroot"/*) ;;
        *) foreign=true ;;
      esac

      local pr="?"
      if $no_pr || ! $have_gh; then
        pr="?"
      elif [ "$branch" = "detached" ] || [ "$branch" = "bare" ]; then
        pr="-"
      else
        local hit
        hit="$(printf '%s\n' "$pr_map" | awk -F'\t' -v b="$branch" '$1==b{print $2"/"$3; exit}')"
        if [ -z "$hit" ]; then
          hit="$(pr_lookup_fallback "$branch" || true)"
        fi
        pr="${hit:-none}"
      fi

      # `pnpm --dir web dev` runs Vite with cwd <worktree>/web, not the
      # worktree root, so match the holder's cwd under the worktree too.
      local ports
      ports="$(printf '%s\n' "$port_map" | awk -F'\t' -v p="$path" '$1==p || index($1,p"/")==1{print $2}' | sort -un | tr '\n' ',' | sed 's/,$//')"
      [ -z "$ports" ] && ports="-"

      local dirty=false
      [ -n "$(git -C "$path" status --porcelain 2>/dev/null)" ] && dirty=true

      local flags=""
      if $foreign; then
        flags="foreign"
      else
        $dirty && flags="dirty"
        if [ "${pr#*/}" = "MERGED" ] && ! $dirty && [ "$ports" = "-" ]; then
          flags="${flags:+$flags,}removable"
        fi
      fi
      [ -z "$flags" ] && flags="-"

      printf '%s\t%s\t%s\t%s\t%s\n' "$path" "$branch" "$pr" "$ports" "$flags"
    done <<< "$list"
  } | column -t -s "$(printf '\t')"
}

# ── dev-bg / dev-stop ────────────────────────────────────────────────

tmux_window_name() {
  local name="${1:-}"
  [ -z "$name" ] && name="$(basename "$PWD")"
  printf '%s' "$name" | tr '.:' '__'
}

cmd_dev_bg() {
  local name
  name="$(tmux_window_name "${1:-}")"
  local target="yogurt:$name"

  if tmux has-session -t yogurt 2>/dev/null; then
    if ! tmux list-windows -t yogurt -F '#{window_name}' 2>/dev/null | grep -qx "$name"; then
      tmux new-window -t yogurt -n "$name" -c "$PWD" 'just dev'
    fi
  else
    tmux new-session -d -s yogurt -n "$name" -c "$PWD" 'just dev'
  fi

  local pane="" backend_port="" vite_port="" i
  for i in $(seq 1 90); do
    pane="$(tmux capture-pane -p -t "$target" -S -200 2>/dev/null || true)"
    [ -z "$backend_port" ] && backend_port="$(printf '%s\n' "$pane" | sed -n 's/^YOGURT_PORT=\([0-9]*\).*/\1/p' | tail -1)"
    [ -z "$vite_port" ] && vite_port="$(printf '%s\n' "$pane" | sed -n 's/^YOGURT_VITE_PORT=\([0-9]*\).*/\1/p' | tail -1)"
    if [ -n "$backend_port" ] && curl -sf -o /dev/null "http://127.0.0.1:$backend_port/api/health" 2>/dev/null; then
      echo "backend=http://localhost:$backend_port vite=${vite_port:-?} tmux=$target"
      return 0
    fi
    sleep 1
  done

  echo "task: timed out waiting for $target to become healthy" >&2
  printf '%s\n' "$pane" | tail -20 >&2
  echo "tmux attach -t $target" >&2
  exit 1
}

# PIDs of LISTENing processes whose cwd is $1 or a subdirectory of it.
# `just dev`'s own EXIT/INT/TERM/HUP trap usually reaps its children when
# the tmux window dies, but `just` and its subshells sit between tmux and
# the actual backend/vite processes, so a signal can get lost in that
# chain - this is the belt-and-suspenders backstop dev-stop relies on.
_listener_pids_under() {
  local wt="$1" pid cwd
  lsof -nP -iTCP -sTCP:LISTEN -F p 2>/dev/null | awk '/^p/{print substr($0,2)}' | sort -un |
    while read -r pid; do
      [ -z "$pid" ] && continue
      cwd="$(lsof -a -p "$pid" -d cwd -F n 2>/dev/null | awk '/^n/{sub(/^n/,"");print;exit}')"
      case "$cwd" in
        "$wt"|"$wt"/*) echo "$pid" ;;
      esac
    done
  true # an empty match must not trip `set -e` via the while loop's status
}

cmd_dev_stop() {
  local name
  name="$(tmux_window_name "${1:-}")"
  tmux kill-window -t "yogurt:$name" 2>/dev/null || true

  local pid
  _listener_pids_under "$PWD" | while read -r pid; do
    [ -z "$pid" ] && continue
    kill -TERM "$pid" 2>/dev/null || true
  done || true
  sleep 0.3
  _listener_pids_under "$PWD" | while read -r pid; do
    [ -z "$pid" ] && continue
    kill -9 "$pid" 2>/dev/null || true
  done || true

  exit 0
}

main() {
  case "${1:-}" in
    start) shift; cmd_start "$@" ;;
    worktrees) shift; cmd_worktrees "$@" ;;
    dev-bg) shift; cmd_dev_bg "$@" ;;
    dev-stop) shift; cmd_dev_stop "$@" ;;
    -h|--help) usage ;;
    *) usage; exit 2 ;;
  esac
}

main "$@"
