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

echo "============================================================"
echo "[tasks_p1] BASE_URL=${BASE_URL}"
echo "============================================================"

api_post_with_status() {
  local path="$1"
  local body="$2"
  curl -sS -X POST "${BASE_URL}${path}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${TOKEN}" \
    -d "${body}" \
    -w "\n__HTTP_STATUS__:%{http_code}\n"
}

api_patch_with_status() {
  local path="$1"
  local body="$2"
  curl -sS -X PATCH "${BASE_URL}${path}" \
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

extract_status() { echo "$1" | sed -n 's/^__HTTP_STATUS__://p'; }
extract_body()   { echo "$1" | sed '/^__HTTP_STATUS__:/d'; }

echo
echo "[tasks_p1] 0) login (admin)"
LOGIN_RES="$(curl -sS -X POST "${BASE_URL}/auth/login" \
  -H "Content-Type: application/json" \
  -d "{\"username\":\"${ADMIN_USER}\",\"password\":\"${ADMIN_PASS}\",\"device_name\":\"tasks_p1\"}")"
echo "${LOGIN_RES}" | jq .

TOKEN="$(echo "${LOGIN_RES}" | jq -r '.data.access_token // empty')"
if [[ -z "${TOKEN}" ]]; then
  echo "ERROR: login did not return access_token"
  exit 1
fi

echo
echo "[tasks_p1] 1) fetch seeded IDs"

DOC_ID="$(psql "${DATABASE_URL}" -Atc "
SELECT e.employee_id
FROM employee e
JOIN dcms_user u ON u.user_id = e.user_id
WHERE u.roles = 3
ORDER BY e.employee_display_number
LIMIT 1;
")"

REC_ID="$(psql "${DATABASE_URL}" -Atc "
SELECT e.employee_id
FROM employee e
JOIN dcms_user u ON u.user_id = e.user_id
WHERE u.roles = 4
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
echo "receptionist_employee_id=${REC_ID}"
echo "patient_id=${PAT_ID}"
echo "service_1=${SVC1}"
echo "service_2=${SVC2}"

if [[ -z "${DOC_ID}" || -z "${REC_ID}" || -z "${PAT_ID}" || -z "${SVC1}" ]]; then
  echo "ERROR: missing required seeded IDs"
  exit 1
fi

echo
echo "[tasks_p1] 2) create appointment (for linking)"
START_AT="$(date -u -d 'tomorrow 09:00' +%Y-%m-%dT%H:%M:%SZ)"
END_AT="$(date -u -d 'tomorrow 09:30' +%Y-%m-%dT%H:%M:%SZ)"

CREATE_APPT_BODY="$(jq -nc \
  --arg patient_id "${PAT_ID}" \
  --arg doctor_employee_id "${DOC_ID}" \
  --arg start_at "${START_AT}" \
  --arg end_at "${END_AT}" \
  '{
    patient_id: $patient_id,
    doctor_employee_id: $doctor_employee_id,
    start_at: $start_at,
    end_at: $end_at,
    note: "tasks_p1 linked appt",
    priority: 0,
    is_new_patient: false
  }'
)"

APPT_RES="$(api_post_with_status "/appointments" "${CREATE_APPT_BODY}")"
APPT_STATUS="$(extract_status "${APPT_RES}")"
APPT_BODY="$(extract_body "${APPT_RES}")"
echo "${APPT_BODY}" | jq .
echo "[tasks_p1] appt http_status=${APPT_STATUS}"
APT_ID="$(echo "${APPT_BODY}" | jq -r '.data.appointment_id // empty')"
if [[ -z "${APT_ID}" ]]; then
  echo "ERROR: appointment create failed"
  exit 1
fi

echo
echo "[tasks_p1] 3) create task (unassigned inbox)"
TASK1_BODY="$(jq -nc \
  --arg task_type "CALL_PATIENT" \
  --arg title "Call patient re: overdue follow-up" \
  --arg patient_id "${PAT_ID}" \
  --arg appointment_id "${APT_ID}" \
  '{
    task_type: $task_type,
    title: $title,
    details: "Please call and confirm next visit time.",
    priority: 1,
    patient_id: $patient_id,
    appointment_id: $appointment_id
  }'
)"

