#!/usr/bin/env bash
set -euo pipefail

# Load .env if present
if [ -f .env ]; then
  set -a
  source .env
  set +a
fi

: "${DATABASE_URL:?DATABASE_URL is required}"

echo "[migrate] DATABASE_URL=${DATABASE_URL}"
sqlx migrate run
echo "[migrate] done"

