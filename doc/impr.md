# DCMS-Server Improvement Plan (Single-tenant, Dev-first, Deploy-friendly)

> **Single-tenant decision:** One clinic per deployment.  
> Practically: **one server instance + one Postgres database** per clinic/customer.  
> This removes tenant scoping (`clinic_id`) from most tables and simplifies queries + auth.

---

## 0) Core Design Choices (Single-tenant)

### 0.1 DB is the source of truth
- Keep strict Postgres constraints:
  - FK, NOT NULL, UNIQUE, CHECK constraints
  - exclusion constraint for appointment overlaps (doctor + time range)
- App validates too, but DB prevents impossible states.

### 0.2 Migrations only
- No manual schema edits in psql.
- Every schema change = new migration file.
- Dev reset = reset schema + migrate + seed.
- **Because you may not have CREATEDB permission**, default reset method should drop/recreate schema, not database.

### 0.3 Thin HTTP layer
- `routes/` = request parsing + response formatting
- `services/` = business rules
- `repos/` = SQLx queries only

### 0.4 Safe schema evolution: expand → backfill → contract
- Add new columns/tables first
- Backfill data
- Remove old columns/tables after code is fully switched

### 0.5 Single-tenant clinic metadata
- Instead of a `clinics` table + `clinic_id`, store clinic info in a singleton table:
  - `clinic_settings(singleton_id=true, clinic_name, ...)`

---

## 1) Target Folder Layout (unchanged structurally)

```

src/
main.rs
lib.rs (optional)

config.rs
db.rs
error.rs

models/
mod.rs
common.rs
auth.rs
clinic.rs          (clinic_settings DTOs)
patient.rs
appointment.rs
billing.rs
enums.rs

auth/
mod.rs
token.rs
password.rs
guard.rs

middleware/
mod.rs
auth_context.rs

repos/
mod.rs
auth_repo.rs
clinic_repo.rs     (load clinic_settings)
patient_repo.rs
appointment_repo.rs
billing_repo.rs

services/
mod.rs
auth_service.rs
patient_service.rs
appointment_service.rs
billing_service.rs

routes/
mod.rs
auth_routes.rs
patient_routes.rs
appointment_routes.rs
billing_routes.rs
health_routes.rs

utils/
mod.rs
time.rs

migrations/
001_auth.sql
002_patients.sql
003_appointments.sql
004_services.sql
005_invoices.sql
006_payments.sql
007_indexes.sql

scripts/
seed_dev.sql
reset_dev.sh
migrate.sh

docker/
Dockerfile
docker-compose.yml

```

---

## 2) Dev Workflow (Single-tenant edition)

### 2.1 Reset strategy (important)
Because `dcms` might not have permission to `createdb`, make reset do **schema reset**, not db drop:

**TODO**
- [ ] `scripts/reset_dev.sh`:
  1) `DROP SCHEMA public CASCADE; CREATE SCHEMA public;`
  2) `sqlx migrate run`
  3) `psql $DATABASE_URL -f scripts/seed_dev.sql`

### 2.2 Install + use sqlx-cli
**TODO**
- [ ] Ensure every dev has:
  - `cargo install sqlx-cli --no-default-features --features postgres`

### 2.3 Makefile tasks (optional but recommended)
- `make reset`
- `make migrate`
- `make run`
- `make test`
- `make fmt`
- `make clippy`

### 2.4 ENV safety
**TODO**
- [ ] Add `ENV=dev|prod` to `.env`
- [ ] Seed refuses to run unless `ENV=dev`

---

## 3) Auth & Authorization (Single-tenant)

### 3.1 AuthContext contents
Single-tenant means AuthContext should contain:
- `user_id`
- `roles[]`
- `session_id`
(no clinic_id)

**TODO**
- [ ] Add role-check helpers:
  - `require_any_role(&auth, &["admin","manager"])`
  - `require_role(&auth, "admin")`

### 3.2 Admin visibility: sessions
Single-tenant admin needs:
- list active sessions
- revoke a session
- revoke all for dcms_user

**TODO**
- [ ] `GET /api/v1/admin/sessions` (admin-only)
- [ ] `POST /api/v1/admin/sessions/:id/revoke`
- [ ] `POST /api/v1/admin/users/:id/revoke_sessions`

---

## 4) Database Roadmap (Core Workflows) — updated for single-tenant

> The biggest change: **remove `clinic_id` columns** from domain tables.  
> Tenant isolation is provided by “one DB per clinic”.

### 4.0 Clinic settings (new baseline)
**Migration 001_auth.sql already includes**
- `clinic_settings(singleton_id=true, clinic_name, ...)`

**TODO**
- [ ] Add fields later:
  - timezone, address, phone, currency, etc.

---

### 4.1 Patients (no clinic_id)
**Migration 002_patients.sql**
- `patients`
  - patient_id UUID PK
  - register_number UNIQUE (within this DB)
  - names, birthday, gender, etc.
  - created_at, updated_at
- `patient_contacts`
  - patient_id FK
  - type (phone/email)
  - value UNIQUE (optional)
  - is_primary

