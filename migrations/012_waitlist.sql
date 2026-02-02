-- 012_waitlist.sql
BEGIN;

CREATE TABLE IF NOT EXISTS waitlist_entry (
  waitlist_entry_id       UUID PRIMARY KEY DEFAULT gen_random_uuid(),

  patient_id              UUID NOT NULL REFERENCES patient(patient_id) ON DELETE CASCADE,
  requested_doctor_employee_id UUID NULL REFERENCES employee(employee_id) ON DELETE SET NULL,

  -- request context
  reason                  TEXT NULL,
  notes                   TEXT NULL,

  -- preference (simple, Phase 1)
  preferred_date          DATE NULL,
  preferred_time_start    TIME NULL,
  preferred_time_end      TIME NULL,

  priority                SMALLINT NOT NULL DEFAULT 0 CHECK (priority IN (0,1,2)),
  status                  SMALLINT NOT NULL DEFAULT 0 CHECK (status IN (0,1,2,3)),
  -- 0 waiting, 1 contacted, 2 scheduled, 3 removed

  created_by_employee_id  UUID NOT NULL REFERENCES employee(employee_id) ON DELETE RESTRICT,
  created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),

  -- once scheduled, link to the appointment
  scheduled_appointment_id UUID NULL REFERENCES appointment(appointment_id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS waitlist_status_priority_idx
  ON waitlist_entry(status, priority, created_at);

CREATE INDEX IF NOT EXISTS waitlist_patient_idx
  ON waitlist_entry(patient_id);

COMMIT;
