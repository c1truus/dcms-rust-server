#!/usr/bin/env bash
# scripts/test_auth_new_endpoints_full.sh
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:8080/api/v1}"
USERNAME="${USERNAME:-admin}"
PASSWORD="${PASSWORD:-admin123}"

# If you have a patient user in seeds, you can override:
PATIENT_USERNAME="${PATIENT_USERNAME:-}"
PATIENT_PASSWORD="${PATIENT_PASSWORD:-}"

need_cmd() { command -v "$1" >/dev/null 2>&1 || { echo "ERROR: missing dependency: $1"; exit 1; }; }
need_cmd curl
need_cmd jq

echo "============================================================"
echo "[auth_full] BASE_URL=$BASE_URL"
echo "============================================================"

# --- helpers -------------------------------------------------

curl_json() {
  # usage: curl_json METHOD URL [DATA] [AUTH_TOKEN]
  local method="$1"
  local url="$2"
  local data="${3:-}"
  local token="${4:-}"

  if [[ -n "$data" && -n "$token" ]]; then
    curl -sS -X "$method" "$url" \
      -H "Content-Type: application/json" \
      -H "Authorization: Bearer $token" \
      -d "$data"
  elif [[ -n "$data" ]]; then
    curl -sS -X "$method" "$url" \
      -H "Content-Type: application/json" \
      -d "$data"
  elif [[ -n "$token" ]]; then
    curl -sS -X "$method" "$url" \
      -H "Authorization: Bearer $token"
  else
    curl -sS -X "$method" "$url"
  fi
}

http_code() {
  # usage: http_code METHOD URL [DATA] [AUTH_TOKEN]
  local method="$1"
  local url="$2"
  local data="${3:-}"
  local token="${4:-}"

  if [[ -n "$data" && -n "$token" ]]; then
    curl -sS -o /dev/null -w "%{http_code}" -X "$method" "$url" \
      -H "Content-Type: application/json" \
      -H "Authorization: Bearer $token" \
      -d "$data"
  elif [[ -n "$data" ]]; then
    curl -sS -o /dev/null -w "%{http_code}" -X "$method" "$url" \
      -H "Content-Type: application/json" \
      -d "$data"
  elif [[ -n "$token" ]]; then
    curl -sS -o /dev/null -w "%{http_code}" -X "$method" "$url" \
      -H "Authorization: Bearer $token"
  else
    curl -sS -o /dev/null -w "%{http_code}" -X "$method" "$url"
  fi
}

expect_http() {
  local got="$1"
  local want="$2"
  local msg="$3"
  if [[ "$got" != "$want" ]]; then
    echo "ERROR: $msg (expected HTTP $want, got $got)"
    exit 1
  fi
}

expect_not_empty() {
  local val="$1"
  local msg="$2"
  if [[ -z "$val" || "$val" == "null" ]]; then
    echo "ERROR: $msg"
    exit 1
  fi
}

# --- Scenario A: 2 sessions + list/detail --------------------

echo
echo "[auth_full] A1) login #1 (create session 1)"
LOGIN1="$(curl_json POST "$BASE_URL/auth/login" "{\"username\":\"$USERNAME\",\"password\":\"$PASSWORD\",\"device_name\":\"AuthFull-Dev1\"}")"
echo "$LOGIN1" | jq .
TOKEN1="$(echo "$LOGIN1" | jq -r '.data.access_token')"
USER_ID="$(echo "$LOGIN1" | jq -r '.data.user.user_id')"
expect_not_empty "$TOKEN1" "login #1 did not return access_token"
expect_not_empty "$USER_ID" "login #1 did not return user_id"

echo
echo "[auth_full] A2) login #2 (create session 2)"
LOGIN2="$(curl_json POST "$BASE_URL/auth/login" "{\"username\":\"$USERNAME\",\"password\":\"$PASSWORD\",\"device_name\":\"AuthFull-Dev2\"}")"
TOKEN2="$(echo "$LOGIN2" | jq -r '.data.access_token')"
expect_not_empty "$TOKEN2" "login #2 did not return access_token"

