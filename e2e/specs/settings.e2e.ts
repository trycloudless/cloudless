import {
  Selectors,
  Headings,
  waitForHeading,
  hasText,
  navigateTo,
  clickButton,
} from "../helpers/selectors";

// Runs after files.e2e.ts in the same session.

/**
 * Close any open modal dialog by clicking the X close button (SVG icon).
 */
async function closeModal(): Promise<void> {
  // The X close button is the small button with an SVG in the modal header.
  // It has class containing "rounded-lg" and "text-text-secondary".
  const closeBtn = await $("button=Cancel");
  if (await closeBtn.isExisting() && await closeBtn.isClickable()) {
    await closeBtn.click();
    await browser.pause(500);
    return;
  }
  // Fallback: find the close X button (SVG with cross lines)
  const xBtn = await $("//div[contains(@class,'fixed')]//button[.//svg[.//line]]");
  if (await xBtn.isExisting() && await xBtn.isClickable()) {
    await xBtn.click();
    await browser.pause(500);
  }
}

describe("Settings", () => {
  before(async () => {
    await navigateTo("Settings");
    await waitForHeading(Headings.settings);
  });

  // ── Backups Tab ─────────────────────────────────────────────────

  describe("Backups Tab", () => {
    before(async () => {
      await clickButton("Backups");
      await browser.pause(500);
    });

    it("should display auto-backup settings", async () => {
      await browser.waitUntil(
        async () => (await hasText("automatic")) || (await hasText("Auto")),
        { timeout: 10000, timeoutMsg: "Auto-backup settings not found" }
      );
    });

    it("should display backup config section", async () => {
      await browser.waitUntil(
        async () =>
          (await hasText("Default Backup")) || (await hasText("cloudless-e2e-backup")),
        { timeout: 10000, timeoutMsg: "Backup config not found" }
      );
    });
  });

  // ── Storage Tab ─────────────────────────────────────────────────

  describe("Storage Tab", () => {
    before(async () => {
      await clickButton("Storage");
      await browser.pause(500);
    });

    it("should display the current device", async () => {
      await browser.waitUntil(
        async () => (await hasText("This Device")) || (await hasText("device")),
        { timeout: 10000, timeoutMsg: "Device info not found" }
      );
    });

    it("should display the storage we configured", async () => {
      await browser.waitUntil(
        async () =>
          (await hasText("E2E Test Storage")) ||
          (await hasText("Local Filesystem")) ||
          (await hasText("AWS")),
        { timeout: 10000, timeoutMsg: "Storage configuration not found" }
      );
    });
  });

  // ── Security Tab ────────────────────────────────────────────────

  describe("Security Tab", () => {
    before(async () => {
      await clickButton("Security");
      await browser.pause(500);
    });

    it("should display encryption info section", async () => {
      await browser.waitUntil(
        async () => (await hasText("AES-256-GCM")) || (await hasText("Encryption")),
        { timeout: 10000, timeoutMsg: "Encryption info not found" }
      );
    });

    it("should show non-zero byte metrics when files have been backed up", async () => {
      // After a successful backup, all three dashboard counters (Encrypted files,
      // Original data, Storage used) must be non-zero and internally consistent.
      // A mismatch like "71 files / 0 B / 0 B" indicates the backend query draws
      // from different sources with different scopes.
      await browser.waitUntil(
        async () =>
          (await hasText("Encrypted files")) ||
          (await hasText("Original data")) ||
          (await hasText("Storage used")),
        { timeout: 10000, timeoutMsg: "Security metrics section not found" }
      );

      const bodyText = await $("body").getText();
      // Both size metrics must show a real value — "0 B" for either means the
      // consistency invariant is violated.
      const originalZero = bodyText.match(/Original data[\s\S]*?0\s*B/);
      const storageZero = bodyText.match(/Storage used[\s\S]*?0\s*B/);
      expect(originalZero).toBeNull();
      expect(storageZero).toBeNull();
    });

    it("should mask device IDs (not show full UUIDs in primary view)", async () => {
      const bodyText = await $("body").getText();
      // Storage tab should not expose raw full UUIDs in the primary view.
      // Masked format is "xxxxxxxx-…-xxxxx" — the ellipsis character signals masking.
      const hasMasked = bodyText.includes("…") || bodyText.includes("…");
      // If device info is loaded, the ID must be masked.
      if (bodyText.includes("This Device") || bodyText.includes("Registered")) {
        expect(hasMasked).toBe(true);
      }
    });

    it("should display the Save Recovery Key button", async () => {
      const btn = await $(Selectors.exportRecoveryKeyButton);
      await btn.waitForExist({ timeout: 10000 });
      expect(await btn.isDisplayed()).toBe(true);
    });

    it("should display the Change Password button", async () => {
      const btn = await $(Selectors.changePasswordButton);
      expect(await btn.isExisting()).toBe(true);
    });

    it("should open and close export recovery key modal", async () => {
      const btn = await $(Selectors.exportRecoveryKeyButton);
      await btn.click();

      // Modal should show the recovery key display
      await browser.waitUntil(
        async () => (await hasText("recovery key")) || (await hasText("Recovery Key")),
        { timeout: 10000, timeoutMsg: "Recovery key modal did not appear" }
      );

      // Wait for the key to be generated (Display phase)
      await browser.waitUntil(
        async () => (await hasText("Copy to Clipboard")) || (await hasText("Download .txt")),
        { timeout: 15000, timeoutMsg: "Recovery key Display phase did not appear" }
      );

      // The acknowledgment checkbox must be present before continuing.
      const ackCheckbox = await $("input[type='checkbox']");
      expect(await ackCheckbox.isExisting()).toBe(true);

      // The Download .txt button must be present.
      const downloadBtn = await $("button=Download .txt");
      expect(await downloadBtn.isExisting()).toBe(true);

      // The "I've saved it" continue button must be disabled until checkbox is checked.
      const continueBtn = await $("button=I've saved it — Continue");
      if (await continueBtn.isExisting()) {
        const isDisabled = await continueBtn.getAttribute("disabled");
        expect(isDisabled).not.toBeNull();
      }

      await closeModal();
    });

    it("should open and close change password dialog", async () => {
      const btn = await $(Selectors.changePasswordButton);
      await btn.click();

      // Should show password change form
      await browser.waitUntil(
        async () => (await hasText("Current Password")) || (await hasText("New Password")),
        { timeout: 10000, timeoutMsg: "Change password dialog did not appear" }
      );

      await closeModal();
    });

    it("should display the Account section", async () => {
      await browser.waitUntil(
        async () => await hasText("Account"),
        { timeout: 10000, timeoutMsg: "Account section not found in Security tab" }
      );
    });

    it("should show the subscription tier", async () => {
      await browser.waitUntil(
        async () => {
          const b = await $("body").getText();
          return (
            b.includes("Free") ||
            b.includes("Starter") ||
            b.includes("Pro") ||
            b.includes("Lifetime")
          );
        },
        { timeout: 10000, timeoutMsg: "Subscription tier did not load" }
      );
    });
  });

  // ── Retention Tab ────────────────────────────────────────────────

  describe("Retention Tab", () => {
    before(async () => {
      await clickButton("Retention");
      await browser.pause(500);
    });

    it("should display retention settings", async () => {
      await browser.waitUntil(
        async () => (await hasText("Retention")) || (await hasText("retention")) || (await hasText("days")),
        { timeout: 10000, timeoutMsg: "Retention settings not found" }
      );
    });

    it("should display garbage collection section", async () => {
      await browser.waitUntil(
        async () =>
          (await hasText("Garbage Collection")) || (await hasText("Run Garbage Collection")),
        { timeout: 10000, timeoutMsg: "GC section not found" }
      );
    });

    it("should run garbage collection", async () => {
      const gcBtn = await $(Selectors.runGcButton);
      if (await gcBtn.isExisting() && await gcBtn.isClickable()) {
        await gcBtn.click();

        // Wait for GC to complete — may show deleted/freed/completed text
        await browser.waitUntil(
          async () =>
            (await hasText("deleted")) ||
            (await hasText("freed")) ||
            (await hasText("completed")) ||
            (await hasText("No expired")),
          { timeout: 30000, timeoutMsg: "GC did not complete" }
        );
      }
    });

    it("should not show plain 'Loading...' text in GC History after load", async () => {
      // Navigate away and back to trigger a fresh load.
      await navigateTo("Dashboard");
      await browser.pause(300);
      await navigateTo("Settings");
      await waitForHeading(Headings.settings);
      await clickButton("Retention");

      // Give the async load time to finish, then assert the raw loading
      // placeholder is gone. Either run rows or the empty-state message must
      // be present — never the plain "Loading..." string.
      await browser.waitUntil(
        async () =>
          (await hasText("Completed")) ||
          (await hasText("Failed")) ||
          (await hasText("No garbage collection")),
        { timeout: 15000, timeoutMsg: "GC History did not finish loading" }
      );

      const bodyText = await $("body").getText();
      // The plain "Loading..." placeholder must never survive past the load phase.
      expect(bodyText).not.toContain("Loading...");
    });
  });

  // ── Preferences Tab ─────────────────────────────────────────────

  describe("Preferences Tab", () => {
    before(async () => {
      await clickButton("Preferences");
      await browser.pause(500);
    });

    it("should display theme options", async () => {
      await browser.waitUntil(
        async () =>
          (await hasText("Theme")) || (await hasText("Dark")) || (await hasText("Light")),
        { timeout: 10000, timeoutMsg: "Preferences content not found" }
      );
    });

  });
});
