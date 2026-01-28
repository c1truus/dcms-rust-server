-- 007_patient_register_number.sql

-- Create sequence if not exists
CREATE SEQUENCE IF NOT EXISTS patient_register_seq
    START 1
    INCREMENT 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

-- Ensure column exists
ALTER TABLE patient
    ADD COLUMN IF NOT EXISTS register_number text;

-- Set DEFAULT to auto-generate register_number
ALTER TABLE patient
    ALTER COLUMN register_number
    SET DEFAULT (
        'P' || LPAD(nextval('patient_register_seq')::text, 6, '0')
    );

-- Enforce constraints
ALTER TABLE patient
    ALTER COLUMN register_number SET NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS patient_register_number_key
    ON patient(register_number);
