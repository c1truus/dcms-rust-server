#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:8080/api/v1}"

# ---------- helpers ----------
hr() { echo; echo "============================================================"; echo "== $1"; echo "============================================================"; }
die() { echo "ERROR: $*" >&2; exit 1; }

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

assert_jq_eq() {
  local json="$1" jq_expr="$2" expected="$3"
  local actual
  actual="$(echo "$json" | jq -r "$jq_expr")"
  if [[ "$actual" != "$expected" ]]; then
    echo "$json" | jq . || true
    die "ASSERT FAIL: $jq_expr expected '$expected' got '$actual'"
  fi
}

assert_jq_not_empty() {
  local json="$1" jq_expr="$2"
  local actual
  actual="$(echo "$json" | jq -r "$jq_expr")"
  if [[ -z "$actual" || "$actual" == "null" ]]; then
    echo "$json" | jq . || true
    die "ASSERT FAIL: $jq_expr expected non-empty, got '$actual'"
  fi
}

assert_http_code() {
  local code="$1" expected="$2"
  if [[ "$code" != "$expected" ]]; then
    die "ASSERT FAIL: expected HTTP $expected, got HTTP $code"
  fi
}

# curl wrapper returning: BODY and CODE separately
curl_json() {
  local method="$1" url="$2" token="${3:-}" data="${4:-}"
  local tmp_body tmp_code
  tmp_body="$(mktemp)"
  tmp_code="$(mktemp)"

  local auth_args=()
  if [[ -n "$token" ]]; then
    auth_args=(-H "Authorization: Bearer $token")
  fi

  if [[ -n "$data" ]]; then
    curl -sS -X "$method" "$url" \
      "${auth_args[@]}" \
      -H "Content-Type: application/json" \
      -d "$data" \
      -o "$tmp_body" -w "%{http_code}" > "$tmp_code"
  else
    curl -sS -X "$method" "$url" \
      "${auth_args[@]}" \
      -o "$tmp_body" -w "%{http_code}" > "$tmp_code"
  fi

  cat "$tmp_body"
  echo "::HTTP_CODE::$(cat "$tmp_code")"
  rm -f "$tmp_body" "$tmp_code"
}

split_body() {
  # usage: body="$(split_body "$resp")"
  echo "$1" | sed 's/::HTTP_CODE::.*//'
}

split_code() {
  # usage: code="$(split_code "$resp")"
  echo "$1" | sed -n 's/.*::HTTP_CODE:://p'
}

login() {
  local username="$1" password="$2" device="$3"
  local resp code body token
  resp="$(curl_json POST "$BASE_URL/auth/login" "" "{\"username\":\"$username\",\"password\":\"$password\",\"device_name\":\"$device\"}")"
  code="$(split_code "$resp")"
  body="$(split_body "$resp")"
  assert_http_code "$code" "200"
  token="$(echo "$body" | jq -r '.data.access_token')"
  [[ -n "$token" && "$token" != "null" ]] || die "login failed: no token"
  echo "$token"
}

create_user_admin() {
  # requires: admin token
  local token="$1" username="$2" display="$3" password="$4" roles="$5"
  local resp code body
  resp="$(curl_json POST "$BASE_URL/users" "$token" "{\"username\":\"$username\",\"display_name\":\"$display\",\"password\":\"$password\",\"roles\":$roles}")"
  code="$(split_code "$resp")"
  body="$(split_body "$resp")"
  # could be 200 or 201 depending on your implementation; accept 200/201
  if [[ "$code" != "200" && "$code" != "201" ]]; then
    echo "$body" | jq . || true
    die "create user failed (HTTP $code)"
  fi
  echo "$body"
}

# ---------- preflight ----------
require_cmd curl
require_cmd jq

TS="$(date +%s)"
RAND="${TS}"
PAT_FIRST="PhoneTest_${RAND}"
PAT_LAST="User_${RAND}"

PHONE1="+8613800${RAND: -6}"   # keep within 15 digits after +
PHONE2="+8613900${RAND: -6}"

# ---------- 1) login as admin ----------
hr "Login as admin"
ADMIN_TOKEN="$(login "admin" "admin123" "phone-test-admin")"
echo "[ok] got admin token"

# ---------- 2) ensure a doctor exists for RBAC tests ----------
# We’ll create a doctor user via /users if possible. If /users isn’t available, we’ll skip RBAC negative test.
hr "Ensure doctor user exists for RBAC test (best-effort)"
DOC_USERNAME="doctor_phone_${RAND}"
DOC_PASSWORD="doctor123"
DOC_TOKEN=""

