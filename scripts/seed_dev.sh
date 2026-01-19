#!/usr/bin/env bash
set -euo pipefail

if [ -f .env ]; then
  set -a
  source .env
  set +a
fi

: "${ENV:?ENV is required (dev|prod)}"
: "${DATABASE_URL:?DATABASE_URL is required}"

if [ "${ENV}" != "dev" ]; then
  echo "[seed] REFUSING: ENV=${ENV}. Seed is DEV-only."
  exit 1
fi

echo "[seed] ENV=dev OK"
psql -v ON_ERROR_STOP=1 "$DATABASE_URL" -f scripts/seed_dev.sql
psql -v ON_ERROR_STOP=1 "$DATABASE_URL" -f scripts/seed_demo_data.sql
echo "[seed] done"
