import { test, expect } from "@playwright/test";

test.describe("Server Migration & Account Import UI (T15.06)", () => {
  test("Admin/Client: views transfer progress, inspects live logs & step timeline, and initiates remote cPanel migration", async ({
    page,
  }) => {
    // 1. Navigate to Migration & Transfers page
    await page.goto("/admin/migration");

    await expect(page.getByTestId("migration-page-title")).toBeVisible();
    await expect(page.getByTestId("migration-stats-grid")).toBeVisible();
    await expect(page.getByTestId("stat-active-transfers")).toBeVisible();
    await expect(page.getByTestId("stat-completed-transfers")).toBeVisible();

    // 2. Transfers list & Inspect live transfer logs (OpenPanel /import/transfers reference)
    await expect(page.getByTestId("transfers-table-section")).toBeVisible();
    await expect(page.getByTestId("transfer-row-TRF-90241A")).toBeVisible();
    await expect(page.getByTestId("transfer-badge-TRF-90241A")).toContainText("RUNNING");

    // Open Live Log Modal
    const viewLogsBtn = page.getByTestId("btn-view-logs-TRF-90241A");
    await viewLogsBtn.click();

    await expect(page.getByTestId("transfer-log-dialog")).toBeVisible();
    await expect(page.getByTestId("transfer-step-timeline")).toBeVisible();
    await expect(page.getByTestId("transfer-log-console")).toBeVisible();
    await expect(page.getByTestId("transfer-log-console")).toContainText("cpmove");

    // Test log copying action
    const copyLogBtn = page.getByTestId("btn-copy-transfer-logs");
    await expect(copyLogBtn).toBeVisible();

    // Close log modal
    await page.getByTestId("btn-close-log-dialog").click();
    await expect(page.getByTestId("transfer-log-dialog")).not.toBeVisible();

    // 3. Initiate New Migration Transfer Wizard
    const openWizardBtn = page.getByTestId("btn-open-new-transfer");
    await openWizardBtn.click();

    await expect(page.getByTestId("new-transfer-dialog")).toBeVisible();

    // Choose cPanel platform
    await page.getByTestId("source-type-cpanel").click();

    // Fill SSH remote server connection
    await page.getByTestId("input-migration-host").fill("198.51.100.25");
    await page.getByTestId("input-migration-ssh-user").fill("root");
    await page.getByTestId("input-migration-password").fill("SuperSecretP@ssword123");

    // Fill Destination Tenant Mapping
    await page.getByTestId("input-migration-target-domain").fill("imported-store.com");
    await page.getByTestId("input-migration-tenant-user").fill("storeuser");

    // Submit Migration Transfer
    await page.getByTestId("btn-start-transfer").click();

    // Verify submission succeeds and log dialog or new row appears
    await expect(page.getByTestId("transfer-log-dialog")).toBeVisible();
    await expect(page.getByTestId("transfer-dialog-title")).toBeVisible();
    await page.getByTestId("btn-close-log-dialog").click();

    // Verify new transfer appears in table
    await expect(
      page.getByTestId("transfers-table-section").getByText("imported-store.com")
    ).toBeVisible();
  });
});
