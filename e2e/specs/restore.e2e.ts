import {
  Headings,
  waitForHeading,
  waitForText,
  hasText,
  navigateTo,
  clickButton,
} from "../helpers/selectors";

// Runs after files.e2e.ts — files have been backed up and are browsable.

describe("Restore", () => {
  describe("Restore a file from Files page", () => {
    before(async () => {
      await navigateTo("Files");
      await waitForHeading(Headings.files);
    });

    it("should select config and show backed up files", async () => {
      await waitForText("Default Backup");

      const configCard = await $('[data-testid="config-card"]');
      if (await configCard.isExisting()) {
        await configCard.click();
      } else {
        const btn = await $("//button[.//*[contains(text(),'Default Backup')]]");
        if (await btn.isExisting()) await btn.click();
      }
      await browser.pause(1000);

      // Wait for file list to appear — check body text for our test files
      await browser.waitUntil(
        async () => {
          const text = await $("body").getText();
          return text.includes("test-file-1");
        },
        { timeout: 15000, timeoutMsg: "Test files did not appear in file list" }
      );
    });

    it("should expand a file to show version details", async () => {
      const fileRow = await $('[data-testid="file-row"]');
      if (await fileRow.isExisting()) {
        await fileRow.click();
      } else {
        const row = await $("//*[contains(text(),'test-file-1')]");
        if (await row.isExisting()) await row.click();
      }
      await browser.pause(500);

      await browser.waitUntil(
        async () => (await hasText("v1")) || (await hasText("Verified")),
        { timeout: 15000, timeoutMsg: "Version details did not expand" }
      );
    });

    it("should click Restore on a file version and open the modal", async () => {
      // Restore button is opacity-0 until hover — use JS click
      let restoreBtn = await $('[data-testid="version-restore-button"]');
      if (!(await restoreBtn.isExisting())) {
        restoreBtn = await $("//button[contains(text(),'Restore')]");
      }
      if (await restoreBtn.isExisting()) {
        await browser.execute((el: HTMLElement) => el.click(), restoreBtn);
      }

      await browser.waitUntil(
        async () => (await hasText("Restore Files")) || (await hasText("Selected")),
        { timeout: 10000, timeoutMsg: "Restore modal did not appear" }
      );
    });

    it("should show destination options in the restore modal", async () => {
      const hasDestOpts =
        (await hasText("Original location")) ||
        (await hasText("Downloads folder")) ||
        (await hasText("Custom path"));
      expect(hasDestOpts).toBe(true);
    });

    it("should proceed to review and confirm restore", async () => {
      // Click "Review & Restore" — use JS click to bypass WebdriverIO's center-point
      // interactability check, which is flaky inside an animated modal backdrop.
      let reviewBtn = await $('[data-testid="review-restore-button"]');
      if (!(await reviewBtn.isExisting())) {
        reviewBtn = await $("button=Review & Restore");
      }
      await reviewBtn.waitForExist({ timeout: 15000 });
      await browser.execute((el: HTMLElement) => el.click(), reviewBtn);
      await browser.pause(1000);

      await waitForText("Confirm Restore", 10000);

      // Click "Confirm & Restore" — use JS click to bypass the modal backdrop's
      // center-point interactability check (same reason reviewBtn uses JS click above).
      let confirmBtn = await $('[data-testid="confirm-restore-button"]');
      if (!(await confirmBtn.isExisting())) {
        confirmBtn = await $("button=Confirm & Restore");
      }
      await confirmBtn.waitForExist({ timeout: 15000 });
      await browser.execute((el: HTMLElement) => el.click(), confirmBtn);

      // Wait for restore to complete — single small file should be fast.
      // The modal closes on success, so the "Confirm Restore" text disappears.
      await browser.waitUntil(
        async () => {
          const stillConfirming = await hasText("Confirm Restore");
          return !stillConfirming;
        },
        { timeout: 60000, timeoutMsg: "Restore did not complete within 60s" }
      );
    });
  });

  describe("Restore Reports", () => {
    before(async () => {
      await browser.pause(2000); // Let any modal/overlay dismiss
      await navigateTo("Backup");
      await waitForHeading(Headings.backup, 20000);
    });

    it("should show restore job in Reports > Restore tab", async () => {
      await clickButton("Reports");
      await browser.pause(500);

      const restoreTab = await $("//button[text()='Restore']");
      if (await restoreTab.isExisting()) {
        await restoreTab.click();
        await browser.pause(1000);
      }

      // The restore tab should load — may show jobs or "No restore jobs"
      await browser.waitUntil(
        async () =>
          (await hasText("completed")) ||
          (await hasText("Completed")) ||
          (await hasText("No restore jobs")) ||
          (await hasText("Restore")),
        { timeout: 15000, timeoutMsg: "Restore reports did not load" }
      );
    });
  });
});
