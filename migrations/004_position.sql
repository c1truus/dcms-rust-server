-- migrations/004_position.sql

-- category:
-- 0 clinical, 1 support, 2 admin/frontdesk/finance
CREATE TABLE IF NOT EXISTS position (
  position_id    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  position_type  TEXT NOT NULL UNIQUE,           -- stable key: DOCTOR, MANAGER, ...
  display_name   TEXT NOT NULL,                  -- label: General Dentist, Orthodontist, ...
  category       SMALLINT NOT NULL CHECK (category IN (0,1,2)),
  is_active      BOOLEAN NOT NULL DEFAULT TRUE,
  description    TEXT NULL
);

CREATE INDEX IF NOT EXISTS position_category_idx ON position(category);
CREATE INDEX IF NOT EXISTS position_is_active_idx ON position(is_active);

-- employee_position link
CREATE TABLE IF NOT EXISTS employee_position (
  employee_position_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  employee_id          UUID NOT NULL REFERENCES employee(employee_id) ON DELETE CASCADE,
  position_id          UUID NOT NULL REFERENCES position(position_id) ON DELETE RESTRICT,

  is_primary           BOOLEAN NOT NULL DEFAULT FALSE,
  started_at           DATE NULL,
  ended_at             DATE NULL,

  CONSTRAINT employee_position_unique UNIQUE (employee_id, position_id)
);

CREATE INDEX IF NOT EXISTS employee_position_employee_id_idx ON employee_position(employee_id);
CREATE INDEX IF NOT EXISTS employee_position_position_id_idx ON employee_position(position_id);

-- Optional: only one primary position per employee
-- (partial unique index)
CREATE UNIQUE INDEX IF NOT EXISTS employee_one_primary_position_idx
ON employee_position(employee_id)
WHERE is_primary = TRUE;
