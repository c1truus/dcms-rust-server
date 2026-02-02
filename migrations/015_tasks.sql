-- migrations/015_tasks.sql
BEGIN;

-- ------------------------------------------------------------
-- Extend task table (from 011_tasks.sql)
-- ------------------------------------------------------------

ALTER TABLE task
  ADD COLUMN IF NOT EXISTS started_at TIMESTAMPTZ NULL,
  ADD COLUMN IF NOT EXISTS canceled_at TIMESTAMPTZ NULL,
  ADD COLUMN IF NOT EXISTS updated_by_employee_id UUID NULL REFERENCES employee(employee_id) ON DELETE SET NULL;

-- Make sure updated_at is always refreshed
CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS trigger AS $$
BEGIN
  NEW.updated_at = now();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_trigger WHERE tgname = 'task_set_updated_at_trg'
  ) THEN
    CREATE TRIGGER task_set_updated_at_trg
    BEFORE UPDATE ON task
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();
  END IF;
END $$;

-- Helpful indexes for common Phase 1 queries
CREATE INDEX IF NOT EXISTS task_created_by_idx
  ON task(created_by_employee_id, created_at DESC);

CREATE INDEX IF NOT EXISTS task_assignee_due_idx
  ON task(assigned_to_employee_id, status, due_at);

CREATE INDEX IF NOT EXISTS task_status_created_idx
  ON task(status, created_at DESC);

CREATE INDEX IF NOT EXISTS task_appointment_idx
  ON task(appointment_id);

COMMIT;