set +e
DOC_CREATE_BODY="$(create_user_admin "$ADMIN_TOKEN" "$DOC_USERNAME" "Dr Phone ${RAND}" "$DOC_PASSWORD" 3 2>/dev/null)"
CREATE_EXIT=$?
set -e

if [[ $CREATE_EXIT -eq 0 ]]; then
  echo "$DOC_CREATE_BODY" | jq .
  DOC_TOKEN="$(login "$DOC_USERNAME" "$DOC_PASSWORD" "phone-test-doctor")"
  echo "[ok] created+logged in doctor token"
else
  echo "[warn] could not create doctor via /users (maybe endpoint not present). RBAC negative delete test will be skipped."
fi

# ---------- 3) create patient ----------
hr "Create patient (POST /patients)"
CREATE_PAT="$(curl_json POST "$BASE_URL/patients" "$ADMIN_TOKEN" "{\"first_name\":\"$PAT_FIRST\",\"last_name\":\"$PAT_LAST\",\"gender\":0}")"
CREATE_PAT_CODE="$(split_code "$CREATE_PAT")"
CREATE_PAT_BODY="$(split_body "$CREATE_PAT")"
assert_http_code "$CREATE_PAT_CODE" "200"
echo "$CREATE_PAT_BODY" | jq .

PATIENT_ID="$(echo "$CREATE_PAT_BODY" | jq -r '.patient_id')"
assert_jq_not_empty "$CREATE_PAT_BODY" ".patient_id"
echo "[ok] patient_id=$PATIENT_ID"

# ---------- 4) normalize endpoint ----------
hr "Normalize phone number (POST /phone_numbers/normalize)"
NORM="$(curl_json POST "$BASE_URL/phone_numbers/normalize" "$ADMIN_TOKEN" "{\"raw\":\" 00 86-138 0012 3456 \"}")"
NORM_CODE="$(split_code "$NORM")"
NORM_BODY="$(split_body "$NORM")"
assert_http_code "$NORM_CODE" "200"
echo "$NORM_BODY" | jq .
assert_jq_eq "$NORM_BODY" ".data.normalized" "+8613800123456"
echo "[ok] normalize works"

# ---------- 5) add first phone number (primary) ----------
hr "Add phone number #1 as primary (POST /patients/{patient_id}/phone_numbers)"
ADD1="$(curl_json POST "$BASE_URL/patients/$PATIENT_ID/phone_numbers" "$ADMIN_TOKEN" "{\"phone_number\":\"$PHONE1\",\"label\":\"Self\",\"is_primary\":true}")"
ADD1_CODE="$(split_code "$ADD1")"
ADD1_BODY="$(split_body "$ADD1")"
assert_http_code "$ADD1_CODE" "200"
echo "$ADD1_BODY" | jq .

PN1_ID="$(echo "$ADD1_BODY" | jq -r '.phone_number_id')"
assert_jq_not_empty "$ADD1_BODY" ".phone_number_id"
assert_jq_eq "$ADD1_BODY" ".is_primary" "true"
assert_jq_eq "$ADD1_BODY" ".phone_number" "$PHONE1"
echo "[ok] phone_number_id #1=$PN1_ID"

# ---------- 6) add second phone number (not primary) ----------
hr "Add phone number #2 not primary (POST /patients/{patient_id}/phone_numbers)"
ADD2="$(curl_json POST "$BASE_URL/patients/$PATIENT_ID/phone_numbers" "$ADMIN_TOKEN" "{\"phone_number\":\"$PHONE2\",\"label\":\"Mother\",\"is_primary\":false}")"
ADD2_CODE="$(split_code "$ADD2")"
ADD2_BODY="$(split_body "$ADD2")"
assert_http_code "$ADD2_CODE" "200"
echo "$ADD2_BODY" | jq .

PN2_ID="$(echo "$ADD2_BODY" | jq -r '.phone_number_id')"
assert_jq_not_empty "$ADD2_BODY" ".phone_number_id"
assert_jq_eq "$ADD2_BODY" ".is_primary" "false"
echo "[ok] phone_number_id #2=$PN2_ID"

# ---------- 7) list patient phone numbers ----------
hr "List patient phone numbers (GET /patients/{patient_id}/phone_numbers)"
LIST1="$(curl_json GET "$BASE_URL/patients/$PATIENT_ID/phone_numbers" "$ADMIN_TOKEN")"
LIST1_CODE="$(split_code "$LIST1")"
LIST1_BODY="$(split_body "$LIST1")"
assert_http_code "$LIST1_CODE" "200"
echo "$LIST1_BODY" | jq .

