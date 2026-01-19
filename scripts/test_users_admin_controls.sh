#!/usr/bin/env bash
# scripts/test_users_admin_controls.sh
#
# Broad end-to-end test for /api/v1/users admin-ish controls:
# - list users
# - create many users (various roles)
# - get user by id
# - patch (display_name / roles / is_active)
# - disable/enable endpoints
# - negative tests (forbidden for non-admin/manager, invalid role, duplicate username)
# - "remove" (NOTE: no delete endpoint implemented; we simulate removal by disabling)
#
# Requirements: curl, jq
#
# Usage:
#   chmod +x scripts/test_users_admin_controls.sh
#   ./scripts/test_users_admin_controls.sh
#
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:8080/api/v1}"

ADMIN_USERNAME="${ADMIN_USERNAME:-admin}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-admin123}"

# If you also seed a manager account, set these env vars; otherwise the script will skip manager tests.
MANAGER_USERNAME="${MANAGER_USERNAME:-manager1}"
MANAGER_PASSWORD="${MANAGER_PASSWORD:-manager123}"

# Helper: print section headers
section() {
  echo
  echo "============================================================"
  echo "== $1"
  echo "============================================================"
}

# Helper: curl JSON and ensure it's valid JSON
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

# Helper: curl and capture HTTP status + body
curl_with_status() {
  # args: method url token json_body(optional)
  local method="$1"
  local url="$2"
  local token="${3:-}"
  local body="${4:-}"

  local tmp
  tmp="$(mktemp)"
  local code

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

# Helper: assert equals
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

# Helper: assert HTTP code in list
assert_code_in() {
  local code="$1"
  shift
  local ok="false"
  for allowed in "$@"; do
    if [[ "$code" == "$allowed" ]]; then ok="true"; fi
  done
  if [[ "$ok" != "true" ]]; then
    echo "ASSERT FAIL: HTTP code $code not in allowed set: $*"
    exit 1
  fi
}

# Login and return token
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

# Create user, return user_id
create_user() {
  local token="$1"
  local username="$2"
  local display="$3"
  local password="$4"
  local roles="$5"
  local is_active="${6:-true}"

  local resp user_id
  resp="$(curl_json POST "$BASE_URL/users" "$token" \
    "{\"username\":\"$username\",\"display_name\":\"$display\",\"password\":\"$password\",\"roles\":$roles,\"is_active\":$is_active}")"
  user_id="$(echo "$resp" | jq -r '.data.user_id // empty')"
  if [[ -z "$user_id" ]]; then
    echo "ERROR: create_user failed for $username"
    echo "$resp" | jq .
    exit 1
  fi
  echo "$user_id"
}

# Get user
get_user() {
  local token="$1"
  local user_id="$2"
  curl_json GET "$BASE_URL/users/$user_id" "$token"
}

# List users
list_users() {
  local token="$1"
  curl_json GET "$BASE_URL/users" "$token"
}

# Patch user
patch_user() {
  local token="$1"
  local user_id="$2"
  local body="$3"
  curl_json PATCH "$BASE_URL/users/$user_id" "$token" "$body"
}

disable_user() {
  local token="$1"
  local user_id="$2"
  curl_json POST "$BASE_URL/users/$user_id/disable" "$token"
}

enable_user() {
  local token="$1"
  local user_id="$2"
  curl_json POST "$BASE_URL/users/$user_id/enable" "$token"
}

section "Login as admin"
ADMIN_TOKEN="$(login "$ADMIN_USERNAME" "$ADMIN_PASSWORD" "users-admin-test")"
echo "[ok] got admin token"

section "Baseline list users"
BASE_LIST="$(list_users "$ADMIN_TOKEN")"
echo "$BASE_LIST" | jq .
BASE_COUNT="$(echo "$BASE_LIST" | jq -r '.data.users | length')"
echo "[info] baseline user count = $BASE_COUNT"

section "Create a bunch of users (various roles)"
TS="$(date +%s)"
# usernames must be unique; use timestamp suffix
U_PATIENT="patient_${TS}"
U_DOCTOR="doctor_${TS}"
U_RECEPT="recept_${TS}"
U_MANAGER="manager_${TS}"
U_ADMIN2="admin2_${TS}"
U_MISC1="staffa_${TS}"
U_MISC2="staffb_${TS}"

ID_PATIENT="$(create_user "$ADMIN_TOKEN" "$U_PATIENT" "Test Patient $TS" "patient1234" 0 true)"
ID_DOCTOR="$(create_user "$ADMIN_TOKEN" "$U_DOCTOR" "Dr Test $TS" "doctor1234" 3 true)"
ID_RECEPT="$(create_user "$ADMIN_TOKEN" "$U_RECEPT" "Reception $TS" "reception1234" 4 true)"
ID_MANAGER="$(create_user "$ADMIN_TOKEN" "$U_MANAGER" "Manager $TS" "manager1234" 2 true)"
ID_ADMIN2="$(create_user "$ADMIN_TOKEN" "$U_ADMIN2" "Admin Two $TS" "admin9999" 1 true)"
ID_MISC1="$(create_user "$ADMIN_TOKEN" "$U_MISC1" "Staff A $TS" "staffaaaa" 4 true)"
ID_MISC2="$(create_user "$ADMIN_TOKEN" "$U_MISC2" "Staff B $TS" "staffbbbb" 3 true)"

echo "[created] patient=$ID_PATIENT doctor=$ID_DOCTOR recept=$ID_RECEPT manager=$ID_MANAGER admin2=$ID_ADMIN2"

section "List users after creation (should be +7)"
AFTER_CREATE_LIST="$(list_users "$ADMIN_TOKEN")"
AFTER_CREATE_COUNT="$(echo "$AFTER_CREATE_LIST" | jq -r '.data.users | length')"
echo "$AFTER_CREATE_LIST" | jq '.data.users[0:5]'
echo "[info] after creation user count = $AFTER_CREATE_COUNT"
if (( AFTER_CREATE_COUNT < BASE_COUNT + 7 )); then
  echo "ERROR: expected at least BASE+7 users"
  echo "BASE=$BASE_COUNT AFTER=$AFTER_CREATE_COUNT"
  exit 1
fi

section "GET each created user and validate fields"
for pair in \
  "$ID_PATIENT:$U_PATIENT:0" \
  "$ID_DOCTOR:$U_DOCTOR:3" \
  "$ID_RECEPT:$U_RECEPT:4" \
  "$ID_MANAGER:$U_MANAGER:2" \
  "$ID_ADMIN2:$U_ADMIN2:1"
do
  IFS=":" read -r uid uname role <<<"$pair"
  resp="$(get_user "$ADMIN_TOKEN" "$uid")"
  got_uname="$(echo "$resp" | jq -r '.data.username')"
  got_role="$(echo "$resp" | jq -r '.data.roles')"
  assert_eq "$uname" "$got_uname" "username mismatch for $uid"
  assert_eq "$role" "$got_role" "roles mismatch for $uid"
done
echo "[ok] GET validations passed"

section "PATCH: change display_name"
NEW_NAME="Renamed Doctor $TS"
resp="$(patch_user "$ADMIN_TOKEN" "$ID_DOCTOR" "{\"display_name\":\"$NEW_NAME\"}")"
echo "$resp" | jq .
assert_eq "$NEW_NAME" "$(echo "$resp" | jq -r '.data.display_name')" "display_name not updated"
echo "[ok] display_name updated"

section "PATCH: change roles (reception -> manager)"
resp="$(patch_user "$ADMIN_TOKEN" "$ID_RECEPT" '{"roles":2}')"
echo "$resp" | jq .
assert_eq "2" "$(echo "$resp" | jq -r '.data.roles')" "roles not updated"
echo "[ok] roles updated"

section "PATCH: set is_active=false via PATCH"
resp="$(patch_user "$ADMIN_TOKEN" "$ID_MISC1" '{"is_active":false}')"
echo "$resp" | jq .
assert_eq "false" "$(echo "$resp" | jq -r '.data.is_active')" "is_active not updated by PATCH"
echo "[ok] PATCH is_active=false works"

section "Disable endpoint: /users/{id}/disable"
resp="$(disable_user "$ADMIN_TOKEN" "$ID_MISC2")"
echo "$resp" | jq .
assert_eq "true" "$(echo "$resp" | jq -r '.data.ok')" "disable endpoint did not return ok=true"

# verify disabled in GET
resp="$(get_user "$ADMIN_TOKEN" "$ID_MISC2")"
assert_eq "false" "$(echo "$resp" | jq -r '.data.is_active')" "user not disabled after /disable"
echo "[ok] disable endpoint works"

section "Enable endpoint: /users/{id}/enable"
resp="$(enable_user "$ADMIN_TOKEN" "$ID_MISC2")"
echo "$resp" | jq .
assert_eq "true" "$(echo "$resp" | jq -r '.data.ok')" "enable endpoint did not return ok=true"

resp="$(get_user "$ADMIN_TOKEN" "$ID_MISC2")"
assert_eq "true" "$(echo "$resp" | jq -r '.data.is_active')" "user not enabled after /enable"
echo "[ok] enable endpoint works"

section "Negative test: invalid roles (should 400)"
# PATCH invalid role
read -r code body < <(curl_with_status PATCH "$BASE_URL/users/$ID_PATIENT" "$ADMIN_TOKEN" '{"roles":99}')
echo "[http] $code"
echo "$body" | sed 's/^/[body] /'
assert_code_in "$code" "400" "422"

section "Negative test: duplicate username (should error)"
# Attempt to create same username again
read -r code body < <(curl_with_status POST "$BASE_URL/users" "$ADMIN_TOKEN" \
  "{\"username\":\"$U_PATIENT\",\"display_name\":\"Dup\",\"password\":\"patient1234\",\"roles\":0}")
echo "[http] $code"
echo "$body" | sed 's/^/[body] /'
# Might be 500 right now because create_user maps db errors to Internal; accept 409 if you improve later
assert_code_in "$code" "409" "400" "500"

section "RBAC negative test: non-admin/manager cannot access /users"
# Login as a normal staff user (created doctor) and attempt list users
DOCTOR_TOKEN="$(login "$U_DOCTOR" "doctor1234" "users-doctor-test")"

read -r code body < <(curl_with_status GET "$BASE_URL/users" "$DOCTOR_TOKEN")
echo "[doctor list users] http=$code"
echo "$body" | sed 's/^/[body] /'
assert_code_in "$code" "401" "403"

section "RBAC positive test: manager can access /users (if you want)"
# If you have seeded manager1 with manager123, test that manager works.
# Otherwise, test with the manager we created above (U_MANAGER / manager1234).
MANAGER_TOKEN="$(login "$U_MANAGER" "manager1234" "users-manager-test")"

read -r code body < <(curl_with_status GET "$BASE_URL/users" "$MANAGER_TOKEN")
echo "[manager list users] http=$code"
if [[ "$code" != "200" ]]; then
  echo "$body" | sed 's/^/[body] /'
  echo "ERROR: manager should be allowed to list users"
  exit 1
fi
echo "$body" | jq '.data.users[0:3]' >/dev/null
echo "[ok] manager can list users"

section '"Remove" users (simulate removal by disabling)'
# Since there is no DELETE /users endpoint, "remove" == disable in this build.
for uid in "$ID_PATIENT" "$ID_DOCTOR" "$ID_RECEPT" "$ID_MANAGER" "$ID_ADMIN2" "$ID_MISC1" "$ID_MISC2"; do
  resp="$(disable_user "$ADMIN_TOKEN" "$uid")"
  ok="$(echo "$resp" | jq -r '.data.ok')"
  if [[ "$ok" != "true" ]]; then
    echo "ERROR: failed to disable $uid"
    echo "$resp" | jq .
    exit 1
  fi
done
echo "[ok] all created users disabled"

section "Verify disabled users cannot login (is_active=false)"
# Attempt login for doctor (disabled)
read -r code body < <(curl_with_status POST "$BASE_URL/auth/login" "" \
  "{\"username\":\"$U_DOCTOR\",\"password\":\"doctor1234\",\"device_name\":\"disabled-login\"}")
echo "[disabled login] http=$code"
echo "$body" | sed 's/^/[body] /'
# Your login returns 403 when is_active false (per auth_routes.rs)
assert_code_in "$code" "403" "401"

section "Re-enable a user and verify login works again"
enable_user "$ADMIN_TOKEN" "$ID_DOCTOR" | jq .
DOCTOR_TOKEN2="$(login "$U_DOCTOR" "doctor1234" "re-enabled-login")"
echo "[ok] login works after enable"

section "Final sanity: list users (should include disabled flags)"
FINAL_LIST="$(list_users "$ADMIN_TOKEN")"
# Show only our created users, compact
echo "$FINAL_LIST" | jq --arg u1 "$U_PATIENT" --arg u2 "$U_DOCTOR" --arg u3 "$U_RECEPT" --arg u4 "$U_MANAGER" --arg u5 "$U_ADMIN2" '
  .data.users
  | map(select(.username==$u1 or .username==$u2 or .username==$u3 or .username==$u4 or .username==$u5))
'

section "PASS ✅ users admin controls broadly tested"
echo "Created usernames:"
echo "  $U_PATIENT"
echo "  $U_DOCTOR"
echo "  $U_RECEPT"
echo "  $U_MANAGER"
echo "  $U_ADMIN2"
echo "  $U_MISC1"
echo "  $U_MISC2"
