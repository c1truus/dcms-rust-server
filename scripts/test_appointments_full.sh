#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:8080/api/v1}"
DATABASE_URL="${DATABASE_URL:-}"

ADMIN_USER="${ADMIN_USER:-admin}"
ADMIN_PASS="${ADMIN_PASS:-admin123}"

need() { command -v "$1" >/dev/null 2>&1 || { echo "ERROR: missing dependency: $1"; exit 1; }; }
need curl
need jq
need psql

if [[ -z "${DATABASE_URL}" ]]; then
  echo "ERROR: DATABASE_URL is required for this test script (to fetch seeded UUIDs)."
  echo "Example:"
  echo "  export DATABASE_URL='postgres://dcms:314159@127.0.0.1:5432/dcms_dev'"
  exit 1
fi

echo "============================================================"
echo "[appts_full] BASE_URL=${BASE_URL}"
echo "============================================================"

# -----------------------------
# Helpers
# -----------------------------
api_post() {
  local path="$1"
  local body="$2"
  curl -sS -X POST "${BASE_URL}${path}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${TOKEN}" \
    -d "${body}"
}

api_post_with_status() {
  local path="$1"
  local body="$2"
  curl -sS -X POST "${BASE_URL}${path}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${TOKEN}" \
    -d "${body}" \
    -w "\n__HTTP_STATUS__:%{http_code}\n"
}

api_put_with_status() {
  local path="$1"
  local body="$2"
  curl -sS -X PUT "${BASE_URL}${path}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${TOKEN}" \
    -d "${body}" \
    -w "\n__HTTP_STATUS__:%{http_code}\n"
}

api_get() {
  local path="$1"
  curl -sS -X GET "${BASE_URL}${path}" \
    -H "Authorization: Bearer ${TOKEN}"
}

# Print JSON if possible; otherwise print raw response (prevents jq crashes hiding the real error)
print_json_or_raw() {
  local s="$1"
  if echo "$s" | jq . >/dev/null 2>&1; then
    echo "$s" | jq .
  else
    echo "---- non-JSON response start ----"
    echo "$s"
    echo "---- non-JSON response end ----"
  fi
}

# -----------------------------
# 0) Login as admin
# -----------------------------
echo
echo "[appts_full] 0) login (admin)"
LOGIN_RES="$(curl -sS -X POST "${BASE_URL}/auth/login" \
  -H "Content-Type: application/json" \
  -d "{\"username\":\"${ADMIN_USER}\",\"password\":\"${ADMIN_PASS}\",\"device_name\":\"appts_test\"}")"

print_json_or_raw "${LOGIN_RES}"

TOKEN="$(echo "${LOGIN_RES}" | jq -r '.data.access_token // empty')"
if [[ -z "${TOKEN}" ]]; then
  echo "ERROR: login did not return access_token."
  echo "Hint: run: ENV=dev make reset   (so seed resets password back to admin123)"
  exit 1
fi

# -----------------------------
# 1) Fetch seeded IDs from DB
# -----------------------------
echo
echo "[appts_full] 1) Fetch seeded IDs from DB via psql"

DOC_ID="$(psql "${DATABASE_URL}" -Atc "
SELECT e.employee_id
FROM employee e
JOIN dcms_user u ON u.user_id = e.user_id
WHERE u.roles = 3
ORDER BY e.employee_display_number
LIMIT 1;
")"

PAT_ID="$(psql "${DATABASE_URL}" -Atc "
SELECT patient_id
FROM patient
ORDER BY created_at
LIMIT 1;
")"

SVC1="$(psql "${DATABASE_URL}" -Atc "
SELECT service_id
FROM service_catalog
ORDER BY display_number
LIMIT 1;
")"

SVC2="$(psql "${DATABASE_URL}" -Atc "
SELECT service_id
FROM service_catalog
ORDER BY display_number
OFFSET 1 LIMIT 1;
")"

echo "doctor_employee_id=${DOC_ID}"
echo "patient_id=${PAT_ID}"
echo "service_1=${SVC1}"
echo "service_2=${SVC2}"

if [[ -z "${DOC_ID}" || -z "${PAT_ID}" || -z "${SVC1}" ]]; then
  echo "ERROR: failed to fetch required seeded IDs. Check seeds."
  exit 1
fi

# -----------------------------
# 2) Create an appointment
# -----------------------------
echo
echo "[appts_full] 2) Create appointment"

START_AT="$(date -u -d 'tomorrow 09:00' +%Y-%m-%dT%H:%M:%SZ)"
END_AT="$(date -u -d 'tomorrow 09:30' +%Y-%m-%dT%H:%M:%SZ)"

