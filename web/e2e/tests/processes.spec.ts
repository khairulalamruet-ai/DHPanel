import { test, expect } from "@playwright/test";

test.describe("Process Manager UI (T10.14)", () => {
  test("Client: manages container processes, terminates via SIGTERM, force kills via SIGKILL, and verifies isolation (DoD)", async ({ page }) => {
    // 1. Navigate to client processes page
    await page.goto("/processes");

    await expect(page.getByTestId("processes-page-title")).toBeVisible();

    // Summary cards
    await expect(page.getByTestId("card-proc-total")).toBeVisible();
    await expect(page.getByTestId("card-proc-cpu")).toBeVisible();
    await expect(page.getByTestId("card-proc-ram")).toBeVisible();

    // Processes table
    await expect(page.getByTestId("process-row-14")).toBeVisible();
    await expect(page.getByTestId("proc-cmd-14")).toContainText("php-fpm: pool www");

    // DoD Requirement: Client never sees other tenants or host processes
    await expect(page.locator("text=dockerd")).not.toBeVisible();
    await expect(page.locator("text=acme")).not.toBeVisible();

    // 2. Terminate process via SIGTERM (graceful)
    const sigtermBtn = page.getByTestId("btn-sigterm-14");
    await expect(sigtermBtn).toBeVisible();
    await sigtermBtn.click();

    await expect(page.getByTestId("kill-process-dialog-title")).toBeVisible();
    await expect(page.getByTestId("radio-signal-sigterm")).toBeVisible();
    await page.getByTestId("btn-confirm-kill-process").click();

    await expect(page.getByTestId("kill-process-dialog-title")).not.toBeVisible();
    await expect(
      page.getByText(/Signal SIGTERM sent|সিগন্যাল SIGTERM পাঠানো/i)
    ).toBeVisible();

    // 3. Force kill process via SIGKILL
    const sigkillBtn = page.getByTestId("btn-sigkill-15");
    await expect(sigkillBtn).toBeVisible();
    await sigkillBtn.click();

    await expect(page.getByTestId("kill-process-dialog-title")).toBeVisible();
    await expect(page.getByTestId("radio-signal-sigkill")).toBeVisible();
    await page.getByTestId("btn-confirm-kill-process").click();

    await expect(page.getByTestId("kill-process-dialog-title")).not.toBeVisible();
    await expect(
      page.getByText(/Signal SIGKILL sent|সিগন্যাল SIGKILL পাঠানো/i)
    ).toBeVisible();
  });

  test("Admin: inspects host and cluster processes, filters tenant, and executes signal", async ({ page }) => {
    // 1. Navigate to admin processes page
    await page.goto("/admin/processes");

    await expect(page.getByTestId("admin-processes-title")).toBeVisible();
    await expect(page.getByTestId("card-admin-proc-total")).toBeVisible();

    // Verify host processes are visible in admin
    await expect(page.getByTestId("admin-process-row-412")).toBeVisible();
    await expect(page.getByTestId("admin-proc-cmd-412")).toContainText("dockerd");

    // 2. Filter by tenant
    await page.getByTestId("select-admin-proc-tenant").selectOption("acme");
    await expect(page.getByTestId("admin-process-row-2200")).toBeVisible();
    await expect(page.getByTestId("admin-process-row-412")).not.toBeVisible();

    // 3. Open kill dialog with sudo password
    await page.getByTestId("btn-admin-sigterm-2200").click();
    await expect(page.getByTestId("kill-process-dialog-title")).toBeVisible();
    await expect(page.getByTestId("input-sudo-password")).toBeVisible();

    await page.getByTestId("input-sudo-password").fill("admin_secret_sudo");
    await page.getByTestId("btn-confirm-kill-process").click();

    await expect(page.getByTestId("kill-process-dialog-title")).not.toBeVisible();
  });
});
