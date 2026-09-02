#!/usr/bin/env bash
# Shared port-busy handler for run-*.sh scripts.
#
# Source this file, then call:
#   PORT=$(ensure_port_free "<role>" <wanted_port>)
# Returns a free port on stdout. May prompt the user.
#
# Modes (via $YOGURT_PORT_POLICY env var, default "ask"):
#   ask    - interactively prompt: kill / new port / abort  (default)
#   kill   - silently kill the holder
#   next   - silently pick the next free port (wanted+1, +2, …, up to +20)
#   fail   - print and exit 1
#
# Inputs:
#   $1 = role label (e.g. "backend", "vite", shown in the prompt)
#   $2 = wanted port

# Pick the right TTY for prompts. When the parent script reads its stdout
# (e.g. `PORT=$(ensure_port_free ...)`), stdin/stdout are pipes; prompt
# the controlling tty instead.
_ypg_tty() {
  if [ -t 0 ]; then echo "/dev/tty"; else echo "/dev/tty"; fi
}

_ypg_holder_pid() {
  # macOS: lsof returns the listening PID (first match wins)
  lsof -nP -iTCP:"$1" -sTCP:LISTEN -F p 2>/dev/null | awk '/^p/ {sub("p",""); print; exit}'
}

_ypg_holder_cmd() {
  # Friendly process name for the prompt
  ps -o command= -p "$1" 2>/dev/null | head -c 80
}

ensure_port_free() {
  local role="$1"
  local wanted="$2"
  local policy="${YOGURT_PORT_POLICY:-ask}"

  local pid
  pid="$(_ypg_holder_pid "$wanted")"
  if [ -z "$pid" ]; then
    echo "$wanted"
    return 0
  fi

  local cmd
  cmd="$(_ypg_holder_cmd "$pid")"

  case "$policy" in
    kill)
      kill -9 "$pid" 2>/dev/null || true
      sleep 0.5
      echo "$wanted"
      return 0
      ;;
    next)
      _ypg_next_free "$role" "$wanted"
      return $?
      ;;
    fail)
      printf '\033[31m✗\033[0m port %d (%s) is busy - held by PID %s (%s)\n' \
        "$wanted" "$role" "$pid" "$cmd" >&2
      return 1
      ;;
    ask|*)
      _ypg_ask "$role" "$wanted" "$pid" "$cmd"
      return $?
      ;;
  esac
}

_ypg_next_free() {
  local role="$1"
  local start="$2"
  local p
  for ((i=1; i<=20; i++)); do
    p=$((start + i))
    if [ -z "$(_ypg_holder_pid "$p")" ]; then
      echo "$p"
      return 0
    fi
  done
  printf '\033[31m✗\033[0m no free port found in [%d..%d] for %s\n' \
    "$((start+1))" "$((start+20))" "$role" >&2
  return 1
}

_ypg_ask() {
  local role="$1"
  local wanted="$2"
  local pid="$3"
  local cmd="$4"
  local tty
  tty="$(_ypg_tty)"

  if [ ! -t 0 ] && [ ! -r "$tty" ]; then
    # No interactive terminal - fall through to fail with hint
    printf '\033[31m✗\033[0m port %d (%s) busy (PID %s: %s) and no TTY for prompt.\n' \
      "$wanted" "$role" "$pid" "$cmd" >&2
    printf '   set YOGURT_PORT_POLICY=kill or =next to choose non-interactively.\n' >&2
    return 1
  fi

  printf '\n\033[33m!\033[0m port %d (%s) is busy - held by PID %s\n' \
    "$wanted" "$role" "$pid" >&2
  printf '   process: %s\n' "$cmd" >&2
  printf '\n   [k] kill it       (kill -9 %s)\n' "$pid" >&2
  printf '   [n] use next port (try %d, %d, …)\n' "$((wanted+1))" "$((wanted+2))" >&2
  printf '   [a] abort\n\n' >&2

  local choice
  while true; do
    printf '   choose [k/n/a]: ' >&2
    read -r choice < "$tty" || { echo "" >&2; return 1; }
    case "$choice" in
      k|K|kill)
        kill -9 "$pid" 2>/dev/null || true
        sleep 0.5
        if [ -n "$(_ypg_holder_pid "$wanted")" ]; then
          printf '\033[31m✗\033[0m kill -9 PID %s failed - still listening on %d\n' "$pid" "$wanted" >&2
          return 1
        fi
        printf '\033[32m✓\033[0m killed PID %s; using port %d\n' "$pid" "$wanted" >&2
        echo "$wanted"
        return 0
        ;;
      n|N|next)
        local nxt
        nxt="$(_ypg_next_free "$role" "$wanted")" || return 1
        printf '\033[32m✓\033[0m using port %s\n' "$nxt" >&2
        echo "$nxt"
        return 0
        ;;
      a|A|abort)
        printf 'aborted.\n' >&2
        return 1
        ;;
      *)
        printf '   pick k, n, or a.\n' >&2
        ;;
    esac
  done
}
