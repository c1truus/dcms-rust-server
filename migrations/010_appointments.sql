-- 010_appointments.sql
-- Creates appointment scheduling + planning items (NOT treatment)

BEGIN;

-- Needed for exclusion constraint
CREATE EXTENSION IF NOT EXISTS btree_gist;

-- -------------------------------------------------------------------
-- appointment (schedule container)
-- -------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS appointment (
  appointment_id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),

  patient_id                UUID NOT NULL REFERENCES patient(patient_id) ON DELETE RESTRICT,

  -- Use employee_id for staff scheduling (employees can link to dcms_user optionally)
  doctor_employee_id        UUID NOT NULL REFERENCES employee(employee_id) ON DELETE RESTRICT,
  receptionist_employee_id  UUID NULL     REFERENCES employee(employee_id) ON DELETE SET NULL,
  assistant_employee_id     UUID NULL     REFERENCES employee(employee_id) ON DELETE SET NULL,

  -- schedule
  start_at                  TIMESTAMPTZ NOT NULL,
  end_at                    TIMESTAMPTZ NOT NULL,

  -- status: 0 reserved, 1 canceled, 2 confirmed, 3 no-show, 4 came, 5 finished
  status                    SMALLINT NOT NULL CHECK (status IN (0,1,2,3,4,5)),

  is_new_patient            BOOLEAN NOT NULL DEFAULT FALSE,
  priority                  SMALLINT NOT NULL DEFAULT 0 CHECK (priority IN (0,1)), -- 0 normal, 1 ASAP (future)
  note                      TEXT NULL,

  arrived_at                TIMESTAMPTZ NULL,
  seated_at                 TIMESTAMPTZ NULL,
  dismissed_at              TIMESTAMPTZ NULL,

  color_override            INT NULL,

  created_at                TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at                TIMESTAMPTZ NOT NULL DEFAULT now(),

  created_by_user_id        UUID NULL REFERENCES dcms_user(user_id) ON DELETE SET NULL,
  updated_by_user_id        UUID NULL REFERENCES dcms_user(user_id) ON DELETE SET NULL,

  -- Keep for backward compatibility with your PDF concept (single planned service)
  planned_service_id        UUID NULL REFERENCES service_catalog(service_id) ON DELETE SET NULL,

  -- basic sanity checks
  CONSTRAINT appointment_time_ok CHECK (end_at > start_at)
);

-- indexes for week view queries
CREATE INDEX IF NOT EXISTS appointment_doctor_start_idx
  ON appointment(doctor_employee_id, start_at);

CREATE INDEX IF NOT EXISTS appointment_patient_start_idx
  ON appointment(patient_id, start_at);

CREATE INDEX IF NOT EXISTS appointment_start_at_idx
  ON appointment(start_at);

-- -------------------------------------------------------------------
-- Overlap prevention per doctor (excluding canceled/no-show)
-- This is the single most important "nasty bug" prevention.
-- -------------------------------------------------------------------
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname = 'appointment_no_overlap_doctor'
  ) THEN
    ALTER TABLE appointment
      ADD CONSTRAINT appointment_no_overlap_doctor
      EXCLUDE USING gist (
        doctor_employee_id WITH =,
        tstzrange(start_at, end_at, '[)') WITH &&
      )
      WHERE (status NOT IN (1,3));
  END IF;
END $$;

-- -------------------------------------------------------------------
-- appointment_plan_item (planned procedures/services for schedule)
-- This is NOT treatment_item; it is pre-visit planning.
-- -------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS appointment_plan_item (
  appointment_plan_item_id  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  appointment_id            UUID NOT NULL REFERENCES appointment(appointment_id) ON DELETE CASCADE,
  service_id                UUID NOT NULL REFERENCES service_catalog(service_id) ON DELETE RESTRICT,

  qty                       INT NOT NULL DEFAULT 1 CHECK (qty > 0),
  note                      TEXT NULL,

  created_at                TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS appointment_plan_item_appointment_idx
  ON appointment_plan_item(appointment_id);

CREATE INDEX IF NOT EXISTS appointment_plan_item_service_idx
  ON appointment_plan_item(service_id);

COMMIT;
