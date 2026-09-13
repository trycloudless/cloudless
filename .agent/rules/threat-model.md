---
trigger: always_on
---

# threat-model.md
## Threat Model – CloudLess Backup Engine

This document defines trust boundaries, threats, and mitigations for the CloudLess backup system. All code changes must preserve these security properties.

> **Status note:** this document describes the security design as implemented. If you find code that diverges from a mitigation described here, treat it as a bug and report it via [`SECURITY.md`](../../SECURITY.md).

---

## TRUST BOUNDARIES

| Boundary | Trust Level | Notes |
|----------|------------|-------|
| **Device (client)** | Untrusted but authenticated | Runs encryption/decryption, holds DEK in memory |
| **API Server (control plane)** | Trusted, must be hardened | Stateless, never sees plaintext data |
| **PostgreSQL** | Trusted, single source of truth | Stores all metadata, encrypted DEKs, audit trail |
| **Object Storage (S3/Google Drive)** | Untrusted, eventually consistent | Stores only encrypted chunks, no metadata authority |
| **Dodo Payments (payment provider)** | Untrusted external service | Webhook events verified via HMAC-SHA256 signature; never trust payload without verification |
| **Network** | Untrusted | All communication over TLS, JWT-authenticated |

---

## ENCRYPTION TRUST MODEL

### What the server NEVER sees
- Plaintext file content
- Plaintext file paths (only encrypted paths + blind indexes)
- DEK in plaintext (stored encrypted with KEK, recovery key, or biometric key)
- Content encryption keys (derived client-side via HKDF)

### What the server stores
- Encrypted DEKs (password-encrypted, biometric-encrypted, recovery-key-encrypted)
- Encrypted file paths + blind indexes (HMAC-SHA256 for equality search)
- Chunk metadata (hash, index, storage location, nonce, algorithm)
- User/device/config metadata

### Key hierarchy integrity
- KEK derived via `Argon2id(password, salt)`: password never leaves client
- DEK is random 256-bit, encrypted under KEK
- Content key, metadata key, index key derived from DEK via `HKDF-SHA256` with domain separation
- All sensitive keys wrapped in `Zeroizing<T>` (zeroed on drop)

---

## THREATS & MITIGATIONS

### 1. Chunk deletion or corruption in storage
**Threat**: Attacker or storage failure removes/corrupts encrypted chunks.
**Mitigation**:
- Pre-restore integrity check (HEAD request per chunk): aborts before any writes
- SHA-256 hash verification of plaintext after decrypt+decompress
- Fail loudly: never silently skip missing chunks
- GC re-verifies chunk orphan status before database deletion (two-phase)

### 2. OAuth token compromise
**Threat**: Stolen OAuth refresh token grants access to user's Google Drive or OneDrive storage.
**Mitigation**:
- Refresh tokens encrypted at rest in PostgreSQL
- OAuth scope limited to `drive.file` (Google Drive) or `Files.ReadWrite.AppFolder` (OneDrive) so the app can only access files it created
- Token revocation support: hard-stops storage access immediately
- Short-lived access tokens with automatic refresh on 401

### 3. Replay attacks on API
**Threat**: Attacker replays legitimate API requests to duplicate operations.
**Mitigation**:
- Idempotency keys on file version creation and chunk completion
- Same idempotency key + different payload = `409 Conflict`
- Completed requests replay stored responses (no duplicate side effects)
- Enforced at PostgreSQL level (unique constraints)

### 4. Partial uploads / interrupted backups
**Threat**: Network failure or crash leaves backup in inconsistent state.
**Mitigation**:
- Chunks not registered in DB until upload confirmed
- Backup jobs are resumable from last incomplete file
- No assumption of chunk existence: verification required
- Stale S3 chunks re-encrypted and re-registered during backup

### 5. Backend / API server compromise
**Threat**: Attacker gains access to API server or database.
**Mitigation**:
- No plaintext file data stored server-side (zero-knowledge)
- File paths stored encrypted with per-encryption random nonces
- DEK stored encrypted: attacker cannot decrypt without user's password/recovery key
- Limited OAuth scope prevents broad storage access
- Security events logged for audit trail

