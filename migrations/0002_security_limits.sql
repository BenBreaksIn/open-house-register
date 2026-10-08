-- Invalidate old sessions and remove the fast password verifier from live storage.
DELETE FROM sessions;
ALTER TABLE sessions RENAME COLUMN password_fingerprint TO session_generation;
DELETE FROM rate_limits;
CREATE INDEX rate_limits_expiry ON rate_limits(window_start);
CREATE INDEX visitors_export_cursor ON visitors(house_id, checked_in_at DESC, id DESC);
