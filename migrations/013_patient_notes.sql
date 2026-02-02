-- 013_patient_notes.sql
BEGIN;

CREATE TABLE IF NOT EXISTS patient_note (
  patient_note_id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),

  patient_id              UUID NOT NULL REFERENCES patient(patient_id) ON DELETE CASCADE,
  author_employee_id      UUID NOT NULL REFERENCES employee(employee_id) ON DELETE RESTRICT,

  note_type               TEXT NOT NULL, -- 'RECEPTION','CLINICAL','SYSTEM'
  note_text               TEXT NOT NULL,

  is_pinned               BOOLEAN NOT NULL DEFAULT false,

  created_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS patient_note_patient_created_idx
  ON patient_note(patient_id, created_at DESC);

CREATE INDEX IF NOT EXISTS patient_note_type_idx
  ON patient_note(note_type);

COMMIT;
