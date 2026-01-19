# DCMS – Frontend (EU) Integration README

> **Audience**: EU (Frontend / UI Engineer)
> **Backend owner**: MT
> **Scope**: Desktop (Tauri) & Web UI integration
> **API version**: `v1` (stable)

---

## 1. Introduction & Scope

This document defines the **authoritative contract** between the DCMS backend server (MT) and frontend applications (EU).

### This README guarantees:

* Stable API paths under `/api/v1`
* Authentication, session, and RBAC behavior
* Error format and status code semantics
* Data constraints that affect UI behavior

### This README does NOT cover:

* Internal Rust/Axum implementation details
* Database schema evolution beyond what impacts UI
* Future modules (appointments, billing, tooth charting) except as placeholders

If something is unclear or missing here, **the backend contract should be updated**, not guessed in the UI.

---

## 2. Quick Start for EU

### 2.1 Running the backend (dev)

```bash
make reset
cargo run
```

Default bind address:

```
http://127.0.0.1:8080
```

### 2.2 API base URL & versioning

All endpoints are under:

```
{BASE_URL}/api/v1
```

Example:

```
http://127.0.0.1:8080/api/v1/auth/login
```

### 2.3 First sanity check

```bash
curl http://127.0.0.1:8080/api/v1/home
```

Expected:

* `401 SESSION_EXPIRED` (means backend is alive)

---

## 3. Architecture Overview (Mental Model)

### 3.1 Single-tenant system

* **Exactly one clinic**
* No `clinic_id` anywhere
* Clinic metadata stored in `clinic_settings` (singleton row)

UI implication:

* No clinic switcher
* Clinic name is global app state

---

### 3.2 Auth model: stateless API + stateful sessions

* Login returns an **opaque access token**
* Client stores token
* Server stores **hash(token)** in DB (`session_token`)
* Every request must include token

Multiple concurrent sessions **are allowed** (multiple devices).

---

### 3.3 Core data domains

* Users (staff accounts)
* Patients
* Patient communications (phone numbers, SMS)
* Service catalog (read-only)
* Clinic settings

---

## 4. Frontend ↔ Backend Contract (MUST READ)

### 4.1 Authentication header (REQUIRED)

Every protected request must include:

```
Authorization: Bearer <access_token>
```

Missing or invalid token → `401 SESSION_EXPIRED`

---

### 4.2 Session lifecycle

**Login**

* `POST /auth/login`
* Returns:

  * `access_token`
  * `expires_at` (UTC ISO-8601)

**Expiration**

* Token expiration is **configurable** on backend
* Default: 24h
* UI must rely on `expires_at`, not assumptions

**Logout**

* Revokes current session only

**Password change / reset**

* Revokes *other* sessions (sometimes all)

UI rule:

* If any request returns `401 SESSION_EXPIRED` → force re-login

---

### 4.3 Response format (current reality)

⚠️ **Important**: responses are currently mixed.

Patterns you will see:

```json
{ "data": { ... } }
```

or

```json
[ { ... }, { ... } ]
```

or

```json
{ "data": { "ok": true } }
```

**EU requirement**:

* Implement a thin response-normalization layer
* Do NOT assume everything is wrapped

(Backend may standardize later; UI must survive now.)

---

### 4.4 Error contract (STABLE)

All errors follow this structure:

```json
{
  "error": {
    "code": "ERROR_CODE",
    "message": "Human readable message",
    "details": null
  }
}
```

Status code semantics:

| HTTP | Meaning               | UI action             |
| ---- | --------------------- | --------------------- |
| 400  | Validation / conflict | Show message inline   |
| 401  | Auth/session invalid  | Force login           |
| 403  | Permission denied     | Disable / hide action |
| 500  | Internal error        | Show generic error    |

Common error codes:

* `INVALID_CREDENTIALS`
* `SESSION_EXPIRED`
* `FORBIDDEN`
* `CONFLICT`
* `VALIDATION_ERROR`

---

### 4.5 Time & date rules

* Backend uses **UTC**
* Format: ISO-8601 (`TIMESTAMPTZ`)
* UI should:

  * store raw UTC
  * render in local time

---

### 4.6 RBAC (roles & UI gating)

Roles are numeric enums in DB:

| Value | Role             |
| ----- | ---------------- |
| 0     | patient (future) |
| 1     | admin            |
| 2     | manager          |
| 3     | doctor           |
| 4     | receptionist     |

