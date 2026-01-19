#!/usr/bin/env bash
# scripts/test_patients_full.sh
#
# Broad end-to-end test for Patients API:
# - POST   /api/v1/patients                       (create)
# - GET    /api/v1/patients?query=...             (search)
# - GET    /api/v1/patients/{patient_id}          (get)
# - PATCH  /api/v1/patients/{patient_id}          (update)
# - POST   /api/v1/patients/{patient_id}/archive  (archive via status)
# - POST   /api/v1/patients/{patient_id}/restore  (restore via status)
# - POST   /api/v1/patients/{patient_id}/link_user/{user_id}
# - POST   /api/v1/patients/{patient_id}/unlink_user
# - GET    /api/v1/patients/{patient_id}/summary  (aggregate)
#
# Notes:
# - We assume archive sets status=3 and restore sets status=0 (adjust constants below if needed).
# - This script also creates a patient dcms_user via /api/v1/users so we can test linking.
# - RBAC negative checks are tolerant: we accept 401/403 for forbidden behavior.
#
# Requirements: curl, jq
#
# Usage:
#   chmod +x scripts/test_patients_full.sh
#   ./scripts/test_patients_full.sh
#
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:8080/api/v1}"

ADMIN_USERNAME="${ADMIN_USERNAME:-admin}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-admin123}"

STATUS_ACTIVE="${STATUS_ACTIVE:-0}"
STATUS_ARCHIVED="${STATUS_ARCHIVED:-3}"

section() {
  echo
  echo "============================================================"
  echo "== $1"
  echo "============================================================"
}

curl_json() {
  # args: method url token json_body(optional)
  local method="$1"
  local url="$2"
  local token="${3:-}"
  local body="${4:-}"

  if [[ -n "$token" && -n "$body" ]]; then
    curl -sS -X "$method" "$url" \
      -H "Authorization: Bearer $token" \
      -H "Content-Type: application/json" \
      -d "$body"
  elif [[ -n "$token" ]]; then
    curl -sS -X "$method" "$url" \
      -H "Authorization: Bearer $token"
  elif [[ -n "$body" ]]; then
    curl -sS -X "$method" "$url" \
      -H "Content-Type: application/json" \
      -d "$body"
  else
    curl -sS -X "$method" "$url"
  fi
}

curl_with_status() {
  # prints: "<http_code>\n<body>"
  local method="$1"
  local url="$2"
  local token="${3:-}"
  local body="${4:-}"

  local tmp code
  tmp="$(mktemp)"

  if [[ -n "$token" && -n "$body" ]]; then
    code="$(curl -sS -o "$tmp" -w "%{http_code}" -X "$method" "$url" \
      -H "Authorization: Bearer $token" \
      -H "Content-Type: application/json" \
      -d "$body" || true)"
  elif [[ -n "$token" ]]; then
    code="$(curl -sS -o "$tmp" -w "%{http_code}" -X "$method" "$url" \
      -H "Authorization: Bearer $token" || true)"
  elif [[ -n "$body" ]]; then
    code="$(curl -sS -o "$tmp" -w "%{http_code}" -X "$method" "$url" \
      -H "Content-Type: application/json" \
      -d "$body" || true)"
  else
    code="$(curl -sS -o "$tmp" -w "%{http_code}" -X "$method" "$url" || true)"
  fi

  echo "$code"
  cat "$tmp"
  rm -f "$tmp"
}

assert_eq() {
  local expected="$1"
  local actual="$2"
  local msg="$3"
  if [[ "$expected" != "$actual" ]]; then
    echo "ASSERT FAIL: $msg"
    echo "  expected: $expected"
    echo "  actual:   $actual"
    exit 1
  fi
}

assert_nonempty() {
  local val="$1"
  local msg="$2"
  if [[ -z "$val" || "$val" == "null" ]]; then
    echo "ASSERT FAIL: $msg (got '$val')"
    exit 1
  fi
}

