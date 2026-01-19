#!/usr/bin/env bash
set -euo pipefail

BASE="${BASE:-http://127.0.0.1:8080}"

echo "[test] login..."
TOKEN=$(
  curl -sS -X POST "$BASE/api/v1/auth/login" \
    -H "Content-Type: application/json" \
    -d '{"username":"admin","password":"admin123","device_name":"Dev-PC","remember_me":false}' \
  | jq -r '.data.access_token'
)
echo "TOKEN=$TOKEN"

echo "[test] me..."
curl -sS "$BASE/api/v1/auth/me" \
  -H "Authorization: Bearer $TOKEN" | jq

echo "[test] services..."
curl -sS "$BASE/api/v1/services" \
  -H "Authorization: Bearer $TOKEN" | jq

echo "[test] create patient (DB generates register_number)..."
PATIENT_JSON=$(
  curl -sS -X POST "$BASE/api/v1/patients" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{
      "first_name": "John",
      "last_name": "Doe",
      "gender": 0,
      "status": 0
    }'
)

echo "$PATIENT_JSON" | jq
PATIENT_ID=$(echo "$PATIENT_JSON" | jq -r '.patient_id')

if [[ "$PATIENT_ID" == "null" ]]; then
  echo "[error] patient creation failed"
  exit 1
fi

echo "PATIENT_ID=$PATIENT_ID"

echo "[test] get patient..."
curl -sS "$BASE/api/v1/patients/$PATIENT_ID" \
  -H "Authorization: Bearer $TOKEN" | jq

echo "[test] add phone number..."
PN=$(
  curl -sS -X POST "$BASE/api/v1/patients/$PATIENT_ID/phone_numbers" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"phone_number":"+97699112233","label":"Self","is_primary":true}'
)
echo "$PN" | jq
PHONE_ID=$(echo "$PN" | jq -r '.phone_number_id')

echo "[test] list phone numbers..."
curl -sS "$BASE/api/v1/patients/$PATIENT_ID/phone_numbers" \
  -H "Authorization: Bearer $TOKEN" | jq

echo "[test] add sms..."
SMS=$(
  curl -sS -X POST "$BASE/api/v1/phone_numbers/$PHONE_ID/sms" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"direction":1,"sms_text":"Reminder: appointment tomorrow 10:00"}'
)
echo "$SMS" | jq

echo "[test] list sms..."
curl -sS "$BASE/api/v1/phone_numbers/$PHONE_ID/sms" \
  -H "Authorization: Bearer $TOKEN" | jq

echo "[test] logout..."
curl -sS -X POST "$BASE/api/v1/auth/logout" \
  -H "Authorization: Bearer $TOKEN" | jq

echo "[test] done ✅"
