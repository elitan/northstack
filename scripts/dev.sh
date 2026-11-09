#!/usr/bin/env bash
# Local dev environment, built for agents: every command returns when its
# result is ready, and logs go to files instead of a terminal stream.
#
#   scripts/dev.sh [start]   start Postgres, the api and Vite in the background;
#                            returns once both answer (idempotent)
#   scripts/dev.sh stop      stop the api and Vite (Postgres keeps running)
#   scripts/dev.sh status    what is running, and where
#   scripts/dev.sh reload    restart the api with the current binary and wait
#                            until it is ready (`just check` does this for you)
#   scripts/dev.sh logs      recent warnings and errors from the api and Vite
#   scripts/dev.sh logs all  the last 100 lines of each log
#
# State lives in .dev/: api.pid, web.pid, api.log, web.log, and `active`, which
# exists between `start` and `stop` and tells `just check` to reload the api
# (also after a crash, so the next passing check brings it back).

set -euo pipefail
cd "$(dirname "$0")/.."

DIR=.dev
mkdir -p "$DIR"

die() { printf 'error: %s\n' "$*" >&2; exit 1; }

load_env() {
  if [[ ! -f .env ]]; then
    cp .env.example .env
    echo "created .env from .env.example"
  fi
  set -a; . ./.env; set +a
  : "${DATABASE_URL:?DATABASE_URL is not set in .env}"
  export PORT="${PORT:-3000}" WEB_PORT="${WEB_PORT:-5173}"
}

# Homebrew's rustup is keg-only; find it even if it isn't on PATH.
if ! command -v cargo >/dev/null && [[ -x /opt/homebrew/opt/rustup/bin/cargo ]]; then
  export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fi

# --- process helpers ---------------------------------------------------------
# Each service runs in its own process group (set -m), so stopping it also
# stops its children (bun -> vite).

pid_of() { cat "$DIR/$1.pid" 2>/dev/null || true; }
running() { local p; p=$(pid_of "$1"); [[ -n $p ]] && kill -0 "$p" 2>/dev/null; }

stop_one() {
  local p; p=$(pid_of "$1")
  if [[ -n $p ]] && kill -0 "$p" 2>/dev/null; then
    kill -TERM -- "-$p" 2>/dev/null || kill -TERM "$p" 2>/dev/null || true
    for _ in $(seq 1 50); do kill -0 "$p" 2>/dev/null || break; sleep 0.1; done
    kill -KILL -- "-$p" 2>/dev/null || true
  fi
  rm -f "$DIR/$1.pid"
}

port_busy() { lsof -iTCP:"$1" -sTCP:LISTEN -n -P >/dev/null 2>&1; }
port_owner() { lsof -iTCP:"$1" -sTCP:LISTEN -n -P | awk 'NR==2{print $1" (pid "$2")"}'; }

# wait_for <name> <url> <seconds>
wait_for() {
  local name=$1 url=$2 secs=$3
  for _ in $(seq 1 $((secs * 10))); do
    curl -sf -o /dev/null "$url" && return 0
    running "$name" || return 1
    sleep 0.1
  done
  return 1
}

start_api() {
  [[ -x target/debug/app ]] || die "target/debug/app is missing; run 'just check'"
  port_busy "$PORT" && die "port $PORT is in use by $(port_owner "$PORT")"
  : >"$DIR/api.log"
  # `just logs` shows Vite lines only from here on (older ones are stale).
  wc -l <"$DIR/web.log" 2>/dev/null | tr -d ' ' >"$DIR/web.mark" || echo 0 >"$DIR/web.mark"
  set -m
  NO_COLOR=1 LOG_FORMAT=pretty nohup ./target/debug/app >>"$DIR/api.log" 2>&1 </dev/null &
  echo $! >"$DIR/api.pid"
  set +m
  if ! wait_for api "http://localhost:$PORT/readyz" 30; then
    echo "api failed to start; last lines of $DIR/api.log:" >&2
    tail -n 30 "$DIR/api.log" >&2
    stop_one api
    return 1
  fi
}

