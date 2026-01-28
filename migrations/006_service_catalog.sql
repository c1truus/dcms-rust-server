-- migrations/006_service_catalog.sql
-- Adds service_catalog table

CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE IF NOT EXISTS service_catalog (
  service_id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  service_type            TEXT NOT NULL UNIQUE,     -- CHILD, ORTHO, ...
  display_number          INT  NOT NULL,            -- UI ordering
  display_name            TEXT NOT NULL,

  default_duration_min    INT  NULL,                -- minutes (NULL = unspecified)
  disclaimer              TEXT NULL,

  price_cents             INT  NOT NULL DEFAULT 0,  -- store as cents/minor units
  is_active               BOOLEAN NOT NULL DEFAULT true,

  created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS service_catalog_is_active_idx ON service_catalog(is_active);
CREATE INDEX IF NOT EXISTS service_catalog_service_type_idx ON service_catalog(service_type);
CREATE INDEX IF NOT EXISTS service_catalog_display_number_idx ON service_catalog(display_number);
