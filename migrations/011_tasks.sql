-- 011_tasks.sql
BEGIN;

CREATE TABLE IF NOT EXISTS task (
  task_id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),

  -- who created it (doctor/reception/manager)
  created_by_employee_id  UUID NOT NULL REFERENCES employee(employee_id) ON DELETE RESTRICT,

  -- who should handle it (usually reception); can be null = unassigned queue
  assigned_to_employee_id UUID NULL REFERENCES employee(employee_id) ON DELETE SET NULL,

  -- optional linkage
  patient_id              UUID NULL REFERENCES patient(patient_id) ON DELETE CASCADE,
  appointment_id          UUID NULL REFERENCES appointment(appointment_id) ON DELETE SET NULL,

  -- type + workflow
  task_type               TEXT NOT NULL,  -- 'CALL_PATIENT','SMS_PATIENT','SCHEDULE_FOLLOWUP','CALL_STAFF',...
  status                  SMALLINT NOT NULL DEFAULT 0 CHECK (status IN (0,1,2,3)),
  -- 0 open, 1 in_progress, 2 done, 3 canceled

  priority                SMALLINT NOT NULL DEFAULT 0 CHECK (priority IN (0,1,2)),
  -- 0 normal, 1 high, 2 urgent

  due_at                  TIMESTAMPTZ NULL,
  title                   TEXT NOT NULL,
  details                 TEXT NULL,

  created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
  completed_at            TIMESTAMPTZ NULL
);

CREATE INDEX IF NOT EXISTS task_status_due_idx
  ON task(status, due_at);

CREATE INDEX IF NOT EXISTS task_assignee_status_idx
  ON task(assigned_to_employee_id, status);

CREATE INDEX IF NOT EXISTS task_patient_idx
  ON task(patient_id);

COMMIT;