Backend may also return role **strings** for convenience.

UI rules:

* Never show actions user cannot perform
* Backend will still enforce permissions

---

## 5. Platform Integration Notes

### 5.1 Tauri-specific cautions

* **Token storage**: use secure keychain (not plain files)
* **Base URL must be configurable**
* **Multiple windows**: sessions are backend-safe
* **Future embedded backend**: avoid assuming fixed ports

### 5.2 Web-specific cautions

* CORS must allow:

  * `Authorization` header
  * JSON content types
* Do not rely on cookies (Bearer tokens only)

---

## 6. API Reference (Complete)

> Base path: `/api/v1`

---

### 6.1 Auth & Sessions

#### POST `/auth/login`

```json
{ "username": "...", "password": "...", "device_name": "optional" }
```

Response:

```json
{
  "data": {
    "access_token": "...",
    "expires_at": "...",
    "dcms_user": { ... },
    "clinic": { "clinic_name": "..." }
  }
}
```

---

#### GET `/auth/me`

* Auth required
* Used on app boot

---

#### POST `/auth/logout`

* Revokes current session

---

#### GET `/auth/sessions`

* Lists active sessions

#### POST `/auth/sessions/revoke_all`

#### POST `/auth/sessions/{session_id}/revoke`

---

#### POST `/auth/change_password`

* Revokes other sessions

#### POST `/auth/reset_password` (admin/manager)

* May return temporary password

---

### 6.2 Home

#### GET `/home`

Returns role-based view hint.

---

### 6.3 Clinic settings

#### GET `/clinic`

#### PATCH `/clinic` (admin/manager)

---

### 6.4 Users (admin / manager)

#### GET `/users`

#### POST `/users`

#### GET `/users/{user_id}`

#### PATCH `/users/{user_id}`

#### POST `/users/{user_id}/disable`

#### POST `/users/{user_id}/enable`

---

### 6.5 Patients

#### POST `/patients`

* `register_number` auto-generated if omitted

#### GET `/patients?query=...`

* If no query → recent patients

#### GET `/patients/{id}`

#### PATCH `/patients/{id}`

* Email can be cleared by sending `null`

#### POST `/patients/{id}/archive`

#### POST `/patients/{id}/restore`

#### GET `/patients/{id}/summary`

* Patient + phones + recent SMS

---

### 6.6 Patient communications

#### Phone numbers

* One primary per patient
* Duplicate numbers blocked

Endpoints:

* `GET /patients/{id}/phone_numbers`
* `POST /patients/{id}/phone_numbers`
* `PATCH /phone_numbers/{id}`
* `DELETE /phone_numbers/{id}` (blocked if SMS exists)
* `POST /phone_numbers/{id}/make_primary`

---

#### SMS

* Stored only (no real provider yet)

Endpoints:

* `GET /phone_numbers/{id}/sms`
* `POST /phone_numbers/{id}/sms`
* `GET /sms` (global search)
* `GET /sms/{id}`
* `DELETE /sms/{id}` (admin)

Bulk:

* `POST /sms/bulk_send`
* `POST /sms/render`

---

### 6.7 Services

#### GET `/services`

* Active services only
* Read-only

---

## 7. Data Dictionary & Constraints (UI-critical)

### Enums

* `gender`: `0/1/2`
* `patient.status`: `0..3`
* `sms.direction`: `0=in, 1=out`
* `session_type`: `0..3`
* `position.category`: `0=clinical, 1=support, 2=admin`

### Constraints to respect in UI

* Patient register number is unique
* Phone number unique per patient
* Only one primary phone number
* Phone numbers with SMS **cannot be deleted**

---

## 8. Recommended UI Flows

### Login flow

* login → store token → `/auth/me`
* on `401` anywhere → redirect to login

### Patient flow

* search → detail → phones → sms

### Admin flows

* manage users
* reset passwords
* clinic settings

---

## 9. Known Limitations & Roadmap Hooks

Not implemented yet:

* Appointments
* Treatments
* Tooth chart / images
* Invoices & payments

API changes:

* Response wrapping may be standardized later
* New modules will go under `/api/v1/...`

---

## 10. Appendix

### Common error handling checklist

* 401 → logout
* 403 → hide UI action
* 400 → inline message
* 500 → generic toast

---

## Final note

This README is the **contract**.
If frontend needs behavior not defined here → **update this doc first**, then implement.
