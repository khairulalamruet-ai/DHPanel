import { test, expect } from "@playwright/test";

test.describe("Module Manager UI (T03.12)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/modules");
  });

  test("loads module manager page with module list", async ({ page }) => {
    // Verify page title
    await expect(page.getByText(/Module Manager|মডিউল ম্যানেজার/i).first()).toBeVisible();

    // Verify presence of table headers
    await expect(page.getByText(/Module|মডিউল/i).first()).toBeVisible();
    await expect(page.getByText(/Category|ক্যাটাগরি/i).first()).toBeVisible();
    await expect(page.getByText(/Version|ভার্সন/i).first()).toBeVisible();
    await expect(page.getByText(/Dependencies|ডিপেন্ডেন্সি/i).first()).toBeVisible();

    // Verify key modules are listed
    await expect(page.getByText(/DNS Manager|ডিএনএস ম্যানেজার/i)).toBeVisible();
    await expect(page.getByText(/Domains|ডোমেইন/i).first()).toBeVisible();
    await expect(page.getByText(/Web Server|ওয়েব সার্ভার/i)).toBeVisible();
  });

  test("core modules cannot be disabled", async ({ page }) => {
    const authRow = page.locator("tr", { hasText: /Authentication|প্রমাণীকরণ/i });
    await expect(authRow).toBeVisible();

    const coreBtn = authRow.getByRole("button", { name: /Core|কোর/i });
    await expect(coreBtn).toBeVisible();
    await expect(coreBtn).toBeDisabled();
  });

  test("filters modules by category pill", async ({ page }) => {
    const dbFilterBtn = page.getByRole("button", { name: /Database|ডাটাবেস/i });
    await dbFilterBtn.click();

    await expect(page.getByText(/MySQL \/ MariaDB|মাইএসকিউএল \/ মারিয়াডিবি/i)).toBeVisible();
    await expect(page.getByText(/DNS Manager|ডিএনএস ম্যানেজার/i)).not.toBeVisible();

    // Reset to all
    const allFilterBtn = page.getByRole("button", { name: /All|সকল/i });
    await allFilterBtn.click();
    await expect(page.getByText(/DNS Manager|ডিএনএস ম্যানেজার/i)).toBeVisible();
  });

  test("DoD: disabling dns displays dependency warning for domains", async ({ page }) => {
    // Locate the DNS row and click its disable toggle button
    const dnsRow = page.locator("tr", { hasText: /DNS Manager|ডিএনএস ম্যানেজার/i });
    await expect(dnsRow).toBeVisible();

    const disableBtn = dnsRow.getByRole("button", { name: /Disable|বন্ধ করুন/i });
    await expect(disableBtn).toBeVisible();
    await disableBtn.click();

    // DoD: Verify dependency warning modal appears mentioning domains
    const warningModal = page.locator("[role='dialog']");
    await expect(warningModal).toBeVisible();
    await expect(
      warningModal.getByText(/Disable Module & Dependent Services\?|মডিউল ও নির্ভরশীল সার্ভিস বন্ধ করবেন\?/i)
    ).toBeVisible();
    await expect(warningModal.getByText("(domains)")).toBeVisible();

    // Click confirm in modal
    const confirmBtn = warningModal.getByRole("button", {
      name: /Confirm & Disable|নিশ্চিত করে বন্ধ করুন/i,
    });
    await confirmBtn.click();

    // Modal should close
    await expect(warningModal).not.toBeVisible();

    // Verify DNS button is now Enable
    await expect(dnsRow.getByRole("button", { name: /Enable|চালু করুন/i })).toBeVisible();
  });
});
