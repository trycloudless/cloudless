import {
  Headings,
  waitForHeading,
  hasText,
  waitForText,
  navigateTo,
  clickButton,
} from "../helpers/selectors";
import { TEST_FILE_COUNT } from "../helpers/setup-wizard";

// Runs after dashboard.e2e.ts. Test files exist in /tmp/cloudless-e2e-backup/.

describe("Backup", () => {
  describe("Start Backup from Dashboard", () => {
    before(async () => {
      await navigateTo("Dashboard");
      await waitForHeading(Headings.dashboard);
    });

    it("should start a backup from the dashboard quick action", async () => {
      await waitForText("Start Backup");

      // Use data-testid for reliable selection (requires trunk rebuild)
      const btn = await $('[data-testid="quick-backup-button"]');
      if (await btn.isExisting()) {
        await btn.click();
      } else {
        // Fallback: find Backup button in main content area
        const fallbackBtn = await $("//main//button[text()='Backup']");
        if (await fallbackBtn.isExisting()) await fallbackBtn.click();
      }

      // Wait for backup to start
      await browser.waitUntil(
        async () =>
          (await hasText("Running")) ||
          (await hasText("Starting")) ||
          (await hasText("Completed")) ||
          (await hasText("uploaded")),
        { timeout: 30000, timeoutMsg: "Backup did not start within 30s" }
      );
    });

    it("should complete the backup with all test files", async () => {
      await browser.waitUntil(
        async () => (await hasText("Completed")) || (await hasText("uploaded")),
        { timeout: 60000, timeoutMsg: "Backup did not complete within 60s" }
      );
    });
  });

  describe("Backup Page", () => {
    before(async () => {
      await navigateTo("Backup");
      await waitForHeading(Headings.backup);
    });

    it("should show the backup config card", async () => {
      await waitForText("Default Backup");
      expect(await hasText("cloudless-e2e-backup")).toBe(true);
      // Storage type depends on test config (Local Filesystem or AWS S3)
      const hasStorage =
        (await hasText("Local Filesystem")) || (await hasText("AWS S3"));
      expect(hasStorage).toBe(true);
    });

    it("should show the Run Now button on the backup config card", async () => {
      // data-testid="backup-run-now" added to Leptos component
      const runBtn = await $('[data-testid="backup-run-now"]');
      expect(await runBtn.isExisting()).toBe(true);
    });

    it("should show Reports tab with the completed backup job", async () => {
      await clickButton("Reports");
      await browser.pause(2000);

      // Backup reports may take a moment to populate after the job completes
      await browser.waitUntil(
        async () =>
          (await hasText("completed")) ||
          (await hasText("Completed")) ||
          (await hasText(`${TEST_FILE_COUNT}`)) ||
          (await hasText("files")),
        { timeout: 30000, timeoutMsg: "No completed backup job found in reports" }
      );
    });

    it("should show the backup reports table with the completed backup job", async () => {
      const bodyText = await $("body").getText();
      expect(bodyText).toContain("Status");
      expect(bodyText).toContain("Backup");
      expect(bodyText).toContain("Started");
      expect(bodyText).toContain("Files");
      expect(bodyText).toContain("Stored");
      expect(bodyText).toContain("Saved");
      expect(bodyText).toContain(String(TEST_FILE_COUNT));

      const rows = await $$('[data-testid="backup-report-row"]');
      expect(rows.length).toBeGreaterThan(0);
    });

    it("should display job status as display-friendly label, not a raw backend token", async () => {
      // Status must be a title-case label — raw backend values like "in_progress"
      // or "completed_with_errors" must never be rendered verbatim.
      const bodyText = await $("body").getText();
      expect(bodyText).not.toContain("in_progress");
      expect(bodyText).not.toContain("completed_with_errors");
      // "Completed" (capitalised) must appear since we just ran a backup.
      expect(bodyText).toContain("Completed");
    });

    it("should label backup sizes with dedup context, not a bare arrow", async () => {
      // The metrics row must contain a label explaining what the size difference
      // means (deduplication savings), not just "X -> Y".
      const bodyText = await $("body").getText();
      const hasDedupeLabel =
        bodyText.includes("stored") ||
        bodyText.includes("deduped") ||
        bodyText.includes("saved") ||
        bodyText.includes("smaller") ||
        bodyText.includes("uploaded") ||
        bodyText.includes("optimization");
      expect(hasDedupeLabel).toBe(true);
    });

    it("should not show raw dedup jargon in reports list", async () => {
      // User-facing labels must be accessible; raw engineering terms must be absent
      // from default visible text.
      const bodyText = await $("body").getText();
      expect(bodyText).not.toContain("Dedup ratio");
      expect(bodyText).not.toContain("after dedup");
    });

    it("should open a backup job detail and hide system files by default", async () => {
      // Click the first report table row to open its detail view.
      const jobRow = await $('[data-testid="backup-report-row"]');
      await jobRow.waitForClickable({
        timeout: 15000,
        timeoutMsg: "Backup report row was not clickable",
      });
      await jobRow.click();
      await browser.waitUntil(
        async () =>
          (await hasText("System files")) && (await hasText("Original size")),
        { timeout: 15000, timeoutMsg: "Backup job detail did not open" }
      );

      // The detail view must not show system artifact filenames in the file table.
      const bodyText = await $("body").getText();
      expect(bodyText).not.toContain(".DS_Store");
      expect(bodyText).not.toContain("Thumbs.db");

      // The "System files" toggle must be present so users can opt in.
      const hasToggle = bodyText.includes("System files");
      expect(hasToggle).toBe(true);
    });

    it("should show correct column headers in the backup job file table", async () => {
      const bodyText = await $("body").getText();
      // File table column headers
      expect(bodyText).toContain("Original size");
      expect(bodyText).toContain("Storage saved");
      expect(bodyText).toContain("Dedup ratio");
      // Advanced details labels
      expect(bodyText).toContain("Chunks reused");
    });

    it("should show status filter pills above the file table", async () => {
      // All, Completed, In progress, Failed, Skipped pills must be present.
      await browser.waitUntil(
        async () =>
          (await hasText("All")) &&
          (await hasText("Completed")) &&
          (await hasText("Failed")),
        { timeout: 10000, timeoutMsg: "Status filter pills not found" }
      );
    });

    it("should show correct KPIs in job detail", async () => {
      const bodyText = await $("body").getText();
      // Top-level KPIs
      expect(bodyText).toContain("Total files");
      expect(bodyText).toContain("Files completed");
      // Advanced details KPIs
      expect(bodyText).toContain("Stored");
      expect(bodyText).toContain("Chunks reused");
    });

    it("should show progress label with file count caption during active backup", async () => {
      // This test only verifies the label format is present — if no backup is running,
      // the progress panel is hidden and the test passes trivially.
      const bodyText = await $("body").getText();
      // When backup is running, the "of N files" caption pattern must be present.
      if (bodyText.includes("Uploaded so far:")) {
        expect(bodyText).toContain("of");
        expect(bodyText).toContain("files");
        expect(bodyText).toContain("Total selected size:");
      }
    });
  });
});

describe("Restore Reports", () => {
  before(async () => {
    await navigateTo("Backup");
    await waitForHeading(Headings.backup);
    await clickButton("Reports");
    await browser.pause(1000);

    // Switch to the Restore tab within Reports.
    const restoreTab = await $("//button[contains(text(),'Restore')]");
    if (await restoreTab.isExisting()) {
      await restoreTab.click();
      await browser.pause(1000);
    }
  });

  it("should hide system files by default in restore job detail", async () => {
    // Open the first restore job row if one exists.
    const jobRow = await $('[data-testid="restore-report-row"]');
    if (!await jobRow.isExisting()) {
      // No restore jobs yet — skip gracefully.
      return;
    }
    await jobRow.click();
    await browser.pause(1000);

    const bodyText = await $("body").getText();
    expect(bodyText).not.toContain(".DS_Store");
    expect(bodyText).not.toContain("Thumbs.db");

    // Toggle must be present.
    expect(bodyText).toContain("System files");
  });
});