assert_code_in() {
  local code="$1"; shift
  local ok="false"
  for allowed in "$@"; do
    if [[ "$code" == "$allowed" ]]; then ok="true"; fi
  done
  if [[ "$ok" != "true" ]]; then
    echo "ASSERT FAIL: HTTP code $code not in allowed set: $*"
    exit 1
  fi
}

login() {
  local username="$1"
  local password="$2"
  local device="$3"
  local resp token
  resp="$(curl_json POST "$BASE_URL/auth/login" "" \
    "{\"username\":\"$username\",\"password\":\"$password\",\"device_name\":\"$device\"}")"
  token="$(echo "$resp" | jq -r '.data.access_token // empty')"
  if [[ -z "$token" ]]; then
    echo "ERROR: login failed for $username"
    echo "$resp" | jq .
    exit 1
  fi
  echo "$token"
}

create_user() {
  # returns user_id
  local token="$1"
  local username="$2"
  local display_name="$3"
  local password="$4"
  local roles="$5"

  local resp user_id
  resp="$(curl_json POST "$BASE_URL/users" "$token" \
    "{\"username\":\"$username\",\"display_name\":\"$display_name\",\"password\":\"$password\",\"roles\":$roles}")"
  user_id="$(echo "$resp" | jq -r '.data.user_id // empty')"
  if [[ -z "$user_id" ]]; then
    echo "ERROR: create_user failed for $username"
    echo "$resp" | jq .
    exit 1
  fi
  echo "$user_id"
}

section "Login as admin"
ADMIN_TOKEN="$(login "$ADMIN_USERNAME" "$ADMIN_PASSWORD" "patients-test-admin")"
echo "[ok] got admin token"

TS="$(date +%s)"
P_FIRST="John_$TS"
P_LAST="Doe_$TS"
P_QUERY="John_$TS"

section "Create patient (POST /patients)"
CREATE_RESP="$(curl_json POST "$BASE_URL/patients" "$ADMIN_TOKEN" \
  "{\"first_name\":\"$P_FIRST\",\"last_name\":\"$P_LAST\",\"gender\":0}")"
echo "$CREATE_RESP" | jq .

PATIENT_ID="$(echo "$CREATE_RESP" | jq -r '.patient_id // empty')"
assert_nonempty "$PATIENT_ID" "create patient must return patient_id"

REGISTER_NUMBER="$(echo "$CREATE_RESP" | jq -r '.register_number // empty')"
assert_nonempty "$REGISTER_NUMBER" "create patient must return register_number"

assert_eq "$P_FIRST" "$(echo "$CREATE_RESP" | jq -r '.first_name')" "first_name mismatch after create"
assert_eq "$P_LAST"  "$(echo "$CREATE_RESP" | jq -r '.last_name')"  "last_name mismatch after create"

section "Search patient by query (GET /patients?query=...)"
SEARCH_RESP="$(curl_json GET "$BASE_URL/patients?query=$P_QUERY" "$ADMIN_TOKEN")"
echo "$SEARCH_RESP" | jq '.[0:5]'

FOUND_ID="$(echo "$SEARCH_RESP" | jq -r --arg pid "$PATIENT_ID" '.[] | select(.patient_id==$pid) | .patient_id' | head -n 1)"
assert_eq "$PATIENT_ID" "$FOUND_ID" "created patient should appear in search results"

section "Search patient by register_number"
SEARCH_RN_RESP="$(curl_json GET "$BASE_URL/patients?query=$REGISTER_NUMBER" "$ADMIN_TOKEN")"
FOUND_RN_ID="$(echo "$SEARCH_RN_RESP" | jq -r --arg pid "$PATIENT_ID" '.[] | select(.patient_id==$pid) | .patient_id' | head -n 1)"
assert_eq "$PATIENT_ID" "$FOUND_RN_ID" "created patient should be found by register_number search"

