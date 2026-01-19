#!/usr/bin/env bash
set -e

BASE="${BASE:-http://127.0.0.1:8080}"

TOKEN=$(curl -sS -X POST "$BASE/api/v1/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123","device_name":"test"}' \
  | jq -r '.data.access_token')

PHONE_ID=$(curl -sS "$BASE/api/v1/patients" \
  -H "Authorization: Bearer $TOKEN" \
  | jq -r '.[0].phone_numbers[0].phone_number_id')

curl -sS "$BASE/api/v1/phone_numbers/$PHONE_ID/sms" \
  -H "Authorization: Bearer $TOKEN" | jq
