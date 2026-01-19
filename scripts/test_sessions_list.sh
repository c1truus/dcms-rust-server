#!/usr/bin/env bash
# scripts/test_sessions_list.sh
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:8080/api/v1}"
USERNAME="${USERNAME:-admin}"
PASSWORD="${PASSWORD:-admin123}"

echo "[test_sessions_list] login (create session 1)"
TOKEN1="$(
  curl -sS -X POST "$BASE_URL/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$USERNAME\",\"password\":\"$PASSWORD\",\"device_name\":\"SessList-Dev1\"}" \
  | jq -r '.data.access_token'
)"

echo "[test_sessions_list] login (create session 2)"
TOKEN2="$(
  curl -sS -X POST "$BASE_URL/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$USERNAME\",\"password\":\"$PASSWORD\",\"device_name\":\"SessList-Dev2\"}" \
  | jq -r '.data.access_token'
)"

if [[ -z "$TOKEN1" || "$TOKEN1" == "null" || -z "$TOKEN2" || "$TOKEN2" == "null" ]]; then
  echo "ERROR: login did not return access_token"
  exit 1
fi

echo "[test_sessions_list] GET /auth/sessions"
RESP="$(curl -sS "$BASE_URL/auth/sessions" -H "Authorization: Bearer $TOKEN1")"
echo "$RESP" | jq .

CURRENT_ID="$(echo "$RESP" | jq -r '.data.current_session_token_id')"
COUNT="$(echo "$RESP" | jq -r '.data.sessions | length')"

if [[ -z "$CURRENT_ID" || "$CURRENT_ID" == "null" ]]; then
  echo "ERROR: current_session_token_id missing"
  exit 1
fi

if ! [[ "$COUNT" =~ ^[0-9]+$ ]]; then
  echo "ERROR: sessions length is not a number: $COUNT"
  exit 1
fi

if (( COUNT < 1 )); then
  echo "ERROR: expected at least 1 active session, got $COUNT"
  exit 1
fi

echo "[test_sessions_list] PASS (active_sessions=$COUNT, current_session_token_id=$CURRENT_ID)"
