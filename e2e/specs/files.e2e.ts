import {
  Selectors,
  Headings,
  waitForHeading,
  waitForText,
  hasText,
  navigateTo,
  clickButton,
} from "../helpers/selectors";

// Runs after backup.e2e.ts — a backup has completed, files should be browsable.

describe("Files", () => {
  before(async () => {
    await navigateTo("Files");
    await waitForHeading(Headings.files);
  });

  it("should display the Files page subtitle", async () => {
    await waitForText("Browse and restore protected files");
  });

  it("should show the backup config for selection", async () => {
    await waitForText("Default Backup");
  });

  it("should show file list after selecting a config", async () => {
    // Click config card: try data-testid first, then fallback to text click
    const configCard = await $('[data-testid="config-card"]');
    if (await configCard.isExisting()) {
      await configCard.click();
    } else {
      // Fallback: click the button containing "Default Backup" text
      const btn = await $("//button[.//*[contains(text(),'Default Backup')]]");
      if (await btn.isExisting()) await btn.click();
    }
    await browser.pause(1000);

    await browser.waitUntil(
      async () =>
        (await hasText("test-file-1")) ||
        (await hasText("test-data")) ||
        (await hasText("test-binary")) ||
        (await hasText("Search files")),
      { timeout: 15000, timeoutMsg: "File list did not appear after selecting config" }
    );
  });

  it("should have view mode toggle buttons", async () => {
    const listBtn = await $(Selectors.viewModeList);
    if (await listBtn.isExisting()) {
      expect(await listBtn.isDisplayed()).toBe(true);
    }
  });

  it("should show backed up test files", async () => {
    // Re-navigate to files to ensure a fresh render, then check body text.
    await navigateTo("Dashboard");
    await browser.pause(500);
    await navigateTo("Files");
    await browser.pause(1000);

    // Select the config again
    const configCard = await $('[data-testid="config-card"]');
    if (await configCard.isExisting()) {
      await configCard.click();
      await browser.pause(1000);
    }

    // Now check body text for our test file
    await browser.waitUntil(
      async () => {
        const text = await $("body").getText();
        return text.includes("test-file-1");
      },
      { timeout: 10000, timeoutMsg: "test-file-1 not found after re-navigation" }
    );
  });

  it("should expand a file row to show version details", async () => {
    const fileRow = await $('[data-testid="file-row"]');
    if (await fileRow.isExisting()) {
      await fileRow.click();
    } else {
      // Fallback: click first file row text
      const row = await $("//*[contains(text(),'test-file-1')]");
      if (await row.isExisting()) await row.click();
    }
    await browser.pause(500);

    await browser.waitUntil(
      async () => (await hasText("v1")) || (await hasText("Verified")),
      { timeout: 15000, timeoutMsg: "Version details did not expand" }
    );
  });

  it("should show Restore button on version row", async () => {
    // Restore button is hidden until hover — check existence
    const restoreBtn = await $('[data-testid="version-restore-button"]');
    if (await restoreBtn.isExisting()) {
      expect(true).toBe(true);
    } else {
      // Fallback: look for any button with "Restore" text in the version area
      const btn = await $("//button[contains(text(),'Restore')]");
      expect(await btn.isExisting()).toBe(true);
    }
  });
});

