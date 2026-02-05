# DCMS Backend

tbh i don't even know the difference between backend and server.
This is a **Rust + Axum + PostgreSQL** monolithic backend for a **dental clinic management system**.
Everything here is intentionally boring, explicit, and testable. That’s a compliment.

---

## 1 What this project *is* (conceptually)

DCMS-Rust-Server is:

* A **REST API server**
* Backed by **PostgreSQL**
* Stateless at HTTP level - *is this bad?*
* Stateful via **session tokens stored in DB**
* Role-based access controlled (RBAC)
* Designed to serve:

  * Doctors
  * Receptionists
  * Managers...
  * Admins
  * (Later) patients

* Here in DCMS-Rust-Server , we are just rawdogging SQL, but using sqlx as a helper toolkit.

---

## 2 Tech stack

### Language & runtime

* **Rust (edition 2024)**
  Chosen for:

  * Stable hehe
  * Type safety
  * No runtime surprises in prod

### Async runtime

* **tokio**

  * `rt-multi-thread`: real concurrency
  * `macros`: async main, tests

Everything async is driven by tokio.

---

### Web framework

* **axum 0.8**

  * Router-based
  * Typed extractors
  * Middleware-friendly
  * Minimal opinionation

Axum sits nicely with Tower + Tokio.

---

### HTTP middleware

* **tower-http**

  * tracing (request logs)
  * CORS (frontend access)

---

### Serialization

* **serde + serde_json**

  * All request/response bodies
  * JSON everywhere

---

### Database

* **PostgreSQL**
* **sqlx 0.8**

  * Compile-time checked SQL (macros)
  * No runtime query surprises
  * Strong typing between Rust ↔ SQL

---

### IDs & time

* **uuid v4**
* **chrono**

  * UTC timestamps
  * Serialized cleanly to JSON

---

### Auth & crypto

* **argon2** → password hashing
* **sha2 + base64 + hex** → token hashing / encoding
* **rand** → secure randomness

Passwords are **never reversible**, ever!

---

### Logging & errors

* **tracing**
* **tracing-subscriber**
* **thiserror**
* **anyhow**

You get:

* Structured logs
* Clear error boundaries
* No silent failures - I hope...

---

## 3 Environment & runtime configuration

### `.env`

This file is *required* to run locally.

```.env
ENV=dev
DATABASE_URL=postgres://dcms:314159@127.0.0.1:5432/dcms_dev
BIND_ADDR=127.0.0.1:8080
SESSION_TTL_HOURS=24
RUST_LOG=info
```

What each does:

* `ENV`

  * toggles dev behaviors (logging, safety checks)
* `DATABASE_URL`

  * sqlx uses this directly
* `BIND_ADDR`

  * where Axum listens
* `SESSION_TTL_HOURS`

  * session expiration logic
* `RUST_LOG`

  * controls tracing verbosity

---

## 4 PostgreSQL setup (from zero)

To replicate dev on a new machine:

```bash
sudo apt install postgresql
sudo -u postgres psql
```

note:
*if you are on anything but linux, go figure out on your own :3*
  
```sql
CREATE USER dcms WITH PASSWORD '314159';
CREATE DATABASE dcms_dev OWNER dcms;
```

No extensions required beyond standard Postgres.

---

## 5 Project structure

Explained **top-down**, not alphabetically.

---

### 📁 `migrations/` — the *source of truth*

This is the **real model of the DCMS**.

Order matters. Each file is immutable history.

* `001_core.sql`

  * users
  * roles
  * base tables
* `002_people.sql`

  * people abstraction
* `003_session_token.sql`

  * login sessions
* `004_position.sql`

  * staff roles (doctor, receptionist)
* `005_phone_number_sms.sql`

  * phone numbers + SMS logs
* `006_service_catalog.sql`

  * clinic services
* `007_patient_register_number.sql`
* `008_session_impersonation.sql`
* `009_clinic_settings_v2.sql`
* `010_appointments.sql`
* `011_tasks.sql`
* `012_waitlist.sql`
* `013_patient_notes.sql`
* `014_appointment_source.sql`
* `015_tasks.sql` Feb5

  * task evolution without breaking history

