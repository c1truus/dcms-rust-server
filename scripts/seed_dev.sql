-- scripts/seed_dev.sql (DEV ONLY) - RICH SEED
-- Assumes migrations already ran.

\set ON_ERROR_STOP on

-- ------------------------------------------------------------
-- Clinic settings
-- ------------------------------------------------------------
INSERT INTO clinic_settings (singleton_id, clinic_name, timezone, default_slot_minutes, business_hours)
VALUES (TRUE, 'Demo Clinic', 'Asia/Ulaanbaatar', 30, '{}'::jsonb)
ON CONFLICT (singleton_id) DO UPDATE
SET clinic_name = EXCLUDED.clinic_name;

-- ------------------------------------------------------------
-- Common password for seeded accounts: admin123
-- (Argon2 hash must match your backend verify config)
-- ------------------------------------------------------------
DO $$ BEGIN NULL; END $$;

-- Use the SAME hash for all seeded accounts for convenience.
-- This is the hash already in your current seed file.
\set DEV_HASH '''$argon2id$v=19$m=65536,t=3,p=4$vl6eGKpf/3SdpXkQ1xYbnQ$8Fx8GG1GiIiqjH881zBdOBVnmpjpgqm8KVz1X5sUIAE'''

-- ------------------------------------------------------------
-- Users: 2 admins, 2 managers, 5 doctors, 2 receptionists
-- roles: 1 admin, 2 manager, 3 doctor, 4 receptionist
-- ------------------------------------------------------------
INSERT INTO dcms_user (username, display_name, password_hash, roles, is_active)
VALUES
  ('admin',   'Administrator', :DEV_HASH, 1, TRUE),
  ('admin2',  'Admin Two',     :DEV_HASH, 1, TRUE),
  ('mgr1',    'Manager A',     :DEV_HASH, 2, TRUE),
  ('mgr2',    'Manager B',     :DEV_HASH, 2, TRUE),
  ('recept1', 'Reception 1',   :DEV_HASH, 4, TRUE),
  ('recept2', 'Reception 2',   :DEV_HASH, 4, TRUE)
ON CONFLICT (username) DO UPDATE
SET display_name  = EXCLUDED.display_name,
    password_hash = EXCLUDED.password_hash,
    roles         = EXCLUDED.roles,
    is_active     = EXCLUDED.is_active;

-- doctors (5)
INSERT INTO dcms_user (username, display_name, password_hash, roles, is_active)
SELECT
  'doctor' || i,
  'Dr. ' || chr(64+i),            -- Dr. A..E
  :DEV_HASH,
  3,
  TRUE
FROM generate_series(1,5) i
ON CONFLICT (username) DO NOTHING;

-- ------------------------------------------------------------
-- Employees:
-- - create employee rows for all staff users (admins/managers/doctors/receptionists)
-- - create assistants as employees WITHOUT user accounts (no login)
-- ------------------------------------------------------------

-- helper: create employee row for a specific user (if not exists by display_number)
-- We'll use deterministic display numbers.
WITH u AS (
  SELECT user_id, username, display_name, roles
  FROM dcms_user
  WHERE username IN ('admin','admin2','mgr1','mgr2','recept1','recept2')
     OR username LIKE 'doctor%'
)
INSERT INTO employee (
  employee_display_number, user_id,
  first_name, last_name,
  prim_phone_number, seco_phone_number,
  gender, status, hired_at
)
SELECT
  2000000 + row_number() OVER (ORDER BY username),
  user_id,
  split_part(display_name,' ',1),
  split_part(display_name,' ',2),
  NULL, NULL,
  0, 1, CURRENT_DATE
FROM u
ON CONFLICT (employee_display_number) DO UPDATE
SET user_id = EXCLUDED.user_id;

-- Assistants (6) - no user_id
INSERT INTO employee (
  employee_display_number, user_id,
  first_name, last_name,
  prim_phone_number, seco_phone_number,
  gender, status, hired_at
)
SELECT
  3000000 + i,
  NULL,
  'Assistant',
  i::text,
  NULL, NULL,
  0, 1, CURRENT_DATE
FROM generate_series(1,6) i
ON CONFLICT (employee_display_number) DO NOTHING;

-- ------------------------------------------------------------
-- Services (ensure at least a few exist)
-- ------------------------------------------------------------
INSERT INTO service_catalog (service_type, display_number, display_name, default_duration_min, price_cents)
VALUES
  ('CONSULT',  1, 'Consultation', 30, 2000),
  ('CLEAN',    2, 'Cleaning',     30, 8000),
  ('FILL',     3, 'Filling',      45, 15000),
  ('RCT',      4, 'Root Canal',   60, 40000),
  ('XRAY',     5, 'X-Ray',        15, 5000)
ON CONFLICT (service_type) DO NOTHING;

-- ------------------------------------------------------------
-- Patients (100)
-- register_number has default from your seq migration, so we can omit it.
-- ------------------------------------------------------------
INSERT INTO patient (first_name, last_name, email, birthday, gender, status, created_at, last_seen_at)
SELECT
  'Patient' || i,
  'Demo',
  NULL,
  date '1980-01-01' + (i * interval '35 days'),
  CASE WHEN i % 3 = 0 THEN 1 WHEN i % 3 = 1 THEN 0 ELSE 2 END,
  1,
  now() - (random() * interval '120 days'),
  NULL
FROM generate_series(1,100) i;

