-- migrations/005_phone_number_sms.sql
-- Adds phone_number + sms tables (snake_case, UUID PKs, timestamptz)

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- -----------------------
-- phone_number
-- -----------------------
CREATE TABLE IF NOT EXISTS phone_number (
  phone_number_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  patient_id      UUID NOT NULL REFERENCES patient(patient_id) ON DELETE CASCADE,

  phone_number    TEXT NOT NULL,          -- the actual number (E.164 or local)
  label           TEXT NOT NULL,          -- Self / Father / Mother / ...
  is_primary      BOOLEAN NOT NULL DEFAULT false,

  created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),

  CONSTRAINT phone_number_patient_number_unique UNIQUE (patient_id, phone_number)
);

-- Only one primary phone number per patient
CREATE UNIQUE INDEX IF NOT EXISTS phone_number_one_primary_idx
  ON phone_number(patient_id)
  WHERE is_primary = true;

CREATE INDEX IF NOT EXISTS phone_number_patient_id_idx ON phone_number(patient_id);


-- -----------------------
-- sms
-- -----------------------
CREATE TABLE IF NOT EXISTS sms (
  sms_id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  phone_number_id UUID NOT NULL REFERENCES phone_number(phone_number_id) ON DELETE CASCADE,

  direction       SMALLINT NOT NULL,        -- 0=in, 1=out
  sent_at         TIMESTAMPTZ NOT NULL,      -- renamed from "time"

  subject         TEXT NULL,
  sms_text        TEXT NOT NULL,
  note            TEXT NULL,

  created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),

  CONSTRAINT sms_direction_check CHECK (direction IN (0, 1))
);

CREATE INDEX IF NOT EXISTS sms_phone_number_id_idx ON sms(phone_number_id);
CREATE INDEX IF NOT EXISTS sms_sent_at_idx ON sms(sent_at);
