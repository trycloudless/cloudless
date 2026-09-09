-- The original UNIQUE (physical_device_id, user_id) constraint applies even when
-- physical_device_id is the empty string (the sentinel used for all mobile devices,
-- which have no stable machine UID). That silently capped every user to exactly one
-- mobile device row total, defeating the (user_id, platform, display_name) matching
-- that find_matching_device relies on to keep distinct mobile devices separate.
--
-- Desktop devices (non-empty physical_device_id) still need exact-match uniqueness
-- per user. Replace the blanket constraint with a partial unique index that only
-- applies to non-empty physical_device_id, leaving mobile rows unconstrained here
-- (mobile identity is enforced at the application layer via find_matching_device).
ALTER TABLE local_devices DROP CONSTRAINT local_devices_physical_device_id_user_id_key;

CREATE UNIQUE INDEX local_devices_physical_device_id_user_id_key
    ON local_devices (physical_device_id, user_id)
    WHERE physical_device_id <> '';
