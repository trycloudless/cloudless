/**
 * CSS selectors for the CloudLess Tauri app UI elements.
 * Based on shared_ui/src/forms.rs and leptos_ui/src/components/ Leptos components.
 */

export const Selectors = {
  // ── Auth forms ────────────────────────────────────────────────────
  nameInput: 'input[name="name"]',
  emailInput: 'input[name="email"]',
  passwordInput: 'input[name="password"]',
  submitButton: 'button[type="submit"]',
  pageTitle: "h2.gradient-text",
  errorMessage: ".text-error",
  signUpToggle: "button=Sign Up",
  logInToggle: "button=Log In",

  // ── Email verification ────────────────────────────────────────────
  verificationCodeInput: "#verification-code",

  // ── Setup wizard — Encryption ─────────────────────────────────────
  encryptionPassword: "#setup-encryption-password",
  encryptionConfirm: "#setup-confirm-password",
  continueButton: "button=Continue",

  // ── Setup wizard — Device ─────────────────────────────────────────
  deviceName: "#device-name",

  // ── Setup wizard — S3 remote storage ──────────────────────────────
  s3StorageName: "#s3-storage-name",
  s3AccessKey: "#s3-access-key",
  s3SecretKey: "#s3-secret-key",
  s3Region: "#s3-region",
  s3Bucket: "#s3-bucket",
  testConnectionButton: "button=Test Connection",

  // ── Setup wizard — Backup config ──────────────────────────────────
  sourceDirectory: "[data-testid='source-directory-input']",
  chooseFolder: "button=Choose Folder",

  // ── Navigation (sidebar / bottom nav) ─────────────────────────────
  navDashboard: "button=Dashboard",
  navFiles: "button=Files",
  navBackup: "button=Backup",
  navSettings: "button=Settings",
  lockButton: 'button[aria-label="Lock application"]',
  signOutButton: 'button[aria-label="Sign out"]',

  // ── Dashboard ─────────────────────────────────────────────────────
  backupButton: "button=Backup",
  viewReportsLink: "*=View Reports",
  viewAllLink: "*=View All",

  // ── Settings tabs ─────────────────────────────────────────────────
  tabSystem: "button=Backups",
  tabInfrastructure: "button=Storage",
  tabSecurity: "button=Security",
  tabData: "button=Retention",
  tabPreferences: "button=Preferences",

  // ── Settings — Security ───────────────────────────────────────────
  exportRecoveryKeyButton: "button=Save Recovery Key",
  rotateRecoveryKeyButton: "button=Rotate Recovery Key",
  changePasswordButton: "button=Change Password",

  // ── Settings — Data (Retention & GC) ──────────────────────────────
  runGcButton: "button=Run Garbage Collection",

  // ── Backup view tabs ──────────────────────────────────────────────
  tabBackups: "button=Backups",
  tabReports: "button=Reports",

  // ── Files view ────────────────────────────────────────────────────
  fileSearchInput: 'input[placeholder="Search files..."]',
  viewModeList: "button=List",
  viewModeFolder: "button=Folder",

  // ── Files view — bulk selection ───────────────────────────────────
  selectAllCheckbox: '[data-testid="select-all-checkbox"]',
  fileCheckbox: '[data-testid="file-checkbox"]',
  bulkActionBar: '[data-testid="bulk-action-bar"]',
  bulkRestoreButton: '[data-testid="bulk-restore-button"]',
  bulkMoveToBinButton: '[data-testid="bulk-move-to-bin-button"]',
  bulkClearButton: '[data-testid="bulk-clear-button"]',
} as const;

/**
 * Expected heading text for each page/step.
 */
export const Headings = {
  // Auth
  login: "Welcome Back",
  signup: "Create Account",

  // Email verification
  emailVerification: "Verify Your Email",

  // Setup wizard
  beforeYouContinue: "Before you continue",
  setEncryptionPassword: "Set Encryption Password",
  encryptionReady: "Encryption is ready on this device.",
  registerDevice: "Register Device",
  configureStorage: "Choose backup storage",
  configureBackup: "Choose what to protect",

  // Post-login
  unlockEncryption: "Unlock Encryption",

  // Main views
  dashboard: "Dashboard",
  files: "Files",
  backup: "Backup",
  settings: "Settings",
} as const;

// Backwards compat alias
export const AuthHeadings = Headings;

/**
 * Fill a readonly input field by setting its value via JavaScript and
 * dispatching a synthetic 'input' event so that Leptos reactive signals update.
 * Used for fields that are visually read-only but programmatically settable.
 */
export async function fillReadonlyField(
  selector: string,
  value: string
): Promise<void> {
  await browser.waitUntil(
    async () => {
      try {
        const el = await $(selector);
        await el.waitForExist({ timeout: 5000 });
        await browser.execute(
          (sel: string, val: string) => {
            const input = document.querySelector(sel) as HTMLInputElement;
            if (!input) return false;
            const setter = Object.getOwnPropertyDescriptor(
              window.HTMLInputElement.prototype,
              "value"
            )?.set;
            if (setter) setter.call(input, val);
            input.dispatchEvent(new Event("input", { bubbles: true }));
            return true;
          },
          selector,
          value
        );
        return true;
      } catch {
        return false;
      }
    },
    { timeout: 15000, interval: 500 }
  );
}

/**
 * Fill a form field by selector, clearing it first.
 */
