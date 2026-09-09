-- Enforce the documented 7-30 day retention window (CLAUDE.md §6) at the DB level.
-- Clamp any existing out-of-range values first so the CHECK can be added without failing.
UPDATE users SET bin_retention_days = 30 WHERE bin_retention_days < 7 OR bin_retention_days > 30;

ALTER TABLE users ADD CONSTRAINT users_bin_retention_days_range CHECK (bin_retention_days BETWEEN 7 AND 30);