-- ------------------------------------------------------------
-- Make appointment seeding idempotent:
-- Remove the "next 8 days" window and reseed it.
-- This prevents exclusion constraint conflicts when running make seed repeatedly.
-- ------------------------------------------------------------
DELETE FROM appointment
WHERE start_at >= date_trunc('day', now())
  AND start_at <  date_trunc('day', now()) + interval '8 days';

-- ------------------------------------------------------------
-- Appointments (Week schedule): 7 days × 5 doctors × 2 slots = 70
-- Slots: 09:00 and 10:00 (30 min each) — no overlaps by construction.
-- Mix statuses.
-- ------------------------------------------------------------
WITH doctors AS (
  SELECT e.employee_id, row_number() OVER (ORDER BY e.employee_display_number) AS rn
  FROM employee e
  JOIN dcms_user u ON u.user_id = e.user_id
  WHERE u.roles = 3
  ORDER BY e.employee_display_number
),
receptionists AS (
  SELECT e.employee_id
  FROM employee e
  JOIN dcms_user u ON u.user_id = e.user_id
  WHERE u.roles = 4
  ORDER BY e.employee_display_number
),
assistants AS (
  SELECT employee_id
  FROM employee
  WHERE user_id IS NULL
  ORDER BY employee_display_number
),
patients AS (
  SELECT patient_id, row_number() OVER (ORDER BY created_at) AS rn
  FROM patient
),
grid AS (
  SELECT
    d.employee_id AS doctor_employee_id,
    (SELECT employee_id FROM receptionists OFFSET ((d.rn-1) % 2) LIMIT 1) AS receptionist_employee_id,
    (SELECT employee_id FROM assistants OFFSET ((d.rn-1) % 6) LIMIT 1) AS assistant_employee_id,
    (SELECT patient_id FROM patients OFFSET ((d.rn*10 + day*2 + slot) % 100) LIMIT 1) AS patient_id,
    (date_trunc('day', now()) + (day || ' days')::interval + (CASE WHEN slot=0 THEN interval '9 hours' ELSE interval '10 hours' END)) AS start_at,
    (date_trunc('day', now()) + (day || ' days')::interval + (CASE WHEN slot=0 THEN interval '9 hours 30 minutes' ELSE interval '10 hours 30 minutes' END)) AS end_at,
    day,
    slot,
    d.rn AS doctor_rn
  FROM doctors d
  CROSS JOIN generate_series(0,6) AS day
  CROSS JOIN generate_series(0,1) AS slot
)
INSERT INTO appointment (
  patient_id,
  doctor_employee_id,
  receptionist_employee_id,
  assistant_employee_id,
  start_at,
  end_at,
  status,
  is_new_patient,
  priority,
  note,
  created_by_user_id,
  updated_by_user_id
)

SELECT
  patient_id,
  doctor_employee_id,
  receptionist_employee_id,
  assistant_employee_id,
  start_at,
  end_at,
  CASE
    WHEN (day=1 AND slot=1 AND doctor_rn=2) THEN 1  -- canceled example
    WHEN (day=2 AND slot=0 AND doctor_rn=4) THEN 3  -- no-show example
    WHEN (day=0 AND slot=0 AND doctor_rn IN (1,3)) THEN 5 -- finished examples today morning
    WHEN (day=0 AND slot=1 AND doctor_rn IN (2,4)) THEN 4 -- came examples today 10:00
    ELSE 2                                          -- confirmed default
  END AS status,
  ((day*2 + slot + doctor_rn) % 10 = 0) AS is_new_patient,
  0,
  'Seeded appointment',
  NULL,
  NULL
FROM grid;

-- ------------------------------------------------------------
-- Planned items: 1–3 services per appointment
-- Use a temp table so we can reuse the same appointment set
-- across multiple INSERT statements.
-- ------------------------------------------------------------

DROP TABLE IF EXISTS _seed_apts;
CREATE TEMP TABLE _seed_apts AS
SELECT appointment_id
FROM appointment
WHERE start_at >= date_trunc('day', now())
  AND start_at <  date_trunc('day', now()) + interval '8 days';

DROP TABLE IF EXISTS _seed_svcs;
CREATE TEMP TABLE _seed_svcs AS
SELECT service_id
FROM service_catalog
WHERE is_active = TRUE
ORDER BY display_number;

-- 1 item always
INSERT INTO appointment_plan_item (appointment_id, service_id, qty, note)
SELECT
  a.appointment_id,
  (SELECT service_id FROM _seed_svcs OFFSET (floor(random() * (SELECT count(*) FROM _seed_svcs))) LIMIT 1),
  1,
  NULL
FROM _seed_apts a;

-- 2nd item for ~50%
INSERT INTO appointment_plan_item (appointment_id, service_id, qty, note)
SELECT
  a.appointment_id,
  (SELECT service_id FROM _seed_svcs OFFSET (floor(random() * (SELECT count(*) FROM _seed_svcs))) LIMIT 1),
  1,
  NULL
FROM _seed_apts a
WHERE random() < 0.5;

-- 3rd item for ~20%
INSERT INTO appointment_plan_item (appointment_id, service_id, qty, note)
SELECT
  a.appointment_id,
  (SELECT service_id FROM _seed_svcs OFFSET (floor(random() * (SELECT count(*) FROM _seed_svcs))) LIMIT 1),
  1,
  NULL
FROM _seed_apts a
WHERE random() < 0.2;

-- ------------------------------------------------------------
-- Status timestamps for realism
-- ------------------------------------------------------------
UPDATE appointment
SET arrived_at = start_at + interval '5 minutes'
WHERE status >= 4 AND arrived_at IS NULL;

UPDATE appointment
SET dismissed_at = end_at
WHERE status = 5 AND dismissed_at IS NULL;
