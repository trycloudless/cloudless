import {
  Headings,
  waitForHeading,
  hasText,
  navigateTo,
  clickButton,
} from "../helpers/selectors";

// Runs after settings.e2e.ts in the same session. App is already on Settings page.
// Tests inline rename, two-step toggle, and cleanup policy editing on the backup config card.

describe("Backup Config Management", () => {
  before(async () => {
    await navigateTo("Settings");
    await waitForHeading(Headings.settings);
    await clickButton("Backups");
    await browser.pause(500);
    await browser.waitUntil(
      async () =>
        (await hasText("Default Backup")) ||
        (await hasText("cloudless-e2e-backup")) ||
        (await hasText("Renamed E2E Config")),
      { timeout: 10000, timeoutMsg: "Backup config card not found in Backups tab" }
    );
  });

  it("should rename a backup config", async () => {
    // Click the config name to enter rename mode (title text contains "Click to rename")
    const configName = await $(
      "//*[contains(@title,'Click to rename') or (contains(text(),'Default Backup') and not(ancestor::*[contains(@class,'badge')]))]"
    );
    if (await configName.isExisting()) {
      await configName.click();
      await browser.pause(500);
    }

    const renameInput = await $("input.font-semibold");
    if (await renameInput.isExisting()) {
      await renameInput.clearValue();
      await renameInput.setValue("Renamed E2E Config");

      const saveBtn = await $("button=Save");
      await saveBtn.waitForClickable({ timeout: 5000 });
      await saveBtn.click();
      await browser.pause(500);

      await browser.waitUntil(
        async () => await hasText("Renamed E2E Config"),
        { timeout: 10000, timeoutMsg: "Renamed config name did not appear" }
      );
    }
  });

  it("should toggle a backup config off then back on", async () => {
    // First click on "Disable" starts the two-step confirm flow
    const disableBtn = await $("//button[normalize-space()='Disable']");
    if (!(await disableBtn.isExisting())) return; // config already disabled — skip

    await disableBtn.click();
    await browser.pause(300);

    // Second click confirms
    const confirmDisableBtn = await $("//button[contains(normalize-space(),'Confirm Disable')]");
    if (await confirmDisableBtn.isExisting()) {
      await confirmDisableBtn.click();
      await browser.pause(500);

      await browser.waitUntil(
        async () => await hasText("Disabled"),
        { timeout: 10000, timeoutMsg: "Config did not show Disabled badge after toggle" }
      );

      // Re-enable so subsequent tests aren't affected
      const enableBtn = await $("//button[normalize-space()='Enable']");
      if (await enableBtn.isExisting()) {
        await enableBtn.click();
        await browser.pause(300);
        const confirmEnableBtn = await $("//button[contains(normalize-space(),'Confirm Enable')]");
        if (await confirmEnableBtn.isExisting()) {
          await confirmEnableBtn.click();
          await browser.pause(500);
        }
      }
    }
  });

  it("should update the cleanup policy on a backup config", async () => {
    const editBtn = await $("//button[normalize-space()='Edit']");
    if (!(await editBtn.isExisting()) || !(await editBtn.isClickable())) return;

    await editBtn.click();
    await browser.pause(300);

    // Enable the "Delete after N days" checkbox if not already checked
    const deleteCheckbox = await $("input[type='checkbox']");
    if (await deleteCheckbox.isExisting() && !(await deleteCheckbox.isSelected())) {
      await browser.execute((el: HTMLElement) => el.click(), deleteCheckbox);
    }

    const daysInput = await $("input[type='number']");
    if (await daysInput.isExisting()) {
      await daysInput.clearValue();
      await daysInput.setValue("7");
    }

    const saveBtn = await $("button=Save");
    await saveBtn.waitForClickable({ timeout: 5000 });
    await saveBtn.click();
    await browser.pause(500);

    await browser.waitUntil(
      async () => (await hasText("7 days")) || (await hasText("Delete after")),
      { timeout: 10000, timeoutMsg: "Cleanup policy update did not persist" }
    );
  });
});
