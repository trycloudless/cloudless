/**
 * Shared helper that completes the full signup wizard.
 * Call this at the start of any spec that needs a fully set-up user session.
 */

import {
  Selectors,
  Headings,
  fillField,
  submitForm,
  waitForHeading,
  clickButton,
  acceptPendingPoliciesIfShown,
  uniqueEmail,
} from "./selectors";

export const TEST_BACKUP_DIR = "/tmp/cloudless-e2e-backup";
export const LOCAL_STORAGE_DIR = "/tmp/cloudless-e2e-local-storage";
// Must match the files created by run-functional-test.sh
export const TEST_FILE_COUNT = 5; // test-file-1.txt, test-file-2.txt, test-data.json, test-binary.bin, subdir/nested-file.txt
export const TEST_PASSWORD = "SecurePassword123!";
export const TEST_ENCRYPTION_PASSWORD = "Encrypt1234!";

export async function completeSignupWizard(): Promise<{ email: string }> {
  await browser.url("tauri://localhost");
  await browser.pause(2000);

  const email = uniqueEmail();

  // Step 0: Create Account
  const signUpBtn = await $(Selectors.signUpToggle);
  await signUpBtn.waitForClickable();
  await signUpBtn.click();

  await fillField(Selectors.nameInput, "E2E Test User");
  await fillField(Selectors.emailInput, email);
  await fillField(Selectors.passwordInput, TEST_PASSWORD);
  await submitForm();

  // Step 1: Set Encryption Password
  // Note: email verification is skipped in E2E (EMAIL_PROVIDER=log). The server
  // auto-verifies the email on signup. PolicyAcceptance either auto-advances or
  // requires accepting seeded policies before encryption setup can continue.
  await acceptPendingPoliciesIfShown();
  await fillField(Selectors.encryptionPassword, TEST_ENCRYPTION_PASSWORD);
  await fillField(Selectors.encryptionConfirm, TEST_ENCRYPTION_PASSWORD);
  await submitForm();

  // Step 2: Register Device
  await waitForHeading(Headings.registerDevice);
  // Wait for hostname to be auto-filled before submitting (fetched asynchronously).
  await browser.waitUntil(
    async () => {
      const input = await $(Selectors.deviceName);
      return (await input.getValue()).length > 0;
    },
    { timeout: 10000, timeoutMsg: "Device name was not auto-filled within 10s" }
  );
  await submitForm();

  // Step 3: Configure Storage — External Folder
  await waitForHeading(Headings.configureStorage);
  await clickButton("External Folder");
  await fillField("#local-storage-name", "E2E Test Storage");
  await fillField("#local-root-path", LOCAL_STORAGE_DIR);
  await submitForm();

  // Step 4: Configure Backup
  await waitForHeading(Headings.configureBackup);
  await fillField(Selectors.sourceDirectory, TEST_BACKUP_DIR);
  await submitForm();

  // Wait for dashboard to load
  await waitForHeading(Headings.dashboard);

  return { email };
}
