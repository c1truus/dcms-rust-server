-- migrations/009_clinic_settings_v2.sql

ALTER TABLE clinic_settings
  ADD COLUMN IF NOT EXISTS timezone TEXT NOT NULL DEFAULT 'UTC',
  ADD COLUMN IF NOT EXISTS default_slot_minutes INT NOT NULL DEFAULT 30,
  ADD COLUMN IF NOT EXISTS business_hours JSONB NOT NULL DEFAULT '{}'::jsonb,
  ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  ADD COLUMN IF NOT EXISTS updated_by_user_id UUID NULL;

-- Optional FK (recommended)
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM pg_constraint
    WHERE conname = 'clinic_settings_updated_by_user_fk'
  ) THEN
    ALTER TABLE clinic_settings
      ADD CONSTRAINT clinic_settings_updated_by_user_fk
      FOREIGN KEY (updated_by_user_id) REFERENCES dcms_user(user_id)
      ON DELETE SET NULL;
  END IF;
END $$;

-- Ensure singleton row exists (safe in dev)
INSERT INTO clinic_settings (singleton_id, clinic_name)
VALUES (TRUE, 'Demo Clinic')
ON CONFLICT (singleton_id) DO NOTHING;
