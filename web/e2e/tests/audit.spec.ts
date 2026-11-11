import { test, expect } from "@playwright/test";

test.describe("Audit Viewer UI (T01.16)", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate directly to the audit viewer page
    await page.goto("/audit");
  });

  test("loads audit logs page and displays table records", async ({ page }) => {
    // Header should be visible
    await expect(page.getByText(/Audit Logs|অডিট লগ/i).first()).toBeVisible();

    // Filter section should be visible
    await expect(page.getByText(/Filter Logs|ফিল্টার করুন/i).first()).toBeVisible();

    // Table rows should render initial logs (e.g. auth.login)
    await expect(page.getByText("auth.login").first()).toBeVisible();
    await expect(page.getByText("vhost.create").first()).toBeVisible();
  });

  test("filters audit logs by action or outcome", async ({ page }) => {
    const actionInput = page.locator("#actionFilter");
    await expect(actionInput).toBeVisible();

    // Filter by 'vhost'
    await actionInput.fill("vhost");
    const applyBtn = page.getByRole("button", { name: /Apply Filters|ফিল্টার প্রয়োগ করুন/i });
    await applyBtn.click();

    // Only vhost should be present in results
    await expect(page.getByText("vhost.create").first()).toBeVisible();
    await expect(page.getByText("auth.login")).not.toBeVisible();

    // Reset filters
    const resetBtn = page.getByRole("button", { name: /Reset Filters|রিসেট করুন/i });
    await resetBtn.click();
    await applyBtn.click();

    await expect(page.getByText("auth.login").first()).toBeVisible();
  });

  test("opens JSON metadata dialog and displays detailed audit event data", async ({ page }) => {
    // Find the first View JSON button
    const viewButtons = page.getByRole("button", { name: /View JSON|জেসন দেখুন/i });
    await expect(viewButtons.first()).toBeVisible();
    await viewButtons.first().click();

    // Modal should be visible
    await expect(page.getByText(/Audit Entry Details|অডিট এন্ট্রি বিবরণ/i)).toBeVisible();
    await expect(page.getByText(/Structured JSON Metadata|স্ট্রাকচার্ড জেসন মেটাডাটা/i)).toBeVisible();

    // Verify metadata pre block is visible
    const jsonBlock = page.locator("pre");
    await expect(jsonBlock).toBeVisible();

    // Close dialog
    const closeBtn = page.getByRole("button", { name: /Close|বন্ধ করুন/i });
    await closeBtn.click();
    await expect(page.getByText(/Audit Entry Details|অডিট এন্ট্রি বিবরণ/i)).not.toBeVisible();
  });

  test("supports pagination between log pages", async ({ page }) => {
    const nextBtn = page.getByRole("button", { name: /Next|পরবর্তী/i });
    await expect(nextBtn).toBeVisible();

    if (await nextBtn.isEnabled()) {
      await nextBtn.click();
      // Page 2 indicator
      await expect(page.getByText(/(?:Page|পৃষ্ঠা)\s*2/i)).toBeVisible();

      const prevBtn = page.getByRole("button", { name: /Previous|পূর্ববর্তী/i });
      await expect(prevBtn).toBeEnabled();
      await prevBtn.click();
      await expect(page.getByText(/(?:Page|পৃষ্ঠা)\s*1/i)).toBeVisible();
    }
  });
});
