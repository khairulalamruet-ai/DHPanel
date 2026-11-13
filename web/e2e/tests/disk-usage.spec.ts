import { test, expect } from "@playwright/test";

test.describe("Tenant Disk & Inodes Usage UI (T08.10)", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate directly to disk usage page
    await page.goto("/disk-usage");
  });

  test("loads disk usage page with metric cards and progress meters", async ({ page }) => {
    // Page Title
    await expect(page.getByTestId("disk-usage-title")).toBeVisible();

    // Metric cards
    await expect(page.getByTestId("card-disk-quota")).toBeVisible();
    await expect(page.getByTestId("metric-disk-used")).toBeVisible();
    await expect(page.getByTestId("disk-progress-bar")).toBeVisible();

    await expect(page.getByTestId("card-inodes-quota")).toBeVisible();
    await expect(page.getByTestId("metric-inodes-used")).toBeVisible();
    await expect(page.getByTestId("inodes-progress-bar")).toBeVisible();
  });

  test("displays top 10 largest directories meeting DoD requirement", async ({ page }) => {
    await expect(page.getByTestId("top-directories-heading")).toBeVisible();

    // Verify all 10 rows are rendered
    for (let i = 0; i < 10; i++) {
      await expect(page.getByTestId(`top-dir-row-${i}`)).toBeVisible();
    }

    // Verify largest folder is #1 uploads
    await expect(page.getByTestId("dir-path-0")).toContainText("uploads");
    await expect(page.getByTestId("dir-size-0")).toContainText("450.0 MB");

    // Action button to open in file manager exists
    await expect(page.getByTestId("btn-open-dir-0")).toBeVisible();
  });

  test("triggers recalculate storage scan", async ({ page }) => {
    await expect(page.getByTestId("btn-recalculate-disk")).toBeVisible();
    await page.getByTestId("btn-recalculate-disk").click();

    // Expect success toast
    await expect(
      page.getByText(/recalculated successfully|সফলভাবে পুনরায় গণনা করা হয়েছে/i)
    ).toBeVisible();
  });

  test("can navigate from file manager to disk usage", async ({ page }) => {
    await page.goto("/files");
    await expect(page.getByTestId("disk-usage-nav-button")).toBeVisible();
    await page.getByTestId("disk-usage-nav-button").click();

    await expect(page.getByTestId("disk-usage-title")).toBeVisible();
  });
});
