-- 014_appointment_source.sql
BEGIN;

ALTER TABLE appointment
  ADD COLUMN IF NOT EXISTS source TEXT NOT NULL DEFAULT 'SCHEDULED';
-- 'SCHEDULED' | 'WALKIN' | 'WAITLIST'

CREATE INDEX IF NOT EXISTS appointment_source_idx
  ON appointment(source);

COMMIT;

BEGIN;

ALTER TABLE appointment
  ADD COLUMN IF NOT EXISTS reminder_sent_at TIMESTAMPTZ NULL,
  ADD COLUMN IF NOT EXISTS confirmed_at TIMESTAMPTZ NULL;

COMMIT;
