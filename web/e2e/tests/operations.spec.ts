import { test, expect } from "@playwright/test";

test.describe("Operation Ledger UI (T02.17)", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate directly to the operations page
    await page.goto("/operations");
  });

  test("loads operation ledger page and displays operations", async ({ page }) => {
    // Verify header
    await expect(page.getByText(/Operation Ledger|অপারেশন লেজার/i).first()).toBeVisible();

    // Verify operations table rows
    await expect(page.getByText("Ping").first()).toBeVisible();
    await expect(page.getByText("PullImage").first()).toBeVisible();

    // Verify status badges in the table
    await expect(page.locator("table").getByText(/Succeeded|সফল/i).first()).toBeVisible();
    await expect(page.locator("table").getByText(/Failed|ব্যর্থ/i).first()).toBeVisible();
  });

  test("filters operations by status", async ({ page }) => {
    const statusSelect = page.locator("#statusFilter");
    await expect(statusSelect).toBeVisible();

    // Select 'failed'
    await statusSelect.selectOption("failed");

    // Only failed operations should be visible
    await expect(page.getByText("PullImage").first()).toBeVisible();
    await expect(page.getByText("Ping")).not.toBeVisible();

    // Reset filters
    const resetBtn = page.getByRole("button", { name: /Reset|রিসেট/i });
    await resetBtn.click();
    await statusSelect.selectOption("all");

    await expect(page.getByText("Ping").first()).toBeVisible();
  });

  test("opens operation details modal and inspects payload and error", async ({ page }) => {
    // Find the view details buttons
    const viewButtons = page.getByRole("button", { name: /View Payload & Logs|পেলোড ও লগ দেখুন/i });
    await expect(viewButtons.first()).toBeVisible();
    await viewButtons.first().click();

    // Verify details modal
    await expect(page.getByText(/Operation Details|অপারেশন বিস্তারিত বিবরণ/i)).toBeVisible();
    await expect(page.getByText(/Operation Payload|অপারেশন পেলোড/i)).toBeVisible();

    // Close modal
    const closeBtn = page.getByRole("button", { name: /Close|বন্ধ করুন/i });
    await closeBtn.click();
    await expect(page.getByText(/Operation Details|অপারেশন বিস্তারিত বিবরণ/i)).not.toBeVisible();
  });

  test("retries a failed operation directly from table", async ({ page }) => {
    // Look for the retry button on a failed operation
    const retryButtons = page.getByRole("button", { name: /^Retry$|^পুনরায় চেষ্টা$/i });
    await expect(retryButtons.first()).toBeVisible();
    await retryButtons.first().click();

    // Verify success toast
    await expect(page.getByText(/Operation re-queued for execution successfully|অপারেশনটি সফলভাবে পুনরায় কিউতে যুক্ত করা হয়েছে/i)).toBeVisible();
  });
});
