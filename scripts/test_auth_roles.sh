#!/usr/bin/env bash
set -e

BASE="${BASE:-http://127.0.0.1:8080}"

for USER in admin manager1 doctor1 reception1; do
  echo "[auth test] login as $USER"
  curl -sS -X POST "$BASE/api/v1/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$USER\",\"password\":\"${USER}123\",\"device_name\":\"test\"}" \
    | jq '.data.user'
done

echo "[auth test] OK"
