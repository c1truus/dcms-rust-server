-- migrations/002_people.sql

-- employee
CREATE TABLE IF NOT EXISTS employee (
  employee_id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  employee_display_number BIGINT NOT NULL UNIQUE, -- ordered, never reused
  user_id                 UUID NULL REFERENCES "dcms_user"(user_id) ON DELETE SET NULL,

  first_name              TEXT NOT NULL,
  last_name               TEXT NOT NULL,

  prim_phone_number       TEXT NULL,
  seco_phone_number       TEXT NULL,

  gender                  SMALLINT NOT NULL CHECK (gender IN (0,1,2)),
  status                  SMALLINT NOT NULL CHECK (status IN (0,1,2,3,4)),

  birthday                DATE NULL,
  email                   TEXT NULL,
  home_address            TEXT NULL,

  hired_at                DATE NULL,
  fired_at                DATE NULL,

  created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS employee_user_id_idx ON employee(user_id);

-- patient
CREATE TABLE IF NOT EXISTS patient (
  patient_id      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id         UUID NULL REFERENCES "dcms_user"(user_id) ON DELETE SET NULL,

  register_number TEXT NOT NULL UNIQUE,

  first_name      TEXT NOT NULL,
  last_name       TEXT NOT NULL,
  email           TEXT NULL,

  birthday        DATE NULL,
  gender          SMALLINT NOT NULL CHECK (gender IN (0,1,2)),

  status          SMALLINT NOT NULL CHECK (status IN (0,1,2,3)), -- per your spec

  created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_seen_at    TIMESTAMPTZ NULL
);

CREATE INDEX IF NOT EXISTS patient_user_id_idx ON patient(user_id);
CREATE INDEX IF NOT EXISTS patient_last_seen_at_idx ON patient(last_seen_at);