CREATE_BODY="$(jq -nc \
  --arg patient_id "${PAT_ID}" \
  --arg doctor_employee_id "${DOC_ID}" \
  --arg start_at "${START_AT}" \
  --arg end_at "${END_AT}" \
  --arg note "seeded test appointment" \
  '{
    patient_id: $patient_id,
    doctor_employee_id: $doctor_employee_id,
    start_at: $start_at,
    end_at: $end_at,
    priority: 0,
    is_new_patient: false,
    note: $note
  }'
)"

CREATE_RES="$(api_post_with_status "/appointments" "${CREATE_BODY}")"
CREATE_STATUS="$(echo "${CREATE_RES}" | sed -n 's/^__HTTP_STATUS__://p')"
CREATE_BODY_RES="$(echo "${CREATE_RES}" | sed '/^__HTTP_STATUS__:/d')"

print_json_or_raw "${CREATE_BODY_RES}"
echo "[appts_full] http_status=${CREATE_STATUS}"

APT_ID="$(echo "${CREATE_BODY_RES}" | jq -r '.data.appointment_id // empty')"
if [[ -z "${APT_ID}" ]]; then
  echo "ERROR: create appointment failed"
  echo "${CREATE_BODY_RES}" | jq -r '.error.code?, .error.message?, .message?'
  exit 1
fi
echo "[appts_full] created appointment_id=${APT_ID}"

# -----------------------------
# 3) Put plan items
# -----------------------------
echo
echo "[appts_full] 3) Put plan items"

PLAN_BODY="$(jq -nc \
  --arg svc1 "${SVC1}" \
  --arg svc2 "${SVC2:-$SVC1}" \
  '{
    items: [
      { service_id: $svc1, qty: 1, note: "primary planned" },
      { service_id: $svc2, qty: 1, note: "secondary planned" }
    ]
  }'
)"

PLAN_RES="$(api_put_with_status "/appointments/${APT_ID}/plan_items" "${PLAN_BODY}")"
PLAN_STATUS="$(echo "${PLAN_RES}" | sed -n 's/^__HTTP_STATUS__://p')"
PLAN_BODY_RES="$(echo "${PLAN_RES}" | sed '/^__HTTP_STATUS__:/d')"

print_json_or_raw "${PLAN_BODY_RES}"
echo "[appts_full] plan_items http_status=${PLAN_STATUS}"

if [[ "${PLAN_STATUS}" != "200" ]]; then
  echo "ERROR: put plan items failed"
  echo "${PLAN_BODY_RES}"
  exit 1
fi

# -----------------------------
# 4) Get by id
# -----------------------------
echo
echo "[appts_full] 4) Get appointment by id"
GET_RES="$(api_get "/appointments/${APT_ID}")"
print_json_or_raw "${GET_RES}"

# -----------------------------
# 5) Status transitions: arrived -> seated -> dismissed
# -----------------------------
echo
echo "[appts_full] 5) Mark arrived"
ARR_RES="$(api_post "/appointments/${APT_ID}/arrive" '{}')"
print_json_or_raw "${ARR_RES}"

echo
echo "[appts_full] 6) Mark seated"
SEA_RES="$(api_post "/appointments/${APT_ID}/seat" '{}')"
print_json_or_raw "${SEA_RES}"

echo
echo "[appts_full] 7) Mark dismissed"
DIS_RES="$(api_post "/appointments/${APT_ID}/dismiss" '{}')"
print_json_or_raw "${DIS_RES}"

# -----------------------------
# 6) Week view + today view
# -----------------------------
echo
echo "[appts_full] 8) Week view"
START_DAY="$(date -u +%Y-%m-%d)"
WEEK_RES="$(api_get "/appointments/week?start=${START_DAY}&days=7&doctor_employee_id=${DOC_ID}")"
print_json_or_raw "${WEEK_RES}"

# Show a compact summary: count + first/last start_at (since .data is an array)
echo "${WEEK_RES}" | jq -r '
  .data as $a
  | {
      count: ($a | length),
      first_start_at: ($a | map(.start_at) | min // null),
      last_start_at:  ($a | map(.start_at) | max // null)
    }'

echo
echo "[appts_full] 9) Today view"
TODAY_RES="$(api_get "/appointments/today?doctor_employee_id=${DOC_ID}")"
print_json_or_raw "${TODAY_RES}"
echo "${TODAY_RES}" | jq -r '.data | {count: length}'

echo
echo "[appts_full] ✅ done"