echo
echo "[auth_full] A3) GET /auth/sessions (expect >= 2)"
SESS_LIST="$(curl_json GET "$BASE_URL/auth/sessions" "" "$TOKEN1")"
echo "$SESS_LIST" | jq .
CURRENT_ID="$(echo "$SESS_LIST" | jq -r '.data.current_session_token_id')"
COUNT="$(echo "$SESS_LIST" | jq -r '.data.sessions | length')"
expect_not_empty "$CURRENT_ID" "sessions list missing current_session_token_id"
if ! [[ "$COUNT" =~ ^[0-9]+$ ]]; then
  echo "ERROR: sessions length is not a number: $COUNT"
  exit 1
fi
if (( COUNT < 2 )); then
  echo "ERROR: expected at least 2 sessions, got $COUNT"
  exit 1
fi

echo
echo "[auth_full] A4) GET /auth/sessions/{id} (current session)"
DETAIL_CODE="$(http_code GET "$BASE_URL/auth/sessions/$CURRENT_ID" "" "$TOKEN1")"
expect_http "$DETAIL_CODE" "200" "GET /auth/sessions/{id} failed"
SESS_DETAIL="$(curl_json GET "$BASE_URL/auth/sessions/$CURRENT_ID" "" "$TOKEN1")"
echo "$SESS_DETAIL" | jq .
DETAIL_ID="$(echo "$SESS_DETAIL" | jq -r '.data.session_token_id // .data.id // .data.session.session_token_id')"
# tolerate different response shapes; at least ensure response contains the ID somewhere
if ! echo "$SESS_DETAIL" | jq -e --arg id "$CURRENT_ID" '..|strings|select(.==$id)' >/dev/null; then
  echo "ERROR: session detail response does not contain expected session_token_id=$CURRENT_ID"
  exit 1
fi

# --- Scenario B: Extend session -------------------------------

echo
echo "[auth_full] B1) Read expires_at before extend"
EXPIRES_BEFORE="$(echo "$SESS_DETAIL" | jq -r '..|.expires_at? // empty' | head -n1)"
# If not present in detail response, fetch from list as fallback
if [[ -z "$EXPIRES_BEFORE" ]]; then
  EXPIRES_BEFORE="$(echo "$SESS_LIST" | jq -r --arg id "$CURRENT_ID" '.data.sessions[] | select(.session_token_id==$id or .id==$id) | .expires_at' | head -n1)"
fi
expect_not_empty "$EXPIRES_BEFORE" "expires_at missing (detail/list)"

echo "[auth_full] B2) POST /auth/sessions/{id}/extend (extend_hours=24)"
EXT_REQ='{"extend_hours":24}'
EXT_CODE="$(http_code POST "$BASE_URL/auth/sessions/$CURRENT_ID/extend" "$EXT_REQ" "$TOKEN1")"
expect_http "$EXT_CODE" "200" "extend endpoint failed"
EXT_RESP="$(curl_json POST "$BASE_URL/auth/sessions/$CURRENT_ID/extend" "$EXT_REQ" "$TOKEN1")"
echo "$EXT_RESP" | jq .

echo "[auth_full] B3) Fetch expires_at after extend"
SESS_DETAIL2="$(curl_json GET "$BASE_URL/auth/sessions/$CURRENT_ID" "" "$TOKEN1")"
EXPIRES_AFTER="$(echo "$SESS_DETAIL2" | jq -r '..|.expires_at? // empty' | head -n1)"
expect_not_empty "$EXPIRES_AFTER" "expires_at missing after extend"

if [[ "$EXPIRES_AFTER" == "$EXPIRES_BEFORE" ]]; then
  echo "ERROR: expires_at did not change after extend (before=$EXPIRES_BEFORE after=$EXPIRES_AFTER)"
  exit 1
fi
echo "[auth_full] extend OK: expires_at changed"
echo "  before=$EXPIRES_BEFORE"
echo "  after =$EXPIRES_AFTER"

# --- Scenario C: Refresh token rotation -----------------------

