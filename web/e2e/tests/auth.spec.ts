import { test, expect } from "@playwright/test";

test.describe("Authentication Flow (T01.14 DoD: login -> dashboard -> logout)", () => {
  test("performs login, reaches dashboard, and logs out successfully", async ({ page }) => {
    // 1. Visit login page
    await page.goto("/login");

    // Verify login form is visible
    const usernameInput = page.locator("input[name='username']");
    const passwordInput = page.locator("input[name='password']");
    const submitBtn = page.locator("button[type='submit']");

    await expect(usernameInput).toBeVisible();
    await expect(passwordInput).toBeVisible();

    // 2. Fill credentials and submit
    await usernameInput.fill("admin");
    await passwordInput.fill("admin-secret-password");
    await submitBtn.click();

    // 3. Reaches Dashboard
    await expect(page.getByText(/সিস্টেম অবস্থা|System Health/i).first()).toBeVisible();

    // 4. Click Logout button
    const logoutBtn = page.locator("button[aria-label='লগআউট'], button[aria-label='Logout']").first();
    await expect(logoutBtn).toBeVisible();
    await logoutBtn.click();

    // 5. Returns to Login Page
    await expect(page.locator("input[name='username']")).toBeVisible();
  });

  test("supports 2FA verification step for MFA-enabled accounts", async ({ page }) => {
    await page.goto("/login");

    const usernameInput = page.locator("input[name='username']");
    const passwordInput = page.locator("input[name='password']");
    const submitBtn = page.locator("button[type='submit']");

    // Enter 2FA user
    await usernameInput.fill("admin_2fa");
    await passwordInput.fill("valid-password");
    await submitBtn.click();

    // Step 2: 2FA TOTP code input appears
    const totpInput = page.locator("input[name='totpCode']");
    await expect(totpInput).toBeVisible();

    // Enter 6-digit TOTP code
    await totpInput.fill("123456");
    await submitBtn.click();

    // Reaches Dashboard
    await expect(page.getByText(/সিস্টেম অবস্থা|System Health/i).first()).toBeVisible();
  });
});
