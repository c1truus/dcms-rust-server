-- migrations/001_core.sql
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- Single-tenant clinic settings (optional but useful)
CREATE TABLE IF NOT EXISTS clinic_settings (
  singleton_id BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton_id),
  clinic_name  TEXT NOT NULL,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- dcms_user table
-- roles:
-- 0 patient, 1 admin, 2 manager, 3 doctor, 4 receptionist
CREATE TABLE IF NOT EXISTS "dcms_user" (
  user_id       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  username      TEXT NOT NULL UNIQUE,
  display_name  TEXT NOT NULL,
  password_hash TEXT NOT NULL,            -- Argon2 hash
  roles         SMALLINT NOT NULL CHECK (roles IN (0,1,2,3,4)),
  is_active     BOOLEAN NOT NULL DEFAULT TRUE,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS user_roles_idx ON "dcms_user"(roles);
CREATE INDEX IF NOT EXISTS user_is_active_idx ON "dcms_user"(is_active);