import {
  Headings,
  waitForHeading,
  navigateTo,
} from "../helpers/selectors";
import { TEST_ENCRYPTION_PASSWORD } from "../helpers/setup-wizard";

// Runs LAST in the grouped session — tests navigation, lock/unlock, sign out.

describe("Navigation", () => {
  before(async () => {
    // Dismiss any open modal left by a failed prior test (e.g. restore modal).
    // The modal backdrop closes on click; clicking it via JS avoids interactability
    // issues caused by the backdrop itself blocking the sidebar.
    await browser.execute(() => {
      const backdrop = document.querySelector<HTMLElement>(".fixed.inset-0");
      if (backdrop) backdrop.click();
    });
    await browser.pause(500);
    await navigateTo("Dashboard");
    await browser.pause(1000);
  });

  it("should navigate to Files", async () => {
    await navigateTo("Files");
    await waitForHeading(Headings.files);
  });

  it("should navigate to Backup", async () => {
    await navigateTo("Backup");
    await waitForHeading(Headings.backup);
  });

  it("should navigate to Settings", async () => {
    await navigateTo("Settings");
    await waitForHeading(Headings.settings);
  });

  it("should navigate back to Dashboard", async () => {
    await navigateTo("Dashboard");
    await waitForHeading(Headings.dashboard);
  });

  it("should lock the app and show unlock encryption screen", async () => {
    const lockBtn = await $("//aside//button[.//span[text()='Lock']]");
    if (await lockBtn.isExisting()) {
      await lockBtn.click();
    }

    await waitForHeading(Headings.unlockEncryption);
  });

  it("should unlock with encryption password and return to main app", async () => {
    let passwordInput = await $('[data-testid="unlock-password-input"]');
    if (!(await passwordInput.isExisting())) {
      passwordInput = await $("#unlock-password");
    }
    await passwordInput.waitForDisplayed({ timeout: 30000 });
    await passwordInput.setValue(TEST_ENCRYPTION_PASSWORD);

    let unlockBtn = await $('[data-testid="unlock-submit-button"]');
    if (!(await unlockBtn.isExisting())) {
      unlockBtn = await $("button[type='submit']");
    }
    await unlockBtn.waitForClickable();
    await unlockBtn.click();

    // Unlock decryption can take 10-20 seconds. Wait for the unlock heading
    // to disappear (app navigates to main view — could be any page heading).
    await browser.waitUntil(
      async () => {
        try {
          // Check if sidebar appeared (means we're in the main app)
          const sidebar = await $("//aside");
          if (await sidebar.isExisting()) return true;
          // Or check heading changed from unlock
          const heading = await $("h2.gradient-text");
          if (!(await heading.isExisting())) return false;
          const text = await heading.getText();
          return text !== "Unlock Encryption";
        } catch {
          return false;
        }
      },
      { timeout: 60000, timeoutMsg: "App did not return to main view after unlock" }
    );
  });

  it("should sign out and return to login page", async () => {
    await browser.pause(1000);

    const signOutBtn = await $("//aside//button[.//span[text()='Sign Out']]");
    await signOutBtn.waitForClickable({ timeout: 10000 });
    await signOutBtn.click();

    await waitForHeading(Headings.login, 10000);
  });
});
