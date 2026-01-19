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
  echo "[reset] REFUSING: ENV=${ENV}. Reset is DEV-only."
  exit 1
fi

echo "[reset] Resetting schema on DATABASE_URL=${DATABASE_URL}"

# Drop and recreate public schema
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -c "DROP SCHEMA IF EXISTS public CASCADE;"
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -c "CREATE SCHEMA public;"

# Run migrations
sqlx migrate run

# Seed dev data
psql "$DATABASE_URL" -f scripts/seed_dev.sql

echo "[reset] done ✅"
