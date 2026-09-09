import {
  Headings,
  waitForHeading,
  waitForText,
  hasText,
  navigateTo,
} from "../helpers/selectors";

// Runs after signup-wizard.e2e.ts in the same session — app is on dashboard.

describe("Dashboard", () => {
  before(async () => {
    await navigateTo("Dashboard");
    await waitForHeading(Headings.dashboard);
  });

  it("should display the dashboard subtitle", async () => {
    await waitForText("Your backup status at a glance");
  });

  it("should show dashboard stat cards", async () => {
    expect(await hasText("Files Protected")).toBe(true);
    expect(await hasText("Storage Used")).toBe(true);
    expect(await hasText("Active Configs")).toBe(true);
  });

  it("should show the quick actions section", async () => {
    expect(await hasText("Quick Actions")).toBe(true);
    expect(await hasText("Start Backup")).toBe(true);
  });

  it("should show the local device widget", async () => {
    expect(await hasText("This Device")).toBe(true);
  });
});