**Design philosophy**:

> DB evolves, never rewritten!
> Backend adapts to DB, not the other way around.
> Data Base is THE source of truth!

---

### 📁 `scripts/` — automation & truth checks

These scripts are **part of the system**, not helpers.

* `reset_dev.sh`

  * drop schema
  * re-run migrations
* `migrate.sh`

  * apply migrations safely
* `seed_dev.sh`

  * insert test users
* `test_*.sh`

  * curl-based contract tests\
  *Since front-end is behind schedule, I am wrapping curl with shell scripts to do some 'unit' tests.*

If a script fails → backend is broken a.k.a me wrote bad program.

---

### 📁 `src/` — the actual server

#### `main.rs`

The **entry point**.

* loads env
* sets up tracing
* builds DB pool - I dunno what is DB pool... Feb5
* builds router
* starts Axum server

---

#### `config.rs`

Reads env vars.
Validates config.
Centralized config access.

---

#### `db.rs`

* Creates `PgPool`
* Connection options
* Pool sizing

Everything DB-related funnels through this.

---

#### `models.rs`

Shared Rust structs:

* request Data Transfer Objects
* response Data Transfer Objects
* row mappings

No DB queries here. Just shapes.

DTOs are intermediate data containers that:

* Serialize (convert to JSON/XML for transport)
* Deserialize (convert from JSON/XML back to objects)
* Decouple internal data structures from external APIs

---

#### `error.rs`

Defines:

* API error format
* HTTP status mapping
* Consistent error responses

This is why frontend always gets `{ error: { message } }`.

---

#### 📁 `middleware/`

##### `auth_context.rs`

This is **critical**.

What it does:

* Reads `Authorization: Bearer ...`
* Validates session token
* Loads:

  * user
  * roles
  * employee_id
* Injects into request extensions

Every protected route depends on this.

---

### 📁 `routes/` — feature slices

Each file = one domain.

This is intentional **DDD-lite**.

* `auth_routes.rs`

  * login
  * logout
  * sessions
  * impersonation
* `user_routes.rs`

  * admin user management
* `patient_routes.rs`

  * CRUD patients
* `patient_comm_routes.rs`

  * phones
  * SMS
* `appointment_routes.rs`

  * scheduling
  * status transitions
* `task_routes.rs`

  * inbox tasks
* `clinic_routes.rs`

  * clinic profile + settings
* `service_routes.rs`

  * (partially implemented)
* `home_routes.rs`

  * health / home API

Each file:

* defines a `router()`
* uses RBAC explicitly
* talks directly to SQL

---

#### `routes/mod.rs`

The **router combiner**.

This is where `/api/v1/*` is assembled.

---

### 📁 `bin/hashpass.rs`

A tiny CLI tool.

Purpose:

* generate Argon2 password hashes
* used for seeding users

I already used it correctly. But will not be needed on the long run.

---

## 6 Makefile

The Makefile is a **developer UX layer**, not a build system.

* `make reset` → wipe & rebuild DB
* `make migrate` → apply migrations
* `make seed` → insert dev users
* `make run` → start server
* `make test` → Rust tests
* `make test-auth`, etc → API contract tests - I never ran this lol

If you can:

```bash
make reset seed run
```

and log in — your environment is correct.

---

## 7 How to “replicate” this project elsewhere

On a new machine:

1. Install Rust (stable)
2. Install PostgreSQL
3. Clone repo
4. Create/Edit `.env`
5. Create DB + user
6. `make reset`
7. `make seed`- optional, bcs reset also seeds
8. `make run`

If step 6 fails → DB problem
If step 8 fails → Rust/config problem

---

## 8 The philosophy already implemented + notes

This backend is:

* Explicit > clever
* SQL-first > ORM magic - I'm skeptical...
* Role-aware > user-centric
* Testable > “trust me vro :>”

* endpoints are verbose - i think... still needs improvements
* status transitions are explicit
* receptionist ≠ doctor ≠ admin

---
