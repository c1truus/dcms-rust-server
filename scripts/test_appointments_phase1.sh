# scripts/test_appointments_phase1.sh s
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
  echo "ERROR: DATABASE_URL is required"
  echo "Example: export DATABASE_URL='postgres://dcms:314159@127.0.0.1:5432/dcms_dev'"
  exit 1
fi

api_post_ws() {
  local path="$1"
  local body="$2"
  curl -sS -X POST "${BASE_URL}${path}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${TOKEN}" \
    -d "${body}" \
    -w "\n__HTTP_STATUS__:%{http_code}\n"
}

api_put_ws() {
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

echo "============================================================"
echo "[appts_p1] BASE_URL=${BASE_URL}"
echo "============================================================"

echo
echo "[appts_p1] 0) login (admin)"
LOGIN_RES="$(curl -sS -X POST "${BASE_URL}/auth/login" \
  -H "Content-Type: application/json" \
  -d "{\"username\":\"${ADMIN_USER}\",\"password\":\"${ADMIN_PASS}\",\"device_name\":\"appts_p1\"}")"

echo "${LOGIN_RES}" | jq .
TOKEN="$(echo "${LOGIN_RES}" | jq -r '.data.access_token // empty')"
[[ -n "${TOKEN}" ]] || { echo "ERROR: no access_token"; exit 1; }

echo
echo "[appts_p1] 1) fetch seeded IDs"
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
[[ -n "${DOC_ID}" && -n "${PAT_ID}" && -n "${SVC1}" ]] || { echo "ERROR: missing seeds"; exit 1; }

echo
echo "[appts_p1] 2) create appointment"
START_AT="$(date -u -d 'tomorrow 09:00' +%Y-%m-%dT%H:%M:%SZ)"
END_AT="$(date -u -d 'tomorrow 09:30' +%Y-%m-%dT%H:%M:%SZ)"

CREATE_BODY="$(jq -nc \
  --arg patient_id "${PAT_ID}" \
  --arg doctor_employee_id "${DOC_ID}" \
  --arg start_at "${START_AT}" \
  --arg end_at "${END_AT}" \
  '{
    patient_id: $patient_id,
    doctor_employee_id: $doctor_employee_id,
    start_at: $start_at,
    end_at: $end_at,
    priority: 0,
    is_new_patient: false,
    note: "phase1 appt test",
    source: "SCHEDULED"
  }'
)"

CREATE_RES="$(api_post_ws "/appointments" "${CREATE_BODY}")"
CREATE_STATUS="$(echo "${CREATE_RES}" | sed -n 's/^__HTTP_STATUS__://p')"
CREATE_JSON="$(echo "${CREATE_RES}" | sed '/^__HTTP_STATUS__:/d')"

echo "${CREATE_JSON}" | jq .
echo "[appts_p1] create http_status=${CREATE_STATUS}"
[[ "${CREATE_STATUS}" == "200" ]] || { echo "ERROR: create failed"; exit 1; }

APT_ID="$(echo "${CREATE_JSON}" | jq -r '.data.appointment_id // empty')"
[[ -n "${APT_ID}" ]] || { echo "ERROR: no appointment_id"; exit 1; }
echo "[appts_p1] appointment_id=${APT_ID}"

echo
echo "[appts_p1] 3) put plan items"
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

PLAN_RES="$(api_put_ws "/appointments/${APT_ID}/plan_items" "${PLAN_BODY}")"
PLAN_STATUS="$(echo "${PLAN_RES}" | sed -n 's/^__HTTP_STATUS__://p')"
PLAN_JSON="$(echo "${PLAN_RES}" | sed '/^__HTTP_STATUS__:/d')"
echo "${PLAN_JSON}" | jq .
echo "[appts_p1] plan_items http_status=${PLAN_STATUS}"
[[ "${PLAN_STATUS}" == "200" ]] || { echo "ERROR: plan_items failed"; exit 1; }

echo
echo "[appts_p1] 4) confirm + reminder_sent"
CONF_RES="$(api_post_ws "/appointments/${APT_ID}/confirm" '{}')"
CONF_STATUS="$(echo "${CONF_RES}" | sed -n 's/^__HTTP_STATUS__://p')"
CONF_JSON="$(echo "${CONF_RES}" | sed '/^__HTTP_STATUS__:/d')"
echo "${CONF_JSON}" | jq '.data | {appointment_id, confirmed_at, reminder_sent_at}'
[[ "${CONF_STATUS}" == "200" ]] || { echo "ERROR: confirm failed"; exit 1; }

REM_RES="$(api_post_ws "/appointments/${APT_ID}/reminder_sent" '{}')"
REM_STATUS="$(echo "${REM_RES}" | sed -n 's/^__HTTP_STATUS__://p')"
REM_JSON="$(echo "${REM_RES}" | sed '/^__HTTP_STATUS__:/d')"
echo "${REM_JSON}" | jq '.data | {appointment_id, confirmed_at, reminder_sent_at}'
[[ "${REM_STATUS}" == "200" ]] || { echo "ERROR: reminder_sent failed"; exit 1; }

echo
echo "[appts_p1] 5) day/week/today/overdue views"
TOMORROW_DAY="$(date -u -d 'tomorrow' +%Y-%m-%d)"
TODAY_DAY="$(date -u +%Y-%m-%d)"

DAY_RES="$(api_get "/appointments/day?date=${TOMORROW_DAY}&doctor_employee_id=${DOC_ID}")"
echo "${DAY_RES}" | jq '.data | {count: length}'

WEEK_RES="$(api_get "/appointments/week?start=${TODAY_DAY}&days=7&doctor_employee_id=${DOC_ID}")"
echo "${WEEK_RES}" | jq '.data | {count: length}'

TODAY_RES="$(api_get "/appointments/today?doctor_employee_id=${DOC_ID}")"
echo "${TODAY_RES}" | jq '.data | {count: length}'

OVERDUE_RES="$(api_get "/appointments/overdue?doctor_employee_id=${DOC_ID}&within_days=30")"
echo "${OVERDUE_RES}" | jq '.data | {count: length}'

echo
echo "[appts_p1] ✅ done"
