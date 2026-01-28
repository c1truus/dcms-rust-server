-- migrations/008_session_impersonation.sql
-- Adds optional metadata for admin impersonation sessions.
--
-- Minimal approach: extra nullable columns on session_token.

ALTER TABLE session_token
  ADD COLUMN IF NOT EXISTS impersonator_user_id UUID NULL REFERENCES "dcms_user"(user_id) ON DELETE SET NULL,
  ADD COLUMN IF NOT EXISTS impersonated_user_id UUID NULL REFERENCES "dcms_user"(user_id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS session_token_impersonator_user_id_idx ON session_token(impersonator_user_id);
CREATE INDEX IF NOT EXISTS session_token_impersonated_user_id_idx ON session_token(impersonated_user_id);
