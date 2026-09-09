import {
  Selectors,
  Headings,
  fillField,
  fillReadonlyField,
  submitForm,
  waitForHeading,
  waitForText,
  hasText,
  clickButton,
  uniqueEmail,
  navigateTo,
} from "../helpers/selectors";

// This test runs the complete signup wizard using Local Filesystem instead of S3.
// No cloud credentials needed — uses a temp directory on disk.

const TEST_PASSWORD = "SecurePassword123!";
const TEST_ENCRYPTION_PASSWORD = "Encrypt1234!";
const LOCAL_STORAGE_DIR = "/tmp/cloudless-e2e-local-storage";
const LOCAL_BACKUP_DIR = "/tmp/cloudless-e2e-backup";

describe("Local Filesystem Storage Wizard", () => {
  it("should complete the full signup wizard with local storage", async () => {
    // The wdio.conf.ts `before` hook already navigated and waited for the login
    // page.  A second navigation here would trigger another WASM JIT recompile
    // that runs concurrently with the Argon2 password hash in create_user,
    // causing CPU contention and multi-minute API timeouts.

    const email = uniqueEmail();

    // ── Step 0: Create Account ──────────────────────────────────────
    const signUpBtn = await $(Selectors.signUpToggle);
    await signUpBtn.waitForClickable();
    await signUpBtn.click();

    await fillField(Selectors.nameInput, "Local Storage Test User");
    await fillField(Selectors.emailInput, email);
    await fillField(Selectors.passwordInput, TEST_PASSWORD);
    await submitForm();

    // ── Step 1: Set Encryption Password ─────────────────────────────
    // With email sending disabled the server auto-verifies email on signup; PolicyAcceptance
    // auto-advances when no policies are pending. signup_and_login + policy check
    // can take >30s under load — use a longer timeout here.
    await waitForHeading(Headings.setEncryptionPassword, 60000);
    await fillField(Selectors.encryptionPassword, TEST_ENCRYPTION_PASSWORD);
    await fillField(Selectors.encryptionConfirm, TEST_ENCRYPTION_PASSWORD);
    await submitForm();

    // ── Step 2: Register Device ─────────────────────────────────────
    // After encryption setup, the app transitions directly to the device
    // registration step (no intermediate confirmation page).
    await waitForHeading(Headings.registerDevice);
    // Wait for hostname to be auto-filled (fetched asynchronously via Tauri).
    await browser.waitUntil(
      async () => {
        const input = await $(Selectors.deviceName);
        return (await input.getValue()).length > 0;
      },
      { timeout: 10000, timeoutMsg: "Device name was not auto-filled within 10s" }
    );
    await submitForm();

    // ── Step 3: Configure Storage — Local Filesystem ─────────────────
    await waitForHeading(Headings.configureStorage);

    // Select "External Folder" provider
    await clickButton("External Folder");

    // Fill in the local storage form
    await fillField("#local-storage-name", "E2E Local Storage");
    await fillField("#local-root-path", LOCAL_STORAGE_DIR);

    await submitForm();

    // ── Step 4: Configure Backup ────────────────────────────────────
    await waitForHeading(Headings.configureBackup);
    // The source directory field is read-only; set its value programmatically.
    await fillReadonlyField(Selectors.sourceDirectory, LOCAL_BACKUP_DIR);
    await submitForm();

    // ── Wizard complete — should land on the dashboard ──────────────
    await waitForHeading(Headings.dashboard, 30000);
  });

  it("should show the local storage in the dashboard", async () => {
    await waitForText("Quick Actions");
    await waitForText("Start Backup");
  });

  it("should start and complete a backup to local storage", async () => {
    const btn = await $('[data-testid="quick-backup-button"]');
    if (await btn.isExisting()) {
      await btn.click();
    }

    // Wait for backup to complete
    await browser.waitUntil(
      async () =>
        (await hasText("Completed")) ||
        (await hasText("uploaded")) ||
        (await hasText("Running")),
      { timeout: 30000, timeoutMsg: "Backup did not start within 30s" }
    );

    await browser.waitUntil(
      async () => (await hasText("Completed")) || (await hasText("uploaded")),
      { timeout: 60000, timeoutMsg: "Backup did not complete within 60s" }
    );
  });

  it("should show local filesystem storage type in Settings", async () => {
    await navigateTo("Settings");
    await waitForHeading(Headings.settings);

    await clickButton("Storage");
    await browser.pause(500);

    // Should show our local storage
    await browser.waitUntil(
      async () =>
        (await hasText("E2E Local Storage")) || (await hasText("Local Filesystem")),
      { timeout: 10000, timeoutMsg: "Local storage not found in settings" }
    );
  });
});
