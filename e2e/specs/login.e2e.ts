import {
  Selectors,
  Headings,
  fillField,
  submitForm,
  getPageHeading,
  waitForHeading,
  waitForHeadingChange,
  uniqueEmail,
} from "../helpers/selectors";

const ENCRYPTION_PASSWORD = "EncryptionPass123!";

/**
 * Helper: sign up a new user and complete encryption setup so that a
 * subsequent login routes to Unlock Encryption (not the setup wizard).
 *
 * The server now returns encryption_setup_required: true when no DEK exists,
 * which causes login to route to the setup wizard instead of Unlock Encryption.
 * Completing encryption setup here stores the DEK so the test user behaves
 * as a fully-set-up account.
 */
async function createTestUser(): Promise<{ email: string; password: string }> {
  const email = uniqueEmail();
  const password = "TestPassword123!";

  // Toggle to signup
  const signUpBtn = await $(Selectors.signUpToggle);
  await signUpBtn.waitForClickable();
  await signUpBtn.click();

  await fillField(Selectors.nameInput, "Login Test User");
  await fillField(Selectors.emailInput, email);
  await fillField(Selectors.passwordInput, password);
  await submitForm();

  // With email sending disabled the server auto-verifies on signup; PolicyAcceptance
  // auto-advances when no policies are pending, so we land straight here.
  // Complete encryption setup so this user has a stored DEK.
  // Without this, login would detect encryption_setup_required: true and
  // route back to the setup wizard instead of Unlock Encryption.
  // signup_and_login includes two server-side Argon2 hashes (~30s each in
  // debug builds) plus the policy check — use a longer timeout here.
  await waitForHeading(Headings.setEncryptionPassword, 120000);
  await fillField(Selectors.encryptionPassword, ENCRYPTION_PASSWORD);
  await fillField(Selectors.encryptionConfirm, ENCRYPTION_PASSWORD);
  await submitForm();
  await waitForHeading(Headings.registerDevice);

  return { email, password };
}

describe("Login Flow", () => {
  beforeEach(async () => {
    // Re-navigate between tests: some tests (e.g. the login-success test) leave
    // the app on a different screen.  Wait specifically for the login heading
    // ("Welcome Back") using browser.execute to avoid stale-element failures
    // that occur when Leptos re-renders the heading during WASM boot.
    await browser.url("tauri://localhost");
    await browser.waitUntil(
      async () => {
        try {
          const text: string = await browser.execute(() => {
            const h = document.querySelector("h2.gradient-text");
            return h ? (h.textContent ?? "").trim() : "";
          });
          return text === Headings.login;
        } catch {
          return false;
        }
      },
      { timeout: 60000, interval: 500 },
    );
  });

  it("should display the login form by default", async () => {
    const heading = await getPageHeading();
    expect(heading).toBe(Headings.login);

    // waitForDisplayed waits up to waitforTimeout for the element to become
    // visible — more robust than isDisplayed() on cold-JIT first runs where
    // the LoginForm reactive closure may settle slightly after the before hook.
    const emailInput = await $(Selectors.emailInput);
    await emailInput.waitForDisplayed();
    const passwordInput = await $(Selectors.passwordInput);
    await passwordInput.waitForDisplayed();
    const submitButton = await $(Selectors.submitButton);
    await submitButton.waitForDisplayed();
    expect(await submitButton.getText()).toBe("Log In");

    // Name field should NOT exist on login form
    const nameInput = await $(Selectors.nameInput);
    expect(await nameInput.isExisting()).toBe(false);
  });

  it("should show error for invalid credentials", async () => {
    await fillField(Selectors.emailInput, "nonexistent@example.com");
    await fillField(Selectors.passwordInput, "WrongPassword123!");

    await submitForm();

    // Wait for error message to appear
    const errorEl = await $(Selectors.errorMessage);
    await errorEl.waitForDisplayed({
      timeout: 10000,
      timeoutMsg: "Expected error message for invalid credentials",
    });

    expect(await errorEl.isDisplayed()).toBe(true);
  });

  it("should successfully login with valid credentials", async () => {
    // First create a user via signup
    const { email, password } = await createTestUser();

    // Reload to get back to login screen
    await browser.url("tauri://localhost");
    await browser.pause(2000);

    const heading = await getPageHeading();
    expect(heading).toBe(Headings.login);

    await fillField(Selectors.emailInput, email);
    await fillField(Selectors.passwordInput, password);

    await submitForm();

    // After successful login, the app transitions to UnlockEncryption.
    const newHeading = await waitForHeadingChange(Headings.login);
    expect(newHeading).toBe(Headings.unlockEncryption);
  });

  it("should toggle from login to signup", async () => {
    const heading = await getPageHeading();
    expect(heading).toBe(Headings.login);

    // Toggle to signup — allow extra time for Leptos reactive setup on cold JIT start
    const signUpBtn = await $(Selectors.signUpToggle);
    await signUpBtn.waitForClickable({ timeout: 60000 });
    await signUpBtn.click();

    const newHeading = await getPageHeading();
    expect(newHeading).toBe(Headings.signup);

    // Verify name field IS present on signup form
    const nameInput = await $(Selectors.nameInput);
    expect(await nameInput.isDisplayed()).toBe(true);
  });
});
