#!/usr/bin/env bash
set -e

BASE="${BASE:-http://127.0.0.1:8080}"

TOKEN=$(curl -sS -X POST "$BASE/api/v1/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username":"reception1","password":"reception123","device_name":"test"}' \
  | jq -r '.data.access_token')

echo "[patient test] list patients"
curl -sS "$BASE/api/v1/patients" \
  -H "Authorization: Bearer $TOKEN" | jq

echo "[patient test] OK"