export async function fillField(
  selector: string,
  value: string
): Promise<void> {
  // Leptos reactive updates can replace DOM nodes between waitForDisplayed and
  // setValue, causing a transient JS exception. Retry on any error.
  await browser.waitUntil(
    async () => {
      try {
        const el = await $(selector);
        await el.clearValue();
        await el.setValue(value);
        return true;
      } catch {
        return false;
      }
    },
    { timeout: 15000, interval: 500 },
  );
}

/**
 * Submit the current form by clicking the submit button.
 */
export async function submitForm(): Promise<void> {
  const btn = await $(Selectors.submitButton);
  await btn.waitForClickable();
  await btn.click();
}

/**
 * Wait for and return the page heading text.
 * Uses browser.execute to read directly from the DOM, avoiding stale-element JS
 * exceptions that occur when Leptos re-renders the heading during WASM boot.
 */
export async function getPageHeading(): Promise<string> {
  let text = "";
  await browser.waitUntil(
    async () => {
      try {
        text = await browser.execute(() => {
          const h = document.querySelector("h2.gradient-text");
          return h ? (h.textContent ?? "").trim() : "";
        });
        return text.length > 0;
      } catch {
        return false;
      }
    },
    { timeout: 30000, timeoutMsg: "Page heading did not appear within 30s" }
  );
  return text;
}

/**
 * Wait until the page heading changes to a different value.
 * Re-queries the element on each poll to avoid stale element references.
 */
export async function waitForHeadingChange(
  currentHeading: string,
  timeout = 30000
): Promise<string> {
  let newText = currentHeading;
  await browser.waitUntil(
    async () => {
      try {
        const heading = await $(Selectors.pageTitle);
        if (!(await heading.isExisting())) return false;
        newText = await heading.getText();
        return newText !== currentHeading;
      } catch {
        return false;
      }
    },
    { timeout, timeoutMsg: `Heading did not change from "${currentHeading}" within ${timeout}ms` }
  );
  return newText;
}

/**
 * Wait for a specific heading to appear.
 */
export async function waitForHeading(
  expectedHeading: string,
  timeout = 60000
): Promise<void> {
  await browser.waitUntil(
    async () => {
      try {
        const heading = await $(Selectors.pageTitle);
        if (!(await heading.isExisting())) return false;
        // Check display first — avoids JS exceptions from getText on opacity-0 elements,
        // and ensures the animate-slide-up animation has completed before we proceed.
        if (!(await heading.isDisplayed())) return false;
        const text = await heading.getText();
        return text === expectedHeading;
      } catch {
        return false;
      }
    },
    { timeout, timeoutMsg: `Heading "${expectedHeading}" did not appear within ${timeout}ms` }
  );
}

/**
 * Accepts any pending policy cards shown after auth, then waits for the next setup step.
 * E2E environments can have policy seed data, so signup tests must not assume this screen auto-skips.
 */
export async function acceptPendingPoliciesIfShown(timeout = 60000): Promise<void> {
  await browser.waitUntil(
    async () => {
      try {
        const heading = await getPageHeading();
        if (heading === Headings.setEncryptionPassword) {
          return true;
        }

        if (heading !== Headings.beforeYouContinue) {
          return false;
        }

        const acceptButton = await $("button=I Accept");
        if (await acceptButton.isClickable()) {
          await acceptButton.click();
        }

        return false;
      } catch {
        return false;
      }
    },
    {
      timeout,
      interval: 500,
      timeoutMsg: `Did not reach "${Headings.setEncryptionPassword}" after resolving pending policies`,
    }
  );
}

/**
 * Click a button by its text content.
 */
export async function clickButton(text: string): Promise<void> {
  const btn = await $(`button=${text}`);
  await btn.waitForClickable();
  await btn.click();
}

/**
 * Navigate to a main view via the sidebar.
 * Uses XPath to target sidebar buttons containing the view label.
 */
export async function navigateTo(view: "Dashboard" | "Files" | "Backup" | "Settings"): Promise<void> {
  // Desktop sidebar: <aside> with buttons containing <span>label</span>
  const navBtn = await $(`//aside//button[.//span[text()="${view}"]]`);
  if (await navBtn.isExisting()) {
    if (await navBtn.isClickable()) {
      await navBtn.click();
    }
  } else {
    // Mobile bottom nav: buttons with aria-label
    const mobileBtn = await $(`//nav//button[@aria-label="${view}"]`);
    if (await mobileBtn.isExisting() && await mobileBtn.isClickable()) {
      await mobileBtn.click();
    }
  }
  await browser.pause(500);
}

/**
 * Wait for any element on the page to contain the given text.
 * Uses XPath contains() which works reliably on WebKit.
 */
export async function waitForText(
  text: string,
  timeout = 30000
): Promise<void> {
  await browser.waitUntil(
    async () => {
      const el = await $(`//*[contains(text(),"${text}")]`);
      return el.isExisting();
    },
    { timeout, timeoutMsg: `Text "${text}" did not appear within ${timeout}ms` }
  );
}

/**
 * Check if any element on the page contains the given text.
 */
export async function hasText(text: string): Promise<boolean> {
  const el = await $(`//*[contains(text(),"${text}")]`);
  return el.isExisting();
}

/**
 * Generate a unique email for test isolation.
 */
export function uniqueEmail(): string {
  const ts = Date.now();
  const rand = Math.random().toString(36).substring(2, 8);
  return `test-${ts}-${rand}@example.com`;
}
