#!/usr/bin/env bash
# scripts/test_sessions_revoke_all.sh
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:8080/api/v1}"
USERNAME="${USERNAME:-admin}"
PASSWORD="${PASSWORD:-admin123}"

echo "[test_sessions_revoke_all] login (session 1 - will be current)"
TOKEN1="$(
  curl -sS -X POST "$BASE_URL/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$USERNAME\",\"password\":\"$PASSWORD\",\"device_name\":\"RevokeAll-1\"}" \
  | jq -r '.data.access_token'
)"

echo "[test_sessions_revoke_all] login (session 2)"
TOKEN2="$(
  curl -sS -X POST "$BASE_URL/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$USERNAME\",\"password\":\"$PASSWORD\",\"device_name\":\"RevokeAll-2\"}" \
  | jq -r '.data.access_token'
)"

echo "[test_sessions_revoke_all] login (session 3)"
TOKEN3="$(
  curl -sS -X POST "$BASE_URL/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$USERNAME\",\"password\":\"$PASSWORD\",\"device_name\":\"RevokeAll-3\"}" \
  | jq -r '.data.access_token'
)"

if [[ -z "$TOKEN1" || "$TOKEN1" == "null" || -z "$TOKEN2" || "$TOKEN2" == "null" || -z "$TOKEN3" || "$TOKEN3" == "null" ]]; then
  echo "ERROR: login did not return access_token"
  exit 1
fi

echo "[test_sessions_revoke_all] list sessions before"
BEFORE="$(curl -sS "$BASE_URL/auth/sessions" -H "Authorization: Bearer $TOKEN1")"
BEFORE_COUNT="$(echo "$BEFORE" | jq -r '.data.sessions | length')"
CURRENT_ID="$(echo "$BEFORE" | jq -r '.data.current_session_token_id')"
echo "$BEFORE" | jq .

if ! [[ "$BEFORE_COUNT" =~ ^[0-9]+$ ]]; then
  echo "ERROR: sessions length is not a number: $BEFORE_COUNT"
  exit 1
fi

if (( BEFORE_COUNT < 1 )); then
  echo "ERROR: expected at least 1 session before revoke_all"
  exit 1
fi

echo "[test_sessions_revoke_all] POST /auth/sessions/revoke_all"
REVOKE_ALL_RESP="$(curl -sS -X POST "$BASE_URL/auth/sessions/revoke_all" -H "Authorization: Bearer $TOKEN1")"
echo "$REVOKE_ALL_RESP" | jq .
OK="$(echo "$REVOKE_ALL_RESP" | jq -r '.data.ok')"
if [[ "$OK" != "true" ]]; then
  echo "ERROR: expected ok=true"
  exit 1
fi

echo "[test_sessions_revoke_all] list sessions after"
AFTER="$(curl -sS "$BASE_URL/auth/sessions" -H "Authorization: Bearer $TOKEN1")"
AFTER_COUNT="$(echo "$AFTER" | jq -r '.data.sessions | length')"
echo "$AFTER" | jq .

if [[ "$AFTER_COUNT" != "1" ]]; then
  echo "ERROR: expected exactly 1 active session after revoke_all, got $AFTER_COUNT"
  exit 1
fi

AFTER_ONLY_ID="$(echo "$AFTER" | jq -r '.data.sessions[0].session_token_id')"
if [[ "$AFTER_ONLY_ID" != "$CURRENT_ID" ]]; then
  echo "ERROR: expected remaining session to be current ($CURRENT_ID), got $AFTER_ONLY_ID"
  exit 1
fi

echo "[test_sessions_revoke_all] verify TOKEN2 is revoked (calling /me should fail)"
ME2_HTTP_CODE="$(curl -s -o /dev/null -w "%{http_code}" "$BASE_URL/auth/me" -H "Authorization: Bearer $TOKEN2" || true)"
if [[ "$ME2_HTTP_CODE" == "200" ]]; then
  echo "ERROR: expected /auth/me with TOKEN2 to fail after revoke_all"
  exit 1
fi

echo "[test_sessions_revoke_all] verify TOKEN1 still works"
ME1_HTTP_CODE="$(curl -s -o /dev/null -w "%{http_code}" "$BASE_URL/auth/me" -H "Authorization: Bearer $TOKEN1" || true)"
if [[ "$ME1_HTTP_CODE" != "200" ]]; then
  echo "ERROR: expected /auth/me with TOKEN1 to succeed, got HTTP $ME1_HTTP_CODE"
  exit 1
fi

echo "[test_sessions_revoke_all] PASS"