TASK1_RES="$(api_post_with_status "/tasks" "${TASK1_BODY}")"
TASK1_STATUS="$(extract_status "${TASK1_RES}")"
TASK1_BODY_RES="$(extract_body "${TASK1_RES}")"
echo "${TASK1_BODY_RES}" | jq .
echo "[tasks_p1] task1 http_status=${TASK1_STATUS}"
TASK1_ID="$(echo "${TASK1_BODY_RES}" | jq -r '.data.task_id // empty')"
[[ -n "${TASK1_ID}" ]] || { echo "ERROR: task1 create failed"; exit 1; }

echo
echo "[tasks_p1] 4) create task (assigned to receptionist)"
TASK2_BODY="$(jq -nc \
  --arg task_type "SMS_PATIENT" \
  --arg title "Send SMS reminder" \
  --arg patient_id "${PAT_ID}" \
  --arg appointment_id "${APT_ID}" \
  --arg assigned_to "${REC_ID}" \
  '{
    task_type: $task_type,
    title: $title,
    details: "Send reminder SMS for tomorrow appointment.",
    priority: 0,
    patient_id: $patient_id,
    appointment_id: $appointment_id,
    assigned_to_employee_id: $assigned_to
  }'
)"

TASK2_RES="$(api_post_with_status "/tasks" "${TASK2_BODY}")"
TASK2_STATUS="$(extract_status "${TASK2_RES}")"
TASK2_BODY_RES="$(extract_body "${TASK2_RES}")"
echo "${TASK2_BODY_RES}" | jq .
echo "[tasks_p1] task2 http_status=${TASK2_STATUS}"
TASK2_ID="$(echo "${TASK2_BODY_RES}" | jq -r '.data.task_id // empty')"
[[ -n "${TASK2_ID}" ]] || { echo "ERROR: task2 create failed"; exit 1; }

echo
echo "[tasks_p1] 5) inbox / created / my"
INBOX="$(api_get "/tasks/inbox")"
echo "${INBOX}" | jq '.data | {count: length}'

CREATED="$(api_get "/tasks/created")"
echo "${CREATED}" | jq '.data | {count: length}'

MY="$(api_get "/tasks/my")"
echo "${MY}" | jq '.data | {count: length}'

echo
echo "[tasks_p1] 6) assign inbox task to receptionist"
ASSIGN_BODY="$(jq -nc --arg eid "${REC_ID}" '{assigned_to_employee_id: $eid}')"
ASSIGN_RES="$(api_post_with_status "/tasks/${TASK1_ID}/assign" "${ASSIGN_BODY}")"
ASSIGN_STATUS="$(extract_status "${ASSIGN_RES}")"
ASSIGN_BODY_RES="$(extract_body "${ASSIGN_RES}")"
echo "${ASSIGN_BODY_RES}" | jq .
echo "[tasks_p1] assign http_status=${ASSIGN_STATUS}"
[[ "${ASSIGN_STATUS}" == "200" ]] || { echo "ERROR: assign failed"; exit 1; }

echo
echo "[tasks_p1] 7) start + complete task2"
START_RES="$(api_post_with_status "/tasks/${TASK2_ID}/start" '{}')"
echo "$(extract_body "${START_RES}")" | jq '.data | {task_id, status, started_at}'
COMP_RES="$(api_post_with_status "/tasks/${TASK2_ID}/complete" '{}')"
echo "$(extract_body "${COMP_RES}")" | jq '.data | {task_id, status, completed_at}'

echo
echo "[tasks_p1] 8) cancel task1 (now assigned) -> should still be allowed for manage role"
CANCEL_RES="$(api_post_with_status "/tasks/${TASK1_ID}/cancel" '{}')"
echo "$(extract_body "${CANCEL_RES}")" | jq '.data | {task_id, status, canceled_at}'

echo
echo "[tasks_p1] ✅ done"
