#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"

# Ensure venv exists
if [ ! -d "$ROOT/.venv" ]; then
  echo "No .venv found — run: uv venv --python 3.11 && uv pip install -r requirements.txt -e app/"
  exit 1
fi

HOST="${HOST:-0.0.0.0}"
PORT="${PORT:-8000}"
RELOAD="${RELOAD:-true}"
JAEGER="${JAEGER:-true}"   # set JAEGER=false to skip docker compose

# ---------------------------------------------------------------------------
# Docker Compose (Jaeger) — start if available and not disabled
# ---------------------------------------------------------------------------

COMPOSE_STARTED=false

if [ "$JAEGER" = "true" ] && command -v docker >/dev/null 2>&1; then
  if docker compose -f "$ROOT/docker-compose.yml" up -d 2>&1; then
    COMPOSE_STARTED=true
    export OTEL_EXPORTER_OTLP_ENDPOINT="${OTEL_EXPORTER_OTLP_ENDPOINT:-http://localhost:4318}"
    echo "  Jaeger UI → http://localhost:16686"
    echo "  OTEL endpoint → $OTEL_EXPORTER_OTLP_ENDPOINT"
  else
    echo "  [warn] docker compose failed — continuing without Jaeger"
  fi
else
  [ "$JAEGER" != "true" ] && echo "  Jaeger disabled (JAEGER=false)"
  command -v docker >/dev/null 2>&1 || echo "  [warn] docker not found — skipping Jaeger"
fi

# ---------------------------------------------------------------------------
# Shutdown handler — Ctrl-C / SIGTERM
# ---------------------------------------------------------------------------

_shutdown() {
  echo ""
  echo "Shutting down…"
  # Kill uvicorn subprocess group if still running
  if [ -n "${UVICORN_PID:-}" ] && kill -0 "$UVICORN_PID" 2>/dev/null; then
    kill -TERM "$UVICORN_PID" 2>/dev/null || true
    wait "$UVICORN_PID" 2>/dev/null || true
  fi
  # Stop docker compose services
  if [ "$COMPOSE_STARTED" = "true" ]; then
    echo "Stopping Jaeger…"
    docker compose -f "$ROOT/docker-compose.yml" down 2>/dev/null || true
  fi
  exit 0
}

trap _shutdown INT TERM

# ---------------------------------------------------------------------------
# Start uvicorn in background so the trap fires immediately on Ctrl-C
# ---------------------------------------------------------------------------

echo "Starting OpenAgent UI on http://${HOST}:${PORT}"

uv run uvicorn app.main:app \
  --host "$HOST" \
  --port "$PORT" \
  $( [ "$RELOAD" = "true" ] && echo "--reload" ) \
  --reload-dir "$ROOT/app" \
  --reload-dir "$ROOT/openagent" &

UVICORN_PID=$!
wait "$UVICORN_PID"