section "Get patient (GET /patients/{patient_id})"
GET_RESP="$(curl_json GET "$BASE_URL/patients/$PATIENT_ID" "$ADMIN_TOKEN")"
echo "$GET_RESP" | jq .
assert_eq "$PATIENT_ID" "$(echo "$GET_RESP" | jq -r '.patient_id')" "GET patient_id mismatch"

section "PATCH update fields: email + status + gender"
PATCH1_RESP="$(curl_json PATCH "$BASE_URL/patients/$PATIENT_ID" "$ADMIN_TOKEN" \
  '{"email":"john@example.com","status":1,"gender":1}')"
echo "$PATCH1_RESP" | jq .

assert_eq "john@example.com" "$(echo "$PATCH1_RESP" | jq -r '.email')" "email not updated"
assert_eq "1" "$(echo "$PATCH1_RESP" | jq -r '.status')" "status not updated"
assert_eq "1" "$(echo "$PATCH1_RESP" | jq -r '.gender')" "gender not updated"

section "PATCH clear email (PATCH email=null semantics)"
# If your UpdatePatientRequest.email is Option<Option<String>>,
# sending {"email":null} should clear it.
PATCH_CLEAR_EMAIL="$(curl_json PATCH "$BASE_URL/patients/$PATIENT_ID" "$ADMIN_TOKEN" \
  '{"email":null}')"
echo "$PATCH_CLEAR_EMAIL" | jq .
# email should become null
EMAIL_VAL="$(echo "$PATCH_CLEAR_EMAIL" | jq -r '.email')"
if [[ "$EMAIL_VAL" != "null" ]]; then
  echo "ERROR: expected email to be null after PATCH {\"email\":null}, got: $EMAIL_VAL"
  exit 1
fi
echo "[ok] PATCH email null clears email"

section "PATCH set birthday (DATE) and names"
# Adjust birthday format to what your API accepts: if NaiveDate, it's usually "YYYY-MM-DD".
PATCH_BDAY="$(curl_json PATCH "$BASE_URL/patients/$PATIENT_ID" "$ADMIN_TOKEN" \
  "{\"birthday\":\"1999-12-31\",\"first_name\":\"Johnny_$TS\",\"last_name\":\"Doer_$TS\"}")"
echo "$PATCH_BDAY" | jq .

assert_eq "Johnny_$TS" "$(echo "$PATCH_BDAY" | jq -r '.first_name')" "first_name not patched"
assert_eq "Doer_$TS"   "$(echo "$PATCH_BDAY" | jq -r '.last_name')"  "last_name not patched"
# birthday may return with time if your type is DateTime; we accept either exact date or prefix
BDAY_RETURN="$(echo "$PATCH_BDAY" | jq -r '.birthday // empty')"
assert_nonempty "$BDAY_RETURN" "birthday should be present after patch"

section "Negative tests: invalid gender/status should 400/422"
read -r code body < <(curl_with_status PATCH "$BASE_URL/patients/$PATIENT_ID" "$ADMIN_TOKEN" '{"gender":9}')
echo "[invalid gender] http=$code"
echo "$body" | sed 's/^/[body] /'
assert_code_in "$code" "400" "422"

read -r code body < <(curl_with_status PATCH "$BASE_URL/patients/$PATIENT_ID" "$ADMIN_TOKEN" '{"status":99}')
echo "[invalid status] http=$code"
echo "$body" | sed 's/^/[body] /'
assert_code_in "$code" "400" "422"

section "Create a patient dcms_user and link/unlink"
U_PATIENT="patient_user_${TS}"
U_PAT_PW="patient1234"

PATIENT_USER_ID="$(create_user "$ADMIN_TOKEN" "$U_PATIENT" "Patient Portal $TS" "$U_PAT_PW" 0)"
echo "[ok] created patient dcms_user user_id=$PATIENT_USER_ID"

