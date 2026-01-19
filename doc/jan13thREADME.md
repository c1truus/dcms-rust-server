# DCMS-Server (Backend)

Dental Clinic Management System — backend server  
Tech stack: **Rust + Axum + PostgreSQL + SQLx**

---

## 🚀 What this project is

DCMS-Server is a **single-tenant backend** for a dental clinic management system.

**Single-tenant means:**
- One clinic = one database
- One server instance serves one clinic
- No `clinic_id` everywhere — the database itself is the tenant boundary

This design keeps:
- auth logic simpler
- queries cleaner
- security easier to reason about
- deployment predictable

---

## 🧠 Current Project Stage

**Stage: Infrastructure & Dev Workflow Stabilization**

What’s already done:
- Secure login (Argon2 password hashing)
- Session-based auth (Bearer token, hashed in DB)
- Role-based access (admin / doctor / receptionist / manager)
- SQL migrations (schema versioned)
- Dev-safe reset workflow
- Single-tenant DB design

What’s NOT done yet:
- Patients
- Appointments
- Billing
- Payments

Those come next, built on top of this foundation.

---

## ⚙️ Requirements

You need:
- Rust (stable)
- PostgreSQL (local)
- `sqlx-cli`

Install sqlx-cli (one time):
```bash
cargo install sqlx-cli --no-default-features --features postgres
