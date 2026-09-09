-- Make subscription_history idempotent on webhook_id so re-processing a webhook
-- (after crash or recovery) cannot insert duplicate history entries.
--
-- A partial unique index is used (WHERE external_event_id IS NOT NULL) so that
-- manual admin changes with no associated webhook_id can still be recorded freely.
CREATE UNIQUE INDEX IF NOT EXISTS subscription_history_event_unique
    ON subscription_history (subscription_id, external_event_id)
    WHERE external_event_id IS NOT NULL;
