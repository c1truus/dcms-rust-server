bash -lc cat > /mnt/data/endpoint-list.md <<'MD'
# DCMS Server API Endpoints (v1)

Base prefix: `http://<host>:<port>/api/v1`

Conventions
- Most endpoints require `Authorization: Bearer <access_token>`.
- Responses generally follow `{ "data": ... }` on success and `{ "error": { "code": ..., "message": ... } }` on failure.

---

## Auth & Sessions (`/api/v1/auth/*`)

| Method | Path | What it does | Returns (high-level) |
|---|---|---|---|
| POST | `/auth/login` | Staff login (creates a session token). | `access_token`, `expires_at`, `user` profile, `clinic` profile |
| POST | `/auth/patient/login` | Patient login (future/mobile patient web). | same shape as login (patient session type) |
| GET | `/auth/me` | Who am I (based on bearer token). | current user profile + roles |
| POST | `/auth/logout` | Logout current session (revoke current token). | `{ ok: true }` |
| POST | `/auth/logout_all_except_current` | Revoke all other sessions, keep current one. | `{ ok: true }` |
| POST | `/auth/refresh` | Rotate/refresh access token for current session. | new `access_token`, `expires_at` |
| GET | `/auth/sessions` | List sessions for current user. | sessions list + current session id |
| GET | `/auth/sessions/{session_token_id}` | Inspect one session. | session details (device, expiry, revoked state, etc.) |
| POST | `/auth/sessions/{session_token_id}/extend` | Extend a session’s expiry (bounded by server rules). | updated session expiry info |
| POST | `/auth/sessions/{session_token_id}/revoke` | Revoke one session. | `{ ok: true }` |
| POST | `/auth/sessions/revoke_all` | Revoke all sessions for user. | `{ ok: true }` |
| POST | `/auth/impersonate/{user_id}` | **Admin-only**: create an impersonation session for target user. | new `access_token` + impersonation metadata |
| POST | `/auth/change_password` | Change password (current user). | `{ ok: true }` |
| POST | `/auth/reset_password` | **Admin/manager**: reset another user’s password. | `{ ok: true }` (or temp password depending on impl) |

---

## Clinic (single-tenant) (`/api/v1/*`)

> These are singleton resources (one clinic per deployment).

| Method | Path | What it does | Returns (high-level) |
|---|---|---|---|
| GET | `/clinic` | Read clinic profile (currently clinic name). | `{ clinic_name }` |
| PATCH | `/clinic` | **Admin-only**: update clinic profile fields. | updated `{ clinic_name }` |
| GET | `/clinic/settings` | Read operational settings (timezone, slots, hours, …). | full settings object |
| PATCH | `/clinic/settings` | **Admin-only**: partial update + validation + audit. | updated settings object |
| GET | `/clinic/meta` | UI helper payload derived from settings (dropdown options, etc.). | timezone, slot minutes, business hours, helper lists |

---

## Home (`/home`)

| Method | Path | What it does | Returns (high-level) |
|---|---|---|---|
| GET | `/home` | Role-based “home” placeholder payload. | `{ view, message }` |

---

## Users (`/api/v1/users/*`)

> Intended for admin/manager staff.

| Method | Path | What it does | Returns (high-level) |
|---|---|---|---|
| GET | `/users/` | List users (admin/manager). | list of user public rows |
| POST | `/users/` | Create a new user (admin/manager). | created user public row |
| GET | `/users/{user_id}` | Get a user by id (admin/manager). | user public row |
| PATCH | `/users/{user_id}` | Update display name / roles / active flag (admin/manager). | updated user public row |
| POST | `/users/{user_id}/disable` | Disable a user (admin/manager). | `{ ok: true }` |
| POST | `/users/{user_id}/enable` | Enable a user (admin/manager). | `{ ok: true }` |

---

## Service Catalog (`/api/v1/services/*`)

| Method | Path | What it does | Returns (high-level) |
|---|---|---|---|
| GET | `/services/` | List active services from `service_catalog`. | array of service items (name, price, duration, etc.) |

---

## Patients (`/api/v1/patients*`)

| Method | Path | What it does | Returns (high-level) |
|---|---|---|---|
| POST | `/patients` | Create a patient (register number auto or provided). | patient row |
| GET | `/patients` | Search patients by `query` (name/register); empty => recent. | list of patient rows |
| GET | `/patients/{patient_id}` | Get patient details. | patient row |
| PATCH | `/patients/{patient_id}` | Update patient fields (profile info). | updated patient row |
| GET | `/patients/{patient_id}/summary` | Summary payload for patient dashboard. | lightweight summary (depends on impl) |
| POST | `/patients/{patient_id}/archive` | Archive patient (soft disable). | `{ ok: true }` |
| POST | `/patients/{patient_id}/restore` | Restore archived patient. | `{ ok: true }` |
| POST | `/patients/{patient_id}/link_user/{user_id}` | Link a patient record to a `dcms_user` (portal login). | `{ ok: true }` or updated patient |
| POST | `/patients/{patient_id}/unlink_user` | Remove linked user from patient record. | `{ ok: true }` or updated patient |

Query params
- `GET /patients?query=...`

---

## Patient Communication (Phone Numbers + SMS) (`/api/v1/*`)

### Phone numbers

| Method | Path | What it does | Returns (high-level) |
|---|---|---|---|
| GET | `/patients/{patient_id}/phone_numbers` | List patient phone numbers. | array of phone number rows |
| POST | `/patients/{patient_id}/phone_numbers` | Add a phone number to patient. | created phone number row |
| GET | `/patients/{patient_id}/phone_numbers_alias` | Alias of list endpoint (same response). | array of phone number rows |
| POST | `/phone_numbers/normalize` | Normalize/validate phone formatting (utility). | normalized result |
| GET | `/phone_numbers/{phone_number_id}` | Get one phone number. | phone number row |
| PATCH | `/phone_numbers/{phone_number_id}` | Update label / phone string / primary flag rules. | updated phone number row |
| DELETE | `/phone_numbers/{phone_number_id}` | Delete phone number. | `{ ok: true }` |
| POST | `/phone_numbers/{phone_number_id}/make_primary` | Make this number primary (and unset others). | `{ ok: true }` |

### SMS

| Method | Path | What it does | Returns (high-level) |
|---|---|---|---|
| GET | `/phone_numbers/{phone_number_id}/sms` | List SMS history for a phone number. | array of sms rows |
| POST | `/phone_numbers/{phone_number_id}/sms` | Add a manual SMS log entry. | created sms row |
| GET | `/sms` | Global SMS search/filter. | array of sms rows |
| GET | `/sms/{sms_id}` | Get a single SMS. | sms row |
| DELETE | `/sms/{sms_id}` | Delete SMS record. | `{ ok: true }` |
| POST | `/sms/bulk_send` | Bulk send/record SMS (server-side helper). | summary of sends/results |
| POST | `/sms/render` | Render SMS template (server-side helper). | rendered text |

---

## Notes / Next docs
gonna add enpoint openapi.json later