**TODO**
- [ ] Patients CRUD endpoints
- [ ] Unique constraints:
  - register_number unique
  - optional: unique primary contact per patient

---

### 4.2 Appointments (no clinic_id, but doctor ownership)
**Migration 003_appointments.sql**
- `appointments`
  - appointment_id UUID PK
  - patient_id FK
  - doctor_id FK (users)
  - starts_at, ends_at
  - status enum/check
  - notes
  - created_at, updated_at

**No overlap rule**
- exclusion constraint on `(doctor_id, tstzrange(starts_at, ends_at))`

**TODO**
- [ ] Create appointment (transaction)
- [ ] Reschedule
- [ ] Status transitions
- [ ] Indexes:
  - (doctor_id, starts_at)
  - (patient_id, starts_at)

---

### 4.3 Services catalog (no clinic_id)
**Migration 004_services.sql**
- `services`
  - service_id UUID PK
  - code UNIQUE
  - name
  - current_price_cents
  - is_active

---

### 4.4 Invoices (no clinic_id)
**Migration 005_invoices.sql**
- `invoices`
  - invoice_id UUID PK
  - patient_id FK
  - appointment_id FK nullable
  - status
  - currency
  - total_cents
  - created_at, issued_at, voided_at
- `invoice_items`
  - invoice_item_id UUID PK
  - invoice_id FK
  - service_id FK nullable
  - description_snapshot
  - unit_price_snapshot_cents
  - qty
  - line_total_cents

**Rule**
- snapshot service name + price into invoice_items

---

### 4.5 Payments (no clinic_id)
**Migration 006_payments.sql**
- `payments`
  - payment_id UUID PK
  - invoice_id FK
  - amount_cents
  - method enum/check
  - paid_at
  - reference

**TODO**
- [ ] partial payment support
- [ ] invoice status updates:
  - issued → partially_paid → paid
- [ ] prevent paying void invoices

---

## 5) Transactions (same, but simpler queries)

**Must use transactions for**
- [ ] appointment create/reschedule
- [ ] invoice create/issue
- [ ] record payment + update invoice status
- [ ] (optional) login session create

---

## 6) Testing Strategy (Single-tenant)

### 6.1 Integration tests with Postgres
- Use testcontainers or docker compose
- Tests should **create their own users** and data

**TODO**
- [ ] login success/fail
- [ ] protected route requires auth
- [ ] appointment overlap rejected (409)
- [ ] invoice snapshot correctness
- [ ] partial payment updates invoice status

---

## 7) Observability + daemon-grade behavior

**TODO**
- [ ] `GET /health` (process alive)
- [ ] `GET /ready` (db reachable + migrations applied)
- [ ] Graceful shutdown
- [ ] Request timeouts
- [ ] DB statement / acquire timeouts
- [ ] Structured tracing spans + request IDs

---

## 8) Deployment Plan (Single-tenant deployment model)

### 8.1 Default deployment model (recommended)
- **One clinic = one compose stack**:
  - `api` container
  - `postgres` container
- `DATABASE_URL` points to that clinic DB

### 8.2 Migration strategy in deployment
**TODO**
- [ ] choose one:
  - (A) run migrations on startup (api)
  - (B) run one-shot `migrate` container before api starts (cleaner)

### 8.3 Backups
Single-tenant makes backups easy:
- [ ] pg_dump scheduled backup per clinic
- [ ] retention policy

---

## 9) Concrete Step-by-Step TODO Roadmap (Single-tenant)

### Step 1 — finish the single-tenant conversion (now)
- [x] remove clinic_id from auth/session/dcms_user models
- [x] add clinic_settings singleton
- [ ] add `scripts/reset_dev.sh` using schema reset
- [ ] make `sqlx migrate run` + seed fully repeatable

### Step 2 — architecture refactor (1–2 sessions)
- [ ] introduce `repos/` and move SQL out of routes/middleware
- [ ] introduce `services/` and move business rules there
- [ ] keep routes thin

### Step 3 — implement core workflows vertically (patients → appointments → billing)
- [ ] patients CRUD
- [ ] appointments booking w/ overlap constraint
- [ ] services catalog CRUD (admin/manager only)
- [ ] invoices + invoice_items snapshot
- [ ] payments partial + invoice status update logic

### Step 4 — correctness + production-hardening
- [ ] transactions around all writes
- [ ] role guard helpers used everywhere
- [ ] add admin session listing + revoke endpoints
- [ ] health/ready endpoints + graceful shutdown + timeouts

### Step 5 — tests
- [ ] integration tests for core flows
- [ ] appointment overlap test mandatory

### Step 6 — deploy
- [ ] dockerize
- [ ] migrations strategy
- [ ] backup plan

---

## 10) Definition of Done (next milestone)

You’re ready to build the UI seriously when:
- [ ] reset_dev works on any dev machine (no manual DB steps)
- [ ] admin login works consistently
- [ ] patients CRUD exists
- [ ] appointment booking rejects overlaps
- [ ] invoice snapshots service price/name
- [ ] partial payments update invoice status correctly
- [ ] core write flows are transaction-safe
- [ ] integration tests pass
