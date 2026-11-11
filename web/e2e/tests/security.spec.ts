import { test, expect } from "@playwright/test";

test.describe("Account Security UI (T01.15)", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate to security page directly (admin session bootstrap is authenticated in dev/mock)
    await page.goto("/security");
  });

  test("loads account security page with all tabs", async ({ page }) => {
    // Verify page header
    await expect(page.getByText(/Account Security|অ্যাকাউন্ট নিরাপত্তা/i).first()).toBeVisible();

    // Verify tabs
    await expect(page.getByRole("tab", { name: /Password|পাসওয়ার্ড/i })).toBeVisible();
    await expect(page.getByRole("tab", { name: /Two-Factor Auth|2FA নিরাপত্তা/i })).toBeVisible();
    await expect(page.getByRole("tab", { name: /Active Sessions|সক্রিয় সেশন/i })).toBeVisible();
    await expect(page.getByRole("tab", { name: /Login History|লগইন ইতিহাস/i })).toBeVisible();
  });

  test("changes account password with validation", async ({ page }) => {
    const currentInput = page.locator("input[name='currentPassword']");
    const newInput = page.locator("input[name='newPassword']");
    const confirmInput = page.locator("input[name='confirmPassword']");
    const submitBtn = page.locator("button[type='submit']");

    await expect(currentInput).toBeVisible();

    // Fill valid password update
    await currentInput.fill("admin-secret-password");
    await newInput.fill("new-strong-password-2026");
    await confirmInput.fill("new-strong-password-2026");
    await submitBtn.click();

    // Expect success message
    await expect(page.getByText(/Password updated successfully|পাসওয়ার্ড সফলভাবে পরিবর্তন করা হয়েছে/i)).toBeVisible();
  });

  test("performs 2FA enrollment, TOTP verification, and displays backup codes", async ({ page }) => {
    // Switch to 2FA tab
    await page.getByRole("tab", { name: /Two-Factor Auth|2FA নিরাপত্তা/i }).click();

    // Click Enable 2FA
    const enableBtn = page.getByRole("button", { name: /Enable 2FA|2FA চালু করুন/i });
    await expect(enableBtn).toBeVisible();
    await enableBtn.click();

    // Verify QR code area and secret key appear
    await expect(page.getByText(/JBSWY3DPEHPK3PXP/)).toBeVisible();

    // Fill 6-digit TOTP code
    const totpInput = page.locator("input[name='totpInput']");
    await expect(totpInput).toBeVisible();
    await totpInput.fill("123456");

    // Click Verify & Enable
    const verifyBtn = page.getByRole("button", { name: /Verify & Enable|যাচাই ও চালু করুন/i });
    await verifyBtn.click();

    // Verify 2FA status is now ON / Enabled
    await expect(page.getByText(/ON|Enabled|চালু আছে/i).first()).toBeVisible();

    // Verify Backup codes are displayed
    await expect(page.getByText(/Backup Codes|ব্যাকআপ কোড/i).first()).toBeVisible();
    await expect(page.getByText(/a1b2-c3d4/)).toBeVisible();

    // Test Copy All button
    const copyBtn = page.getByRole("button", { name: /Copy All|সব কপি করুন/i }).first();
    await expect(copyBtn).toBeVisible();
  });

  test("displays active sessions and allows revoking non-current session", async ({ page }) => {
    // Switch to Active Sessions tab
    await page.getByRole("tab", { name: /Active Sessions|সক্রিয় সেশন/i }).click();

    // Verify current session badge
    await expect(page.getByText(/Current Session|বর্তমান সেশন/i).first()).toBeVisible();

    // Verify IP addresses in list
    await expect(page.getByText("192.168.1.105")).toBeVisible();
    await expect(page.getByText("203.0.113.42")).toBeVisible();

    // Click Revoke on non-current session (exact match, not revoke-others)
    const revokeBtn = page.getByRole("button", { name: /^Revoke$|^বাতিল করুন$/i }).first();
    await expect(revokeBtn).toBeVisible();
    await revokeBtn.click();

    // Verify success feedback and that revoked session is removed
    await expect(page.getByText(/Session revoked successfully|সেশনটি সফলভাবে বাতিল করা হয়েছে/i)).toBeVisible();
    await expect(page.getByText("203.0.113.42")).not.toBeVisible();
  });

  test("displays login events history", async ({ page }) => {
    // Switch to Login History tab
    await page.getByRole("tab", { name: /Login History|লগইন ইতিহাস/i }).click();

    // Verify event records
    await expect(page.getByText(/Successful|সফল লগইন/i).first()).toBeVisible();
    await expect(page.getByText("192.168.1.105").first()).toBeVisible();
    await expect(page.getByText("185.220.101.5")).toBeVisible();
  });
});
