import {
  Selectors,
  Headings,
  fillField,
  fillReadonlyField,
  submitForm,
  waitForHeading,
  clickButton,
  acceptPendingPoliciesIfShown,
  uniqueEmail,
} from "../helpers/selectors";

const TEST_PASSWORD = "SecurePassword123!";
const TEST_ENCRYPTION_PASSWORD = "Encrypt1234!";
const LOCAL_STORAGE_DIR = "/tmp/cloudless-e2e-local-storage";
const LOCAL_BACKUP_DIR = "/tmp/cloudless-e2e-backup";

describe("Signup Wizard (full flow)", () => {
  it("should complete the entire signup wizard end to end", async () => {
    // The wdio.conf.ts `before` hook already navigated and waited for the login
    // page.  A second navigation here would trigger another WASM JIT recompile
    // that runs concurrently with the Argon2 password hash in create_user,
    // causing CPU contention and multi-minute API timeouts.

    // ── Step 0: Create Account ──────────────────────────────────────
    const signUpBtn = await $(Selectors.signUpToggle);
    await signUpBtn.waitForClickable();
    await signUpBtn.click();

    const email = uniqueEmail();
    await fillField(Selectors.nameInput, "E2E Test User");
    await fillField(Selectors.emailInput, email);
    await fillField(Selectors.passwordInput, TEST_PASSWORD);
    await submitForm();

    // ── Step 1: Set Encryption Password ─────────────────────────────
    // With email sending disabled the server auto-verifies email on signup. PolicyAcceptance
    // either auto-advances when empty or asks for seeded policy acceptance first.
    await acceptPendingPoliciesIfShown(60000);

    await fillField(Selectors.encryptionPassword, TEST_ENCRYPTION_PASSWORD);
    await fillField(Selectors.encryptionConfirm, TEST_ENCRYPTION_PASSWORD);
    await submitForm();

    // ── Step 2: Register Device ─────────────────────────────────────
    await waitForHeading(Headings.registerDevice);

    // Wait for hostname to be auto-filled before submitting (fetched asynchronously).
    // Submitting before it loads leaves the required field empty and the form silently
    // stays on this page (HTML required validation blocks the submit event).
    await browser.waitUntil(
      async () => {
        const input = await $(Selectors.deviceName);
        return (await input.getValue()).length > 0;
      },
      { timeout: 10000, timeoutMsg: "Device name was not auto-filled within 10s" }
    );

    await submitForm();

    // ── Step 3: Configure Storage — Local Filesystem ────────────────
    await waitForHeading(Headings.configureStorage);

    // Select "External Folder" provider (default is Amazon S3)
    await clickButton("External Folder");

    // Fill in the local storage form
    await fillField("#local-storage-name", "E2E Test Storage");
    await fillField("#local-root-path", LOCAL_STORAGE_DIR);

    await submitForm();

    // ── Step 4: Configure Backup ────────────────────────────────────
    await waitForHeading(Headings.configureBackup);

    // The source directory field is read-only; set its value programmatically.
    await fillReadonlyField(Selectors.sourceDirectory, LOCAL_BACKUP_DIR);
    await submitForm();

    // ── Wizard complete — should land on the dashboard ──────────────
    // Client-side Argon2 (encryption setup) can take 30-60s in debug builds.
    // The wizard steps after encryption add another ~30s of API calls.
    await waitForHeading(Headings.dashboard, 120000);
  });
});
