import { test, expect } from "@playwright/test";

test.describe("Client Backup & Restore UI (T09.09)", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate directly to client backups page
    await page.goto("/backups");
  });

  test("loads backups page with metrics, table, and actions", async ({ page }) => {
    // Page Title
    await expect(page.getByTestId("backups-page-title")).toBeVisible();

    // Summary metric cards
    await expect(page.getByTestId("card-metric-total-backups")).toBeVisible();
    await expect(page.getByTestId("card-metric-storage-used")).toBeVisible();
    await expect(page.getByTestId("card-metric-last-backup")).toBeVisible();

    // Action button
    await expect(page.getByTestId("btn-open-create-backup")).toBeVisible();

    // Seed backups
    await expect(page.getByTestId("backup-name-bk_01")).toBeVisible();
    await expect(page.getByTestId("backup-name-bk_02")).toBeVisible();
  });

  test("DoD E2E: triggers new backup, executes restore wizard, and deletes backup", async ({ page }) => {
    // Handle dialog confirmations
    page.on("dialog", (dialog) => dialog.accept());

    // 1. Take a backup now
    await page.getByTestId("btn-open-create-backup").click();
    await expect(page.getByTestId("create-backup-dialog-title")).toBeVisible();

    // Pick databases only
    await page.getByTestId("type-select-databases").click();
    await page.getByTestId("input-backup-note").fill("Automated E2E test backup");
    await page.getByTestId("btn-submit-create-backup").click();

    // Verify dialog closes and toast shows
    await expect(page.getByTestId("create-backup-dialog-title")).not.toBeVisible();
    await expect(
      page.getByText(/Backup creation started|ব্যাকআপ তৈরির প্রক্রিয়া/i)
    ).toBeVisible();

    // 2. Open Restore Wizard for first backup
    await expect(page.getByTestId("btn-restore-bk_01")).toBeVisible();
    await page.getByTestId("btn-restore-bk_01").click();

    await expect(page.getByTestId("restore-wizard-title")).toBeVisible();

    // Verify safety snapshot checkbox is present
    await expect(page.getByTestId("checkbox-safety-snapshot")).toBeChecked();

    // Submit restore
    await page.getByTestId("btn-submit-restore").click();
    await expect(page.getByTestId("restore-wizard-title")).not.toBeVisible();
    await expect(
      page.getByText(/Restore job started|রিস্টোর প্রক্রিয়া/i)
    ).toBeVisible();

    // 3. Delete backup
    await expect(page.getByTestId("btn-delete-backup-bk_02")).toBeVisible();
    await page.getByTestId("btn-delete-backup-bk_02").click();

    // Verify removed
    await expect(page.getByTestId("backup-name-bk_02")).not.toBeVisible();
  });
});