### 6. Device compromise
**Threat**: Attacker gains access to a user's device.
**Mitigation**:
- DEK held in memory only during active session (zeroized on drop)
- Biometric unlock backed by OS keychain (Face ID / Touch ID / fingerprint)
- SQLite local index is a rebuildable cache: no secrets stored
- Device isolation: compromising one device doesn't affect others

### 7. Password compromise
**Threat**: User's password is leaked or brute-forced.
**Mitigation**:
- Argon2id with configurable parameters for key derivation (slow by design)
- Password rotation support (re-encrypts DEK under new KEK)
- Recovery key as independent unlock path
- Security events logged on password change

### 8. Cross-device data leakage
**Threat**: Files from one device appear on or are accessible from another.
**Mitigation**:
- Strict device isolation: file identity = `(backup_config_id, blind_index)`
- Same path on different devices = different files (by design)
- Ownership of every device-scoped foreign key (storage, config, file version, chunk) enforced via composite database foreign keys at the point of use
- Cross-device conflicts do not exist architecturally

### 9. Garbage collection race conditions
**Threat**: GC deletes chunks that are still referenced by active backups.
**Mitigation**:
- Three-phase GC: server collect → client delete → server confirm
- Server re-verifies chunks still orphaned before DB deletion
- Configurable retention period (7–30 days, default 30), enforced by a database CHECK constraint
- Audit trail in `gc_runs`, `gc_run_versions`, `gc_run_chunks`

### 10. Man-in-the-middle attacks
**Threat**: Attacker intercepts client-server communication.
**Mitigation**:
- All API communication over HTTPS/TLS
- JWT authentication with access + refresh token pair
- Android: `rustls_platform_verifier` with bundled Mozilla root certs
- Direct client-to-storage uploads (never proxied through API server)

### 11. JWT token theft
**Threat**: Stolen JWT grants unauthorized API access.
**Mitigation**:
- Short-lived access tokens
- Refresh token rotation
- Token revocation capability

### 12. Webhook payload forgery (payment provider)
**Threat**: Attacker forges a Dodo Payments webhook to grant fraudulent subscription upgrades or trigger billing state changes.
**Mitigation**:
- Every incoming webhook verified with HMAC-SHA256 signature before processing (`verify_webhook_signature` uses constant-time comparison)
- Stale timestamps rejected (replay window limited)
- Webhook secret stored server-side only: never exposed to clients
- Subscription state transitions driven by verified webhook events only; no client-controlled upgrade path
- Server-side reconciliation endpoint (admin only) to re-sync state from payment provider if webhook is missed

### 13. Subscription state fraud
**Threat**: User bypasses payment to gain premium subscription entitlements.
**Mitigation**:
- Subscription state stored and enforced server-side in PostgreSQL
- Entitlement checks performed on each protected API call
- Subscription managed exclusively via payment-provider webhook: no user-facing endpoint grants subscription status directly
- Admin reconcile endpoint behind `SuperAdmin` role for manual correction only

---

## SECURITY INVARIANTS (MUST BE PRESERVED)

1. Server never sees plaintext file data or paths
2. DEK never stored in plaintext: always encrypted under KEK, biometric key, or recovery key
3. All sensitive keys `Zeroizing<T>`: zeroed on drop, no accidental leaks
4. Chunk hash computed on plaintext BEFORE encryption (content-addressable dedup)
5. Per-chunk random nonces for IND-CPA security
6. Device namespaces are strictly isolated: no cross-device data mixing
7. Idempotency enforced at database level for all mutating operations
8. System fails loudly on integrity issues: never silently corrupts or drops data

---

## NON-GOALS

- Preventing all data deletion (user can delete their own data)
- Guaranteeing restore after catastrophic storage loss (all chunks gone)
- Protecting against a user's own compromised password + recovery key simultaneously
- Real-time sync between devices (this is backup, not sync)

System fails loudly, not silently.
