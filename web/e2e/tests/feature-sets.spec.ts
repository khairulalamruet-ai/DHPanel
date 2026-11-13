import { test, expect } from "@playwright/test";

test.describe("Feature Sets UI (T04.02)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/feature-sets");
  });

  test("loads feature sets page with initial seeded presets", async ({ page }) => {
    await expect(page.getByText(/Feature Sets|ফিচার সেট/i).first()).toBeVisible();

    // Verify preset sets
    await expect(page.getByText("Default (Full Stack)")).toBeVisible();
    await expect(page.getByText("Basic Web")).toBeVisible();
    await expect(page.getByText("DNS Only")).toBeVisible();
    await expect(page.getByText("Database Only")).toBeVisible();

    // Verify default badge
    await expect(page.getByText(/Default|ডিফল্ট/i).first()).toBeVisible();
  });

  test("DoD: creates a new feature set from UI dialog", async ({ page }) => {
    const newBtn = page.getByRole("button", { name: /New Feature Set|নতুন ফিচার সেট/i });
    await newBtn.click();

    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();

    const nameInput = dialog.getByLabel(/Feature Set Name|ফিচার সেটের নাম/i);
    await nameInput.fill("E2E Test Feature Set");

    const saveBtn = dialog.getByRole("button", { name: /Save Feature Set|সংরক্ষণ করুন/i });
    await saveBtn.click();

    await expect(dialog).not.toBeVisible();
    await expect(page.locator("table").getByText("E2E Test Feature Set")).toBeVisible();
  });

  test("DoD: edits an existing feature set from UI dialog", async ({ page }) => {
    const basicRow = page.locator("tr", { hasText: "Basic Web" });
    await expect(basicRow).toBeVisible();

    const editBtn = basicRow.getByRole("button", { name: /Edit|সম্পাদনা/i });
    await editBtn.click();

    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();

    const nameInput = dialog.getByLabel(/Feature Set Name|ফিচার সেটের নাম/i);
    await nameInput.fill("Basic Web E2E Renamed");

    const saveBtn = dialog.getByRole("button", { name: /Save Feature Set|সংরক্ষণ করুন/i });
    await saveBtn.click();

    await expect(dialog).not.toBeVisible();
    await expect(page.locator("table").getByText("Basic Web E2E Renamed")).toBeVisible();
  });
});