echo
echo "[auth_full] C1) POST /auth/refresh (expect new access_token)"
REF_CODE="$(http_code POST "$BASE_URL/auth/refresh" "" "$TOKEN1")"
expect_http "$REF_CODE" "200" "refresh endpoint failed"
REF_RESP="$(curl_json POST "$BASE_URL/auth/refresh" "" "$TOKEN1")"
echo "$REF_RESP" | jq .
TOKEN1_NEW="$(echo "$REF_RESP" | jq -r '.data.access_token')"
expect_not_empty "$TOKEN1_NEW" "refresh did not return new access_token"
if [[ "$TOKEN1_NEW" == "$TOKEN1" ]]; then
  echo "ERROR: refresh returned the same token (expected rotation)"
  exit 1
fi

echo
echo "[auth_full] C2) Old token should be invalid now (GET /auth/me with old TOKEN1)"
ME_OLD_CODE="$(http_code GET "$BASE_URL/auth/me" "" "$TOKEN1")"
if [[ "$ME_OLD_CODE" == "200" ]]; then
  echo "ERROR: old token still works after refresh (expected 401/403). This is a nasty security bug."
  exit 1
fi
echo "[auth_full] old token invalidated OK (http=$ME_OLD_CODE)"

echo
echo "[auth_full] C3) New token should work (GET /auth/me with TOKEN1_NEW)"
ME_NEW_CODE="$(http_code GET "$BASE_URL/auth/me" "" "$TOKEN1_NEW")"
expect_http "$ME_NEW_CODE" "200" "new token from refresh does not work"
ME_NEW="$(curl_json GET "$BASE_URL/auth/me" "" "$TOKEN1_NEW")"
echo "$ME_NEW" | jq .

# Replace TOKEN1 with rotated token for subsequent steps
TOKEN1="$TOKEN1_NEW"

# --- Scenario D: logout_all_except_current --------------------

echo
echo "[auth_full] D1) POST /auth/logout_all_except_current"
LOGOUT_OTHERS_CODE="$(http_code POST "$BASE_URL/auth/logout_all_except_current" "" "$TOKEN1")"
expect_http "$LOGOUT_OTHERS_CODE" "200" "logout_all_except_current failed"
LOGOUT_OTHERS="$(curl_json POST "$BASE_URL/auth/logout_all_except_current" "" "$TOKEN1")"
echo "$LOGOUT_OTHERS" | jq .

echo
echo "[auth_full] D2) TOKEN2 should now be invalid"
ME2_CODE="$(http_code GET "$BASE_URL/auth/me" "" "$TOKEN2")"
if [[ "$ME2_CODE" == "200" ]]; then
  echo "ERROR: TOKEN2 still works after logout_all_except_current. Bug: revoke logic is not working."
  exit 1
fi
echo "[auth_full] TOKEN2 invalidated OK (http=$ME2_CODE)"

echo
echo "[auth_full] D3) TOKEN1 should still work"
ME1_CODE="$(http_code GET "$BASE_URL/auth/me" "" "$TOKEN1")"
expect_http "$ME1_CODE" "200" "TOKEN1 stopped working after logout_all_except_current (should keep current)"

# --- Scenario E: session ownership checks (negative test) ------

echo
echo "[auth_full] E1) login as another session (TOKEN3) then try to read TOKEN1's session detail with TOKEN3"
LOGIN3="$(curl_json POST "$BASE_URL/auth/login" "{\"username\":\"$USERNAME\",\"password\":\"$PASSWORD\",\"device_name\":\"AuthFull-Dev3\"}")"
TOKEN3="$(echo "$LOGIN3" | jq -r '.data.access_token')"
expect_not_empty "$TOKEN3" "login #3 did not return access_token"

# pick TOKEN1's current session id again
SESS_LIST_2="$(curl_json GET "$BASE_URL/auth/sessions" "" "$TOKEN1")"
CURRENT_ID_2="$(echo "$SESS_LIST_2" | jq -r '.data.current_session_token_id')"
expect_not_empty "$CURRENT_ID_2" "sessions list missing current_session_token_id (second time)"

# if your design allows same user to access own sessions, this should be 200 even from TOKEN3.
# To create a real "other user" negative test we'd need a non-admin user. We'll do that in impersonation section.