COUNT="$(echo "$LIST1_BODY" | jq 'length')"
[[ "$COUNT" -ge 2 ]] || die "expected >=2 phone numbers, got $COUNT"
echo "[ok] list contains $COUNT rows"

# Ensure only one primary right now and it's PN1
PRIMARY_COUNT="$(echo "$LIST1_BODY" | jq '[.[] | select(.is_primary==true)] | length')"
[[ "$PRIMARY_COUNT" -eq 1 ]] || die "expected exactly 1 primary, got $PRIMARY_COUNT"
PRIMARY_ID="$(echo "$LIST1_BODY" | jq -r '.[] | select(.is_primary==true) | .phone_number_id')"
[[ "$PRIMARY_ID" == "$PN1_ID" ]] || die "expected PN1 to be primary initially"
echo "[ok] single primary enforced initially"

# ---------- 8) get phone number by id ----------
hr "Get one phone number (GET /phone_numbers/{phone_number_id})"
GET1="$(curl_json GET "$BASE_URL/phone_numbers/$PN1_ID" "$ADMIN_TOKEN")"
GET1_CODE="$(split_code "$GET1")"
GET1_BODY="$(split_body "$GET1")"
assert_http_code "$GET1_CODE" "200"
echo "$GET1_BODY" | jq .
assert_jq_eq "$GET1_BODY" ".phone_number_id" "$PN1_ID"
assert_jq_eq "$GET1_BODY" ".patient_id" "$PATIENT_ID"
echo "[ok] get one works"

# ---------- 9) make_primary on PN2 (should flip primary) ----------
hr "Make PN2 primary (POST /phone_numbers/{id}/make_primary)"
MP="$(curl_json POST "$BASE_URL/phone_numbers/$PN2_ID/make_primary" "$ADMIN_TOKEN")"
MP_CODE="$(split_code "$MP")"
MP_BODY="$(split_body "$MP")"
assert_http_code "$MP_CODE" "200"
echo "$MP_BODY" | jq .
assert_jq_eq "$MP_BODY" ".phone_number_id" "$PN2_ID"
assert_jq_eq "$MP_BODY" ".is_primary" "true"
echo "[ok] make_primary set PN2 primary"

hr "Verify only PN2 is primary after flip (GET /patients/{patient_id}/phone_numbers)"
LIST2="$(curl_json GET "$BASE_URL/patients/$PATIENT_ID/phone_numbers" "$ADMIN_TOKEN")"
LIST2_BODY="$(split_body "$LIST2")"
echo "$LIST2_BODY" | jq .
PRIMARY_COUNT2="$(echo "$LIST2_BODY" | jq '[.[] | select(.is_primary==true)] | length')"
[[ "$PRIMARY_COUNT2" -eq 1 ]] || die "expected exactly 1 primary after flip, got $PRIMARY_COUNT2"
PRIMARY_ID2="$(echo "$LIST2_BODY" | jq -r '.[] | select(.is_primary==true) | .phone_number_id')"
[[ "$PRIMARY_ID2" == "$PN2_ID" ]] || die "expected PN2 to be primary after flip"
echo "[ok] single primary enforced after flip"

# ---------- 10) PATCH: update label + phone normalization (also test that label cannot be empty) ----------
hr "PATCH phone number label + phone_number (PATCH /phone_numbers/{id})"
PATCH1="$(curl_json PATCH "$BASE_URL/phone_numbers/$PN2_ID" "$ADMIN_TOKEN" "{\"label\":\"Mom\",\"phone_number\":\" 00 86 139 1111 2222 \"}")"
PATCH1_CODE="$(split_code "$PATCH1")"
PATCH1_BODY="$(split_body "$PATCH1")"
assert_http_code "$PATCH1_CODE" "200"
echo "$PATCH1_BODY" | jq .
assert_jq_eq "$PATCH1_BODY" ".label" "Mom"
assert_jq_eq "$PATCH1_BODY" ".phone_number" "+8613911112222"
echo "[ok] PATCH updated label + normalized number"

