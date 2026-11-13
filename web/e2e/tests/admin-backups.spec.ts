import { test, expect } from "@playwright/test";

test.describe("Admin Backup UI (T09.10)", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate directly to admin backups page
    await page.goto("/admin/backups");
  });

  test("loads admin backups overview, job history, and failed job notification banner", async ({ page }) => {
    // Page Title
    await expect(page.getByTestId("admin-backups-title")).toBeVisible();

    // Overview cards
    await expect(page.getByTestId("card-total-jobs")).toBeVisible();
    await expect(page.getByTestId("card-active-jobs")).toBeVisible();
    await expect(page.getByTestId("card-failed-jobs")).toBeVisible();
    await expect(page.getByTestId("card-system-backup-status")).toBeVisible();

    // DoD Requirement: Failed job notification banner & badge
    await expect(page.getByTestId("banner-failed-jobs-alert")).toBeVisible();
    await expect(page.getByTestId("badge-dod-notification-sent")).toBeVisible();
    await expect(page.getByTestId("badge-notifications-active")).toBeVisible();

    // Verify failed job in the table
    await expect(page.getByTestId("job-row-job_02")).toBeVisible();
    await expect(page.getByTestId("job-notification-job_02")).toBeVisible();
  });

  test("DoD E2E: triggers system backup, tests destination connection, and toggles schedule", async ({ page }) => {
    // 1. Trigger System Backup
    const systemBackupBtn = page.getByTestId("btn-trigger-system-backup");
    await expect(systemBackupBtn).toBeVisible();
    await systemBackupBtn.click();

    await expect(
      page.getByText(/System snapshot creation started|সিস্টেম স্ন্যাপশট ব্যাকগ্রাউন্ডে/i)
    ).toBeVisible();

    // 2. Switch to Destinations tab
    await page.getByTestId("tab-destinations").click();
    await expect(page.getByTestId("dest-row-dest_s3_01")).toBeVisible();
    await expect(page.getByTestId("dest-name-dest_s3_01")).toHaveText("Wasabi EU Cloud");

    // Click Test Connection
    const testDestBtn = page.getByTestId("btn-test-dest-dest_s3_01");
    await expect(testDestBtn).toBeVisible();
    await testDestBtn.click();

    await expect(
      page.getByText(/Connection test to Wasabi EU Cloud succeeded|কানেকশন টেস্ট সফল হয়েছে/i)
    ).toBeVisible();

    // 3. Switch to Schedules tab
    await page.getByTestId("tab-schedules").click();
    await expect(page.getByTestId("sched-row-sched_01")).toBeVisible();
    await expect(page.getByTestId("sched-name-sched_01")).toContainText("Nightly All-Tenant Backup");

    // Toggle Schedule state
    const toggleSchedBtn = page.getByTestId("btn-toggle-sched-sched_01");
    await expect(toggleSchedBtn).toBeVisible();
    await toggleSchedBtn.click();

    await expect(
      page.getByText(/Backup schedule paused|সক্রিয় করা হয়েছে|সাময়িক বন্ধ করা হয়েছে/i)
    ).toBeVisible();
  });
});