# --- Scenario F: impersonation (admin) -------------------------

echo
echo "[auth_full] F1) Find a target user to impersonate via GET /users"
USERS_CODE="$(http_code GET "$BASE_URL/users" "" "$TOKEN1")"
if [[ "$USERS_CODE" != "200" ]]; then
  echo "WARN: cannot GET /users (http=$USERS_CODE). Skipping impersonation tests."
else
  USERS="$(curl_json GET "$BASE_URL/users" "" "$TOKEN1")"
  echo "$USERS" | jq '.data | (if type=="array" then length else .users|length end)'

  # try both possible shapes: {data:[...]} or {data:{users:[...]}}
  TARGET_USER_ID="$(
    echo "$USERS" | jq -r --arg me "$USER_ID" '
      ( .data.users // .data // [] )
      | map(select((.user_id // .id) != $me))
      | map(.user_id // .id)
      | .[0] // empty
    '
  )"

  if [[ -z "$TARGET_USER_ID" || "$TARGET_USER_ID" == "null" ]]; then
    echo "WARN: no other user found to impersonate. Skipping impersonation."
  else
    echo "[auth_full] F2) POST /auth/impersonate/$TARGET_USER_ID"
    IMP_CODE="$(http_code POST "$BASE_URL/auth/impersonate/$TARGET_USER_ID" "" "$TOKEN1")"
    if [[ "$IMP_CODE" != "200" ]]; then
      echo "ERROR: impersonate failed (http=$IMP_CODE). If you haven't applied 008_session_impersonation.sql, it will fail."
      exit 1
    fi
    IMP="$(curl_json POST "$BASE_URL/auth/impersonate/$TARGET_USER_ID" "" "$TOKEN1")"
    echo "$IMP" | jq .
    IMP_TOKEN="$(echo "$IMP" | jq -r '.data.access_token')"
    expect_not_empty "$IMP_TOKEN" "impersonate did not return access_token"

    echo "[auth_full] F3) GET /auth/me with impersonation token (should act as target)"
    ME_IMP_CODE="$(http_code GET "$BASE_URL/auth/me" "" "$IMP_TOKEN")"
    expect_http "$ME_IMP_CODE" "200" "impersonation token cannot call /auth/me"
    ME_IMP="$(curl_json GET "$BASE_URL/auth/me" "" "$IMP_TOKEN")"
    echo "$ME_IMP" | jq .

    # verify target user id appears in response somewhere
    if ! echo "$ME_IMP" | jq -e --arg id "$TARGET_USER_ID" '..|strings|select(.==$id)' >/dev/null; then
      echo "ERROR: /auth/me response does not contain target user_id=$TARGET_USER_ID under impersonation"
      exit 1
    fi
    echo "[auth_full] impersonation OK: acting as target user_id=$TARGET_USER_ID"
  fi
fi

# --- Scenario G: patient login (optional) ----------------------

echo
echo "[auth_full] G1) patient login check (optional)"
if [[ -n "$PATIENT_USERNAME" && -n "$PATIENT_PASSWORD" ]]; then
  PLOGIN="$(curl_json POST "$BASE_URL/auth/patient/login" "{\"username\":\"$PATIENT_USERNAME\",\"password\":\"$PATIENT_PASSWORD\",\"device_name\":\"AuthFull-Patient\"}")"
  echo "$PLOGIN" | jq .
  PTOKEN="$(echo "$PLOGIN" | jq -r '.data.access_token')"
  if [[ -z "$PTOKEN" || "$PTOKEN" == "null" ]]; then
    echo "ERROR: patient login failed (no token) for PATIENT_USERNAME=$PATIENT_USERNAME"
    exit 1
  fi
  PME_CODE="$(http_code GET "$BASE_URL/auth/me" "" "$PTOKEN")"
  expect_http "$PME_CODE" "200" "patient token cannot call /auth/me"
  echo "[auth_full] patient login OK"
else
  echo "SKIP: set PATIENT_USERNAME and PATIENT_PASSWORD to test patient login."
fi

echo
echo "============================================================"
echo "[auth_full] PASS ✅  All selected auth/session tests succeeded"
echo "============================================================"
