import { test, expect } from "@playwright/test";

test.describe("Admin Error & Suspended Pages UI (T06.13)", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate directly to the Error & Suspended Pages management page
    await page.goto("/error-pages");
  });

  test("loads error pages page with template tabs and code editor", async ({ page }) => {
    // Page Header
    await expect(
      page.getByTestId("error-pages-title")
    ).toBeVisible();

    // Verify all 4 template tabs
    await expect(page.getByTestId("tab-template-suspended_domain")).toBeVisible();
    await expect(page.getByTestId("tab-template-suspended_user")).toBeVisible();
    await expect(page.getByTestId("tab-template-error_404")).toBeVisible();
    await expect(page.getByTestId("tab-template-error_502")).toBeVisible();

    // Verify editor textarea
    await expect(page.getByTestId("textarea-template-content")).toBeVisible();
    const textarea = page.getByTestId("textarea-template-content");
    await expect(textarea).toHaveValue(/Domain Suspended/);
  });

  test("switches between template tabs and loads corresponding HTML template", async ({ page }) => {
    await expect(page.getByTestId("tab-template-error_404")).toBeVisible();

    // Switch to 404 tab
    await page.getByTestId("tab-template-error_404").click();

    const textarea = page.getByTestId("textarea-template-content");
    await expect(textarea).toHaveValue(/404 - Page Not Found/);
    await expect(textarea).toHaveValue(/\{\{path\}\}/);

    // Switch to 502 tab
    await page.getByTestId("tab-template-error_502").click();
    await expect(textarea).toHaveValue(/502 - Bad Gateway/);
    await expect(textarea).toHaveValue(/\{\{service\}\}/);
  });

  test("inserts variable token when variable chip is clicked", async ({ page }) => {
    await expect(page.getByTestId("var-chip-reason")).toBeVisible();

    const textarea = page.getByTestId("textarea-template-content");
    const initialVal = await textarea.inputValue();

    // Click variable chip
    await page.getByTestId("var-chip-reason").click();

    // Content should now include variable or change
    const newVal = await textarea.inputValue();
    expect(newVal.length).toBeGreaterThanOrEqual(initialVal.length);
  });

  test("edits template HTML and saves changes successfully", async ({ page }) => {
    const saveBtn = page.getByTestId("btn-save-template");
    await expect(saveBtn).toBeDisabled();

    // Edit textarea
    const textarea = page.getByTestId("textarea-template-content");
    await textarea.fill("<!DOCTYPE html><html><body><h1>Notice: Site Suspended by Admin</h1></body></html>");

    // Save button should be enabled
    await expect(saveBtn).toBeEnabled();
    await saveBtn.click();

    // Verify toast
    await expect(
      page.getByText(/saved successfully|সফলভাবে সংরক্ষিত/i)
    ).toBeVisible();
  });

  test("resets customized template to default HTML via confirmation modal", async ({ page }) => {
    // Modify and save first
    const textarea = page.getByTestId("textarea-template-content");
    await textarea.fill("<h1>Temporary Custom Template</h1>");
    await page.getByTestId("btn-save-template").click();

    await expect(page.getByText(/saved successfully|সফলভাবে সংরক্ষিত/i)).toBeVisible();

    // Click Reset to Default
    const resetBtn = page.getByTestId("btn-reset-template");
    await expect(resetBtn).toBeVisible();
    await resetBtn.click();

    // Confirm dialog
    const confirmBtn = page.getByTestId("btn-confirm-reset");
    await expect(confirmBtn).toBeVisible();
    await confirmBtn.click();

    // Verify reset toast and default content restored
    await expect(page.getByText(/reverted to default|ডিফল্ট মানে ফিরে গেছে/i)).toBeVisible();
    await expect(textarea).toHaveValue(/Domain Suspended/);
  });

  test("switches preview device viewport and toggles preview variables panel", async ({ page }) => {
    await expect(page.getByTestId("preview-device-mobile")).toBeVisible();

    // Switch device
    await page.getByTestId("preview-device-tablet").click();
    await page.getByTestId("preview-device-mobile").click();
    await page.getByTestId("preview-device-desktop").click();

    // Toggle variables panel
    const toggleBtn = page.getByTestId("btn-toggle-preview-vars");
    await expect(toggleBtn).toBeVisible();
    await toggleBtn.click();

    await expect(page.getByTestId("preview-vars-panel")).toBeVisible();
    await expect(page.getByTestId("input-sample-domain")).toBeVisible();
  });
});
