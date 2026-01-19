#!/usr/bin/env bash
# scripts/test_sessions_revoke_one.sh
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:8080/api/v1}"
USERNAME="${USERNAME:-admin}"
PASSWORD="${PASSWORD:-admin123}"

echo "[test_sessions_revoke_one] login (session A)"
TOKEN_A="$(
  curl -sS -X POST "$BASE_URL/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$USERNAME\",\"password\":\"$PASSWORD\",\"device_name\":\"RevokeOne-A\"}" \
  | jq -r '.data.access_token'
)"

echo "[test_sessions_revoke_one] login (session B)"
TOKEN_B="$(
  curl -sS -X POST "$BASE_URL/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$USERNAME\",\"password\":\"$PASSWORD\",\"device_name\":\"RevokeOne-B\"}" \
  | jq -r '.data.access_token'
)"

if [[ -z "$TOKEN_A" || "$TOKEN_A" == "null" || -z "$TOKEN_B" || "$TOKEN_B" == "null" ]]; then
  echo "ERROR: login did not return access_token"
  exit 1
fi

echo "[test_sessions_revoke_one] list sessions (using A)"
RESP="$(curl -sS "$BASE_URL/auth/sessions" -H "Authorization: Bearer $TOKEN_A")"
CURRENT_ID="$(echo "$RESP" | jq -r '.data.current_session_token_id')"

# Pick a session id that is NOT the current one, if possible
TARGET_ID="$(echo "$RESP" | jq -r --arg cur "$CURRENT_ID" '.data.sessions[] | select(.session_token_id != $cur) | .session_token_id' | head -n 1)"

# If only one session is visible (rare), revoke current
if [[ -z "$TARGET_ID" || "$TARGET_ID" == "null" ]]; then
  TARGET_ID="$CURRENT_ID"
fi

echo "[test_sessions_revoke_one] revoke session: $TARGET_ID"
REVOKE_RESP="$(curl -sS -X POST "$BASE_URL/auth/sessions/$TARGET_ID/revoke" -H "Authorization: Bearer $TOKEN_A")"
echo "$REVOKE_RESP" | jq .

OK="$(echo "$REVOKE_RESP" | jq -r '.data.ok')"
REVOKED_ID="$(echo "$REVOKE_RESP" | jq -r '.data.revoked_session_token_id')"

if [[ "$OK" != "true" ]]; then
  echo "ERROR: expected ok=true"
  exit 1
fi

if [[ "$REVOKED_ID" != "$TARGET_ID" ]]; then
  echo "ERROR: revoked_session_token_id mismatch ($REVOKED_ID != $TARGET_ID)"
  exit 1
fi

echo "[test_sessions_revoke_one] verify revoked token can no longer call /me (if revoked was TOKEN_A current, it should fail)"
# If we revoked the current session id of TOKEN_A, /me should fail with session_expired.
# If we revoked a different session, TOKEN_A should still succeed.
ME_HTTP_CODE="$(curl -s -o /dev/null -w "%{http_code}" "$BASE_URL/auth/me" -H "Authorization: Bearer $TOKEN_A" || true)"

if [[ "$TARGET_ID" == "$CURRENT_ID" ]]; then
  if [[ "$ME_HTTP_CODE" == "200" ]]; then
    echo "ERROR: expected /auth/me to fail after revoking current session"
    exit 1
  fi
else
  if [[ "$ME_HTTP_CODE" != "200" ]]; then
    echo "ERROR: expected /auth/me to succeed (revoked non-current session), got HTTP $ME_HTTP_CODE"
    exit 1
  fi
fi

echo "[test_sessions_revoke_one] PASS"
