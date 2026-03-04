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

echo "Starting OpenAgent UI on http://${HOST}:${PORT}"

uv run uvicorn app.main:app \
  --host "$HOST" \
  --port "$PORT" \
  $( [ "$RELOAD" = "true" ] && echo "--reload" ) \
  --reload-dir "$ROOT/app" \
  --reload-dir "$ROOT/openagent"