start_web() {
  port_busy "$WEB_PORT" && die "port $WEB_PORT is in use by $(port_owner "$WEB_PORT")"
  : >"$DIR/web.log"
  set -m
  (cd web && NO_COLOR=1 exec nohup bun run dev --clearScreen false --port "$WEB_PORT") \
    >>"$DIR/web.log" 2>&1 </dev/null &
  echo $! >"$DIR/web.pid"
  set +m
  if ! wait_for web "http://localhost:$WEB_PORT/" 30; then
    echo "vite failed to start; last lines of $DIR/web.log:" >&2
    tail -n 30 "$DIR/web.log" >&2
    stop_one web
    return 1
  fi
}

# --- commands ----------------------------------------------------------------

cmd_start() {
  local missing=()
  command -v cargo >/dev/null || missing+=("rust: brew install rustup && rustup default stable")
  command -v bun >/dev/null || missing+=("bun: brew install oven-sh/bun/bun")
  if ((${#missing[@]})); then printf 'missing tools:\n'; printf '  %s\n' "${missing[@]}"; exit 1; fi

  load_env

  # Postgres: use DATABASE_URL if reachable, otherwise start compose.yaml.
  local host port
  host=$(sed -E 's#^[a-z]+://([^@/]*@)?([^:/?]+).*#\2#' <<<"$DATABASE_URL")
  port=$(sed -nE 's#^[a-z]+://([^@/]*@)?[^:/?]+:([0-9]+).*#\2#p' <<<"$DATABASE_URL")
  export DB_PORT=${port:-5432}
  if ! (exec 3<>"/dev/tcp/$host/$DB_PORT") 2>/dev/null; then
    [[ $host == localhost || $host == 127.0.0.1 ]] \
      || die "can't reach Postgres at $host:$DB_PORT (DATABASE_URL in .env)"
    command -v docker >/dev/null || die "Postgres isn't running and Docker isn't installed"
    docker info >/dev/null 2>&1 || die "Postgres isn't running and the Docker daemon isn't either; start Docker"
    echo "starting Postgres (docker compose, port $DB_PORT)"
    docker compose up -d --wait --quiet-pull db >/dev/null 2>&1 \
      || die "docker compose failed; run 'docker compose up db' to see why"
  fi

  if [[ ! -d web/node_modules || web/bun.lock -nt web/node_modules ]]; then
    echo "installing frontend dependencies"
    (cd web && bun install --silent && touch node_modules)
  fi

  touch "$DIR/active"
  if ! running api; then
    # Migrate, regenerate, compile and typecheck before starting anything.
    ./scripts/check.sh --no-reload || exit 1
    start_api
  fi
  running web || start_web
  cmd_status
}

cmd_stop() {
  rm -f "$DIR/active"
  stop_one api
  stop_one web
  echo "stopped api and web (Postgres keeps running: 'docker compose stop')"
}

cmd_status() {
  load_env >/dev/null

  if running api; then
    printf 'api  running  http://localhost:%s  pid %s  log %s/api.log\n' "$PORT" "$(pid_of api)" "$DIR"
  else
    printf 'api  stopped\n'
  fi
  if running web; then
    printf 'web  running  http://localhost:%s  pid %s  log %s/web.log\n' "$WEB_PORT" "$(pid_of web)" "$DIR"
  else
    printf 'web  stopped\n'
  fi
  return 0
}

cmd_reload() {
  load_env >/dev/null
  [[ -f $DIR/active ]] || { echo "dev environment is not started ('just dev')"; return 0; }
  stop_one api
  start_api
}

cmd_logs() {
  local f name
  for name in api web; do
    f="$DIR/$name.log"
    [[ -f $f ]] || continue
    if [[ ${1:-} == all ]]; then
      echo "== $name ($f, last 100 lines)"
      tail -n 100 "$f"
    else
      # Only lines since the api last (re)started.
      local from=1 hits
      [[ $name == web ]] && from=$(( $(cat "$DIR/web.mark" 2>/dev/null || echo 0) + 1 ))
      hits=$(tail -n "+$from" "$f" | grep -E 'WARN|ERROR|panicked|[Ee]rror|failed' | tail -n 30 || true)
      if [[ -n $hits ]]; then
        echo "== $name: warnings and errors since the api started"
        echo "$hits"
      else
        echo "== $name: no warnings or errors since the api started"
      fi
    fi
  done
}

case "${1:-start}" in
  start) cmd_start ;;
  stop) cmd_stop ;;
  status) cmd_status ;;
  reload) cmd_reload ;;
  logs) shift; cmd_logs "$@" ;;
  *) die "unknown command '$1' (start | stop | status | reload | logs [all])" ;;
esac
