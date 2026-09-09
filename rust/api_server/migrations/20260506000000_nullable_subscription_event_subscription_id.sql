-- subscription_events.subscription_id can be NULL for provider webhook events
-- (e.g. entitlement_grant.delivered) that have no matching subscription row yet.
-- NULL is allowed by PostgreSQL FK constraints; the FK itself is kept for rows that do link.
ALTER TABLE subscription_events ALTER COLUMN subscription_id DROP NOT NULL;
