import { test, expect } from "@playwright/test";

test.describe("Panel Update & Rollback System (T18.10)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/update");
  });

  test("loads update manager, switches channel, and checks for updates", async ({ page }) => {
    // 1. Verify card and page title
    await expect(page.getByTestId("panel-update-page")).toBeVisible();
    await expect(page.getByTestId("current-version-display")).toContainText("0.1.0");

    // 2. Switch to Beta channel
    const betaBtn = page.getByTestId("channel-beta-btn");
    await betaBtn.click();
    await expect(page.getByText(/0\.3\.0-beta\.1/)).toBeVisible();

    // 3. Switch back to Stable channel
    const stableBtn = page.getByTestId("channel-stable-btn");
    await stableBtn.click();
    await expect(page.getByText(/0\.2\.0/)).toBeVisible();

    // 4. Click Check for Updates
    const checkBtn = page.getByTestId("btn-check-updates");
    await checkBtn.click();
    await expect(page.getByTestId("update-available-banner")).toBeVisible();
  });

  test("executes update failure with automatic rollback to previous version (DoD)", async ({
    page,
  }) => {
    // 1. Ensure update is available
    await expect(page.getByTestId("update-available-banner")).toBeVisible();

    // 2. Check simulate failure checkbox
    const simulateCheckbox = page.getByTestId("simulate-failure-checkbox");
    await simulateCheckbox.check();

    // 3. Click Update Now
    const updateBtn = page.getByTestId("btn-update-now");
    await updateBtn.click();

    // 4. Progress bar appears
    await expect(page.getByTestId("update-progress-container")).toBeVisible();

    // 5. Verifies DoD: Update fails and automatically rolls back to previous version
    await expect(page.getByTestId("update-step-message")).toContainText(
      /Update failed|Automatic rollback completed|স্বয়ংক্রিয় রোলব্যাক/i,
      { timeout: 15000 }
    );

    // 6. Current version remains 0.1.0
    await expect(page.getByTestId("current-version-display")).toContainText("0.1.0");
  });

  test("applies update successfully and supports manual rollback", async ({ page }) => {
    // 1. Ensure simulate failure is unchecked
    const simulateCheckbox = page.getByTestId("simulate-failure-checkbox");
    await simulateCheckbox.uncheck();

    // 2. Click Update Now
    const updateBtn = page.getByTestId("btn-update-now");
    await updateBtn.click();

    // 3. Verifies update completion
    await expect(page.getByTestId("update-step-message")).toContainText(
      /successfully applied|সফলভাবে/i,
      { timeout: 15000 }
    );

    // 4. Current version is now 0.2.0
    await expect(page.getByTestId("current-version-display")).toContainText("0.2.0");

    // 5. Manual rollback button is now available to return to 0.1.0
    const rollbackBtn = page.getByTestId("btn-manual-rollback");
    await expect(rollbackBtn).toBeVisible();
    await rollbackBtn.click();

    // 6. Restored to 0.1.0
    await expect(page.getByTestId("current-version-display")).toContainText("0.1.0");
  });
});