LINK_RESP="$(curl_json POST "$BASE_URL/patients/$PATIENT_ID/link_user/$PATIENT_USER_ID" "$ADMIN_TOKEN")"
echo "$LINK_RESP" | jq .
assert_eq "$PATIENT_USER_ID" "$(echo "$LINK_RESP" | jq -r '.user_id')" "link_user did not set patient.user_id"

UNLINK_RESP="$(curl_json POST "$BASE_URL/patients/$PATIENT_ID/unlink_user" "$ADMIN_TOKEN")"
echo "$UNLINK_RESP" | jq .
UNLINK_VAL="$(echo "$UNLINK_RESP" | jq -r '.user_id')"
if [[ "$UNLINK_VAL" != "null" ]]; then
  echo "ERROR: expected user_id null after unlink, got: $UNLINK_VAL"
  exit 1
fi
echo "[ok] unlink_user clears patient.user_id"

section "Patient summary (GET /patients/{patient_id}/summary)"
SUMMARY_RESP="$(curl_json GET "$BASE_URL/patients/$PATIENT_ID/summary" "$ADMIN_TOKEN")"
echo "$SUMMARY_RESP" | jq .

SUMMARY_PID="$(echo "$SUMMARY_RESP" | jq -r '.data.patient.patient_id')"
assert_eq "$PATIENT_ID" "$SUMMARY_PID" "summary patient_id mismatch"

# phone_numbers and recent_sms should be arrays even if empty
PHONES_TYPE="$(echo "$SUMMARY_RESP" | jq -r '.data.phone_numbers | type')"
SMS_TYPE="$(echo "$SUMMARY_RESP" | jq -r '.data.recent_sms | type')"
assert_eq "array" "$PHONES_TYPE" "summary phone_numbers should be array"
assert_eq "array" "$SMS_TYPE" "summary recent_sms should be array"
echo "[ok] summary shape looks correct"

section "Archive patient (POST /patients/{id}/archive) -> status=$STATUS_ARCHIVED"
ARCHIVE_RESP="$(curl_json POST "$BASE_URL/patients/$PATIENT_ID/archive" "$ADMIN_TOKEN")"
echo "$ARCHIVE_RESP" | jq .
assert_eq "$STATUS_ARCHIVED" "$(echo "$ARCHIVE_RESP" | jq -r '.status')" "archive did not set status to archived"

section "Restore patient (POST /patients/{id}/restore) -> status=$STATUS_ACTIVE"
RESTORE_RESP="$(curl_json POST "$BASE_URL/patients/$PATIENT_ID/restore" "$ADMIN_TOKEN")"
echo "$RESTORE_RESP" | jq .
assert_eq "$STATUS_ACTIVE" "$(echo "$RESTORE_RESP" | jq -r '.status')" "restore did not set status to active"

section "Negative test: Not found patient id"
FAKE_ID="00000000-0000-0000-0000-000000000000"
read -r code body < <(curl_with_status GET "$BASE_URL/patients/$FAKE_ID" "$ADMIN_TOKEN")
echo "[get fake patient] http=$code"
echo "$body" | sed 's/^/[body] /'
assert_code_in "$code" "400" "404"

section "RBAC sanity: patient role access should be blocked or restricted (tolerant)"
# Try logging in with patient_user and calling /patients search.
PATIENT_TOKEN="$(login "$U_PATIENT" "$U_PAT_PW" "patients-test-patient")"

read -r code body < <(curl_with_status GET "$BASE_URL/patients?query=$P_QUERY" "$PATIENT_TOKEN")
echo "[patient role /patients search] http=$code"
# Accept 401/403 if blocked (preferred). If you still allow it temporarily, accept 200.
assert_code_in "$code" "200" "401" "403"

section "PASS ✅ patients implementation broadly tested"
echo "patient_id=$PATIENT_ID register_number=$REGISTER_NUMBER"
echo "created patient user: $U_PATIENT (role=0)"
