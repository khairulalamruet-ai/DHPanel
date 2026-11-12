import { test, expect } from "@playwright/test";

test.describe("PHP Runtime & Directives Management UI (T06.11)", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate directly to the PHP management page
    await page.goto("/php");
  });

  test("loads PHP management page with metric cards and version selector", async ({ page }) => {
    // Page Header
    await expect(
      page.getByRole("heading", { level: 1, name: /PHP Configuration & Runtime|পিএইচপি কনফিগারেশন ও রানটাইম/i })
    ).toBeVisible();

    // Metric Summary Cards
    await expect(page.getByTestId("metric-php-default-version")).toContainText("PHP 8.3");
    await expect(page.getByTestId("metric-php-memory-limit")).toContainText("256M");

    // Default selector
    await expect(page.getByTestId("select-default-php-version")).toBeVisible();

    // Domains table
    await expect(page.getByTestId("row-domain-php-dom_01")).toBeVisible();
    await expect(page.getByText("alpha-hosting.com", { exact: true })).toBeVisible();
  });

  test("updates account default PHP version via dropdown selector", async ({ page }) => {
    await expect(page.getByTestId("select-default-php-version")).toBeVisible();

    // Select PHP 8.4
    await page.getByTestId("select-default-php-version").selectOption("8.4");

    // Click Apply Default
    const applyBtn = page.getByTestId("btn-update-default-php");
    await expect(applyBtn).toBeEnabled();
    await applyBtn.click();

    // Metric card updates to PHP 8.4
    await expect(page.getByTestId("metric-php-default-version")).toContainText("PHP 8.4");
  });

  test("overrides PHP version for a specific domain", async ({ page }) => {
    await expect(page.getByTestId("select-domain-php-dom_01")).toBeVisible();

    // Change version for dom_01 to 8.2
    await page.getByTestId("select-domain-php-dom_01").selectOption("8.2");

    // Check row updates
    await expect(page.getByTestId("row-domain-php-dom_01")).toContainText("PHP 8.2");
  });

  test("switches to php.ini tab and modifies resource limit", async ({ page }) => {
    await expect(page.getByTestId("tab-php-ini")).toBeVisible();

    // Click php.ini tab
    await page.getByTestId("tab-php-ini").click();

    // Verify form is visible
    await expect(page.getByTestId("php-ini-form-card")).toBeVisible();
    const memInput = page.getByTestId("input-memory-limit");
    await expect(memInput).toHaveValue("256M");

    // Change memory limit to 512M
    await memInput.fill("512M");
    await page.getByTestId("btn-save-php-ini").click();

    // Value persists
    await expect(memInput).toHaveValue("512M");
  });

  test("DoD: resets php.ini directives to hosting plan defaults", async ({ page }) => {
    await expect(page.getByTestId("tab-php-ini")).toBeVisible();
    await page.getByTestId("tab-php-ini").click();

    const memInput = page.getByTestId("input-memory-limit");
    await expect(memInput).toBeVisible();

    // Change memory limit
    await memInput.fill("384M");
    await expect(memInput).toHaveValue("384M");

    // Click Reset to Defaults button
    await page.getByTestId("btn-reset-php-ini").click();

    // Form value reverts to 256M
    await expect(memInput).toHaveValue("256M");
  });
});
