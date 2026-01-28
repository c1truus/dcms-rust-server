-- migrations/003_session_token.sql

-- session_type:
-- 0 undefined, 1 dcms_user portal, 2 mobile patient web, 3 dcmshq service
CREATE TABLE IF NOT EXISTS session_token (
  session_token_id   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id            UUID NOT NULL REFERENCES "dcms_user"(user_id) ON DELETE CASCADE,

  session_token_hash TEXT NOT NULL UNIQUE,  -- SHA-256 hex/base64 string

  session_type       SMALLINT NOT NULL CHECK (session_type IN (0,1,2,3)),
  device_name        TEXT NULL,

  expires_at         TIMESTAMPTZ NOT NULL,
  created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_seen_at       TIMESTAMPTZ NULL,
  revoked_at         TIMESTAMPTZ NULL
);

CREATE INDEX IF NOT EXISTS session_token_user_id_idx ON session_token(user_id);
CREATE INDEX IF NOT EXISTS session_token_expires_at_idx ON session_token(expires_at);
CREATE INDEX IF NOT EXISTS session_token_revoked_at_idx ON session_token(revoked_at);
CREATE INDEX IF NOT EXISTS session_token_last_seen_at_idx ON session_token(last_seen_at);
CREATE INDEX IF NOT EXISTS session_token_hash_idx ON session_token(session_token_hash);

CREATE INDEX IF NOT EXISTS session_token_active_idx
  ON session_token(session_token_hash)
  WHERE revoked_at IS NULL;