hr "PATCH label empty should fail (PATCH /phone_numbers/{id})"
PATCH_BAD="$(curl_json PATCH "$BASE_URL/phone_numbers/$PN2_ID" "$ADMIN_TOKEN" "{\"label\":\"   \"}")"
PATCH_BAD_CODE="$(split_code "$PATCH_BAD")"
PATCH_BAD_BODY="$(split_body "$PATCH_BAD")"
# expect 400
assert_http_code "$PATCH_BAD_CODE" "400"
echo "$PATCH_BAD_BODY" | jq .
echo "[ok] empty label rejected"

# ---------- 11) RBAC: doctor cannot DELETE (if doctor token exists) ----------
if [[ -n "$DOC_TOKEN" ]]; then
  hr "RBAC: doctor cannot delete phone number (DELETE /phone_numbers/{id})"
  DEL_DOC="$(curl_json DELETE "$BASE_URL/phone_numbers/$PN1_ID" "$DOC_TOKEN")"
  DEL_DOC_CODE="$(split_code "$DEL_DOC")"
  DEL_DOC_BODY="$(split_body "$DEL_DOC")"
  # should be 403 forbidden
  assert_http_code "$DEL_DOC_CODE" "403"
  echo "$DEL_DOC_BODY" | jq .
  echo "[ok] RBAC delete forbidden for doctor"
else
  hr "RBAC: doctor delete test skipped (no doctor token)"
fi

# ---------- 12) DELETE blocked by SMS history ----------
hr "Create SMS on PN1 to block delete (POST /phone_numbers/{id}/sms)"
SMS1="$(curl_json POST "$BASE_URL/phone_numbers/$PN1_ID/sms" "$ADMIN_TOKEN" "{\"direction\":1,\"sms_text\":\"Reminder test $RAND\"}")"
SMS1_CODE="$(split_code "$SMS1")"
SMS1_BODY="$(split_body "$SMS1")"
assert_http_code "$SMS1_CODE" "200"
echo "$SMS1_BODY" | jq .
SMS_ID="$(echo "$SMS1_BODY" | jq -r '.sms_id')"
assert_jq_not_empty "$SMS1_BODY" ".sms_id"
echo "[ok] sms created sms_id=$SMS_ID"

hr "Admin delete PN1 should be blocked because SMS exists (DELETE /phone_numbers/{id})"
DEL_BLOCK="$(curl_json DELETE "$BASE_URL/phone_numbers/$PN1_ID" "$ADMIN_TOKEN")"
DEL_BLOCK_CODE="$(split_code "$DEL_BLOCK")"
DEL_BLOCK_BODY="$(split_body "$DEL_BLOCK")"
# expect 400 (your implementation uses BadRequest CONFLICT)
assert_http_code "$DEL_BLOCK_CODE" "400"
echo "$DEL_BLOCK_BODY" | jq .
echo "[ok] delete blocked when sms exists"

# ---------- 13) Admin delete PN2 (should succeed if no SMS exists on PN2) ----------
hr "Admin delete PN2 (DELETE /phone_numbers/{id})"
DEL_OK="$(curl_json DELETE "$BASE_URL/phone_numbers/$PN2_ID" "$ADMIN_TOKEN")"
DEL_OK_CODE="$(split_code "$DEL_OK")"
DEL_OK_BODY="$(split_body "$DEL_OK")"
assert_http_code "$DEL_OK_CODE" "200"
echo "$DEL_OK_BODY" | jq .
# ok response shape: {"data":{"ok":true}}
assert_jq_eq "$DEL_OK_BODY" ".data.ok" "true"
echo "[ok] delete succeeded for PN2"

hr "Verify PN2 gone (GET /phone_numbers/{id})"
GET_GONE="$(curl_json GET "$BASE_URL/phone_numbers/$PN2_ID" "$ADMIN_TOKEN")"
GET_GONE_CODE="$(split_code "$GET_GONE")"
GET_GONE_BODY="$(split_body "$GET_GONE")"
assert_http_code "$GET_GONE_CODE" "400"
echo "$GET_GONE_BODY" | jq .
echo "[ok] PN2 not found after delete"

hr "Final list for patient (GET /patients/{patient_id}/phone_numbers)"
FINAL_LIST="$(curl_json GET "$BASE_URL/patients/$PATIENT_ID/phone_numbers" "$ADMIN_TOKEN")"
FINAL_LIST_CODE="$(split_code "$FINAL_LIST")"
FINAL_LIST_BODY="$(split_body "$FINAL_LIST")"
assert_http_code "$FINAL_LIST_CODE" "200"
echo "$FINAL_LIST_BODY" | jq .
echo "[ok] phone numbers feature test complete ✅"