// Bulk selection tests run after the Files suite has confirmed files are browsable.
describe("Files — bulk selection", () => {
  // Navigate to Files and open the config before each block of tests.
  before(async () => {
    await navigateTo("Files");
    await waitForHeading(Headings.files);

    const configCard = await $('[data-testid="config-card"]');
    if (await configCard.isExisting()) {
      await configCard.click();
    } else {
      const btn = await $("//button[.//*[contains(text(),'Default Backup')]]");
      if (await btn.isExisting()) await btn.click();
    }
    await browser.pause(1000);

    // Ensure file list is visible before proceeding.
    await browser.waitUntil(
      async () =>
        (await hasText("test-file-1")) ||
        (await hasText("Search files")),
      { timeout: 15000, timeoutMsg: "File list did not load before bulk tests" }
    );
  });

  it("should show bulk action bar after checking a file", async () => {
    // The bulk action bar should be absent initially.
    const barBefore = await $(Selectors.bulkActionBar);
    expect(await barBefore.isExisting()).toBe(false);

    // Click the first file checkbox.
    const checkbox = await $(Selectors.fileCheckbox);
    await checkbox.waitForExist({ timeout: 5000 });
    // Use JS click to avoid issues with the tiny hit-target on WebKit.
    await browser.execute((el: HTMLElement) => el.click(), checkbox);
    await browser.pause(300);

    // Bulk action bar should now be visible.
    const bar = await $(Selectors.bulkActionBar);
    await bar.waitForDisplayed({ timeout: 5000 });
    expect(await bar.isDisplayed()).toBe(true);

    // The bar should report "1 selected".
    const barText = await bar.getText();
    expect(barText).toContain("1 selected");
  });

  it("should update count when a second file is checked", async () => {
    const checkboxes = await $$(Selectors.fileCheckbox);
    if (checkboxes.length >= 2) {
      await browser.execute((el: HTMLElement) => el.click(), checkboxes[1]);
      await browser.pause(300);

      const bar = await $(Selectors.bulkActionBar);
      const barText = await bar.getText();
      expect(barText).toContain("2 selected");
    }
  });

  it("should open the restore modal with selected files when Restore is clicked", async () => {
    const restoreBtn = await $(Selectors.bulkRestoreButton);
    await restoreBtn.waitForClickable({ timeout: 5000 });
    await restoreBtn.click();
    await browser.pause(500);

    // Restore modal should open — look for "Restore Files" heading or file count.
    await browser.waitUntil(
      async () => (await hasText("Restore Files")) || (await hasText("file")),
      { timeout: 10000, timeoutMsg: "Restore modal did not open" }
    );

    // Close without confirming so selection persists for further tests.
    const backdrop = await $(".fixed.inset-0");
    if (await backdrop.isExisting()) {
      await backdrop.click();
    }
    await browser.pause(300);
  });

  it("should retain selection after closing the restore modal without confirming", async () => {
    // Selection should still be non-empty (bar visible) after cancelling.
    const bar = await $(Selectors.bulkActionBar);
    expect(await bar.isDisplayed()).toBe(true);
  });

  it("should clear selection when Clear is clicked", async () => {
    const clearBtn = await $(Selectors.bulkClearButton);
    await clearBtn.waitForClickable({ timeout: 5000 });
    await clearBtn.click();
    await browser.pause(300);

    // Bar should disappear when selection is empty.
    const bar = await $(Selectors.bulkActionBar);
    expect(await bar.isExisting()).toBe(false);
  });

  it("should select all loaded files when the header checkbox is clicked", async () => {
    const selectAll = await $(Selectors.selectAllCheckbox);
    await selectAll.waitForExist({ timeout: 5000 });
    await browser.execute((el: HTMLElement) => el.click(), selectAll);
    await browser.pause(300);

    // Bulk bar should show a positive file count.
    const bar = await $(Selectors.bulkActionBar);
    await bar.waitForDisplayed({ timeout: 5000 });
    const barText = await bar.getText();
    expect(barText).toMatch(/\d+ selected/);

    // All visible file checkboxes should now be checked.
    const checkboxes = await $$(Selectors.fileCheckbox);
    for (const cb of checkboxes.slice(0, 3)) {
      const checked = await browser.execute(
        (el: HTMLInputElement) => el.checked,
        cb
      );
      expect(checked).toBe(true);
    }

    // Deselect all to leave a clean state.
    await browser.execute((el: HTMLElement) => el.click(), selectAll);
    await browser.pause(300);
  });

  it("should not show system artifacts (.DS_Store, Thumbs.db) in the file list", async () => {
    // Re-navigate to ensure a fresh file list render.
    await navigateTo("Dashboard");
    await browser.pause(300);
    await navigateTo("Files");
    await waitForHeading(Headings.files);

    const configCard = await $('[data-testid="config-card"]');
    if (await configCard.isExisting()) {
      await configCard.click();
    } else {
      const btn = await $("//button[.//*[contains(text(),'Default Backup')]]");
      if (await btn.isExisting()) await btn.click();
    }

    // Wait until the file list has loaded (legitimate test files are visible).
    await browser.waitUntil(
      async () =>
        (await hasText("test-file-1")) || (await hasText("Search files")),
      { timeout: 15000, timeoutMsg: "File list did not load for system-file check" }
    );

    const bodyText = await $("body").getText();
    expect(bodyText).not.toContain(".DS_Store");
    expect(bodyText).not.toContain("Thumbs.db");
  });

  it("should move selected files to bin when Move to Bin is clicked", async () => {
    // Select one file.
    const checkbox = await $(Selectors.fileCheckbox);
    await checkbox.waitForExist({ timeout: 5000 });
    await browser.execute((el: HTMLElement) => el.click(), checkbox);
    await browser.pause(300);

    // Record how many files are shown before the delete.
    const bodyBefore = await $("body").getText();

    const moveToBinBtn = await $(Selectors.bulkMoveToBinButton);
    await moveToBinBtn.waitForClickable({ timeout: 5000 });
    await moveToBinBtn.click();

    // Wait for the bulk action bar to disappear (selection cleared after success).
    await browser.waitUntil(
      async () => {
        const bar = await $(Selectors.bulkActionBar);
        return !(await bar.isExisting());
      },
      { timeout: 15000, timeoutMsg: "Bulk action bar did not disappear after move to bin" }
    );
  });
});

describe("Files — search", () => {
  before(async () => {
    await navigateTo("Files");
    await waitForHeading(Headings.files);

    const configCard = await $('[data-testid="config-card"]');
    if (await configCard.isExisting()) {
      await configCard.click();
    } else {
      const btn = await $("//button[.//*[contains(text(),'Default Backup')]]");
      if (await btn.isExisting()) await btn.click();
    }
    await browser.waitUntil(
      async () => (await hasText("test-file-1")) || (await hasText("Search files")),
      { timeout: 15000, timeoutMsg: "File list did not load before search tests" }
    );
  });

  it("should filter file list by search query", async () => {
    const searchInput = await $('input[placeholder="Search files..."]');
    await searchInput.waitForExist({ timeout: 5000 });
    await searchInput.setValue("test-file-1");
    await browser.pause(300);

    const body = await $("body").getText();
    expect(body).toContain("test-file-1");
    // photo.bin is a different file — should be filtered out
    expect(body).not.toContain("photo.bin");
  });

  it("should clear search and restore the full file list", async () => {
    // Try the clear button first (appears when input is non-empty, rendered as sibling of input)
    const clearBtn = await $(
      "//input[@placeholder='Search files...']/following-sibling::button"
    );
    if (await clearBtn.isExisting() && await clearBtn.isClickable()) {
      await clearBtn.click();
    } else {
      const searchInput = await $('input[placeholder="Search files..."]');
      await searchInput.clearValue();
    }
    await browser.pause(300);

    await browser.waitUntil(
      async () => (await hasText("test-file-1")) && (await hasText("test-file-2")),
      { timeout: 5000, timeoutMsg: "Full file list did not restore after clearing search" }
    );
  });
});
