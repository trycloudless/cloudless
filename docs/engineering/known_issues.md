# Known Issues

Tracked issues that are acknowledged but not yet fixed.

## Stale S3 Chunk Detection and Re-upload

**File:** `rust/core/src/applications/backup/backup_file.rs`
**Severity:** Low (handled automatically)

When a chunk's SHA-256 hash exists in S3 but not in the DB (e.g. DB was reset while S3
retained old objects), the backup detects this via `CreateChunkResponse.existed_in_db` and
automatically re-encrypts, re-uploads the chunk, then updates the DB row's `storage_meta`
via `update_storage_meta`. This ensures the nonce in the DB always matches the ciphertext
in storage.

Normal dedup (chunk exists in both S3 and DB) skips the upload entirely, preserving
bandwidth savings.

## DEK Rotation and Deduplication

**File:** `rust/core/src/applications/backup/backup_file.rs`
**Severity:** Medium

If the DEK is rotated between backups, deduplicated chunks will be re-encrypted with the new
DEK (since we always encrypt now). However, the DB's `ON CONFLICT DO NOTHING` preserves the
original chunk row, which still references the old DEK's nonce. Restore would use the old
nonce against ciphertext encrypted with the new DEK, causing decryption failure.

**Workaround:** Do not rotate the DEK while deduplicated chunks from the old DEK still exist
in storage. A full re-backup after DEK rotation avoids this issue.

**Proper fix:** Change `ON CONFLICT DO NOTHING` to `ON CONFLICT DO UPDATE SET storage_meta = ...`
so the encryption metadata is always updated to match the latest upload.

---

## Not Yet Covered by E2E Integration Tests

The following scenarios are acknowledged gaps in the end-to-end test suite.

### Google Drive Storage

**Reason:** Requires OAuth browser flow; cannot be automated in a headless test. S3 covers
the storage adapter code path. Google Drive uses the same `StoragePort` trait.

### Concurrent Backup Jobs

**Reason:** Out of scope for initial E2E tests. Sequential correctness is validated first.
Concurrent access may surface race conditions in job creation/resumption.

### DEK Rotation During Active Backup

**Reason:** Known issue (see deduplication bug above). Testing this would confirm the bug
but not add value until the fix is implemented.

### Token Expiry and Auto-Refresh

**Reason:** E2E tests run fast enough that the JWT access token (120s TTL) never expires.
A dedicated test with shortened TTL would be needed.

### Network Failure Recovery

**Reason:** Requires fault injection (e.g., toxiproxy) to simulate dropped connections
mid-upload. Out of scope for the current integration test harness.

### Very Large Files (>100 MB)

**Reason:** Slow to upload/download in CI. The 1 MB test file is sufficient to validate
multi-chunk backup and restore logic (4 MiB chunk size means even 1 MB is single-chunk,
but the test includes a file large enough to trigger multiple chunks).
