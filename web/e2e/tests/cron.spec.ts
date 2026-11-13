import { test, expect } from "@playwright/test";

test.describe("Cron Jobs Management UI (T10.03)", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate directly to cron page
    await page.goto("/cron");
  });

  test("loads cron jobs page with metrics, table, and actions", async ({ page }) => {
    // Page Title
    await expect(page.getByTestId("cron-page-title")).toBeVisible();

    // Summary metric cards
    await expect(page.getByTestId("card-total-cron")).toBeVisible();
    await expect(page.getByTestId("card-active-cron")).toBeVisible();
    await expect(page.getByTestId("card-failed-cron")).toBeVisible();
    await expect(page.getByTestId("card-next-cron")).toBeVisible();

    // Action button
    await expect(page.getByTestId("btn-open-create-cron")).toBeVisible();

    // Seed cron jobs
    await expect(page.getByTestId("cron-name-cron_01")).toBeVisible();
    await expect(page.getByTestId("cron-name-cron_02")).toBeVisible();
    await expect(page.getByTestId("cron-name-cron_03")).toBeVisible();
  });

  test("DoD E2E: creates cron job with preset, toggles state, executes 'Run Now', and inspects terminal output", async ({ page }) => {
    // 1. Create a new cron job with preset
    await page.getByTestId("btn-open-create-cron").click();
    await expect(page.getByTestId("create-cron-dialog-title")).toBeVisible();

    await page.getByTestId("input-cron-name").fill("Automated Feed Refresher");
    await page.getByTestId("input-cron-command").fill("php /home/demo/artisan feeds:refresh");

    // Select "Hourly" preset
    await page.getByTestId("preset-cron-2").click();
    await expect(page.getByTestId("input-cron-schedule")).toHaveValue("0 * * * *");

    // Submit form
    await page.getByTestId("btn-submit-cron").click();
    await expect(page.getByTestId("create-cron-dialog-title")).not.toBeVisible();
    await expect(
      page.getByText(/Cron job created successfully|ক্রন জব সফলভাবে তৈরি/i)
    ).toBeVisible();

    // 2. Toggle job enabled state
    const toggleBtn = page.getByTestId("btn-toggle-cron-cron_01");
    await expect(toggleBtn).toBeVisible();
    await toggleBtn.click();
    await expect(
      page.getByText(/Cron job disabled|Cron job enabled|ক্রন জব সক্রিয়|নিষ্ক্রিয়/i)
    ).toBeVisible();

    // 3. Execute "Run Now"
    const runBtn = page.getByTestId("btn-run-cron-cron_01");
    await expect(runBtn).toBeVisible();
    await runBtn.click();
    await expect(
      page.getByText(/Cron job executed successfully|ক্রন জব .* সফলভাবে সম্পন্ন/i)
    ).toBeVisible();

    // 4. View Terminal Output
    const viewOutputBtn = page.getByTestId("btn-view-output-cron_01");
    await expect(viewOutputBtn).toBeVisible();
    await viewOutputBtn.click();

    await expect(page.getByTestId("cron-output-modal-title")).toBeVisible();
    await expect(page.getByTestId("cron-output-content")).toBeVisible();
    await expect(page.getByTestId("btn-copy-cron-output")).toBeVisible();

    // Close output modal
    await page.getByTestId("btn-close-cron-output").click();
    await expect(page.getByTestId("cron-output-modal-title")).not.toBeVisible();
  });
});
