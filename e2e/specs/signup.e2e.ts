import {
  Selectors,
  Headings,
  fillField,
  submitForm,
  getPageHeading,
  acceptPendingPoliciesIfShown,
  uniqueEmail,
} from "../helpers/selectors";

describe("Signup Flow", () => {
  beforeEach(async () => {
    await browser.url("tauri://localhost");
    // Re-query the heading on every poll to avoid stale-element failures when
    // Leptos remounts the DOM during WASM boot (same pattern as login.e2e.ts).
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
    // Wait for slide-in animations to complete so toggle buttons are clickable.
    await browser.pause(2000);
  });

  it("should display the signup form when toggled", async () => {
    const signUpBtn = await $(Selectors.signUpToggle);
    await signUpBtn.waitForClickable();
    await signUpBtn.click();

    const heading = await getPageHeading();
    expect(heading).toBe(Headings.signup);

    const nameInput = await $(Selectors.nameInput);
    const emailInput = await $(Selectors.emailInput);
    const passwordInput = await $(Selectors.passwordInput);
    const submitButton = await $(Selectors.submitButton);

    expect(await nameInput.isDisplayed()).toBe(true);
    expect(await emailInput.isDisplayed()).toBe(true);
    expect(await passwordInput.isDisplayed()).toBe(true);
    expect(await submitButton.isDisplayed()).toBe(true);
    expect(await submitButton.getText()).toBe("Sign Up");
  });

  it("should toggle from signup to login", async () => {
    const signUpBtn = await $(Selectors.signUpToggle);
    await signUpBtn.waitForClickable();
    await signUpBtn.click();

    expect(await getPageHeading()).toBe(Headings.signup);

    const logInBtn = await $(Selectors.logInToggle);
    await logInBtn.waitForClickable();
    await logInBtn.click();

    expect(await getPageHeading()).toBe(Headings.login);

    const nameInput = await $(Selectors.nameInput);
    expect(await nameInput.isExisting()).toBe(false);
  });

  it("should successfully sign up and reach encryption setup", async () => {
    const signUpBtn = await $(Selectors.signUpToggle);
    await signUpBtn.waitForClickable();
    await signUpBtn.click();

    const email = uniqueEmail();
    await fillField(Selectors.nameInput, "Test User");
    await fillField(Selectors.emailInput, email);
    await fillField(Selectors.passwordInput, "SecurePassword123!");
    await submitForm();

    // With email sending disabled the server auto-verifies email on signup. If policy
    // seed data exists, the app shows PolicyAcceptance before encryption setup.
    await acceptPendingPoliciesIfShown();
    expect(await getPageHeading()).toBe(Headings.setEncryptionPassword);
  });
});
