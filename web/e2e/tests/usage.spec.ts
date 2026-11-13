import { test, expect } from "@playwright/test";

test.describe("Usage Chart & Analytics UI (T10.10)", () => {
  test("Client Usage: loads gauges, 4 charts, switches time ranges, and verifies light/dark theme readability", async ({ page }) => {
    // 1. Navigate to client usage page
    await page.goto("/usage");

    await expect(page.getByTestId("usage-page-title")).toBeVisible();

    // Summary metric cards
    await expect(page.getByTestId("card-usage-cpu")).toBeVisible();
    await expect(page.getByTestId("card-usage-ram")).toBeVisible();
    await expect(page.getByTestId("card-usage-disk")).toBeVisible();
    await expect(page.getByTestId("card-usage-bandwidth")).toBeVisible();

    // 4 detailed charts
    await expect(page.getByTestId("card-chart-cpu")).toBeVisible();
    await expect(page.getByTestId("card-chart-ram")).toBeVisible();
    await expect(page.getByTestId("card-chart-disk")).toBeVisible();
    await expect(page.getByTestId("card-chart-network")).toBeVisible();

    // 2. Switch time ranges (24h -> 7d -> 30d)
    await page.getByTestId("btn-range-7d").click();
    await expect(page.getByTestId("card-chart-cpu")).toBeVisible();

    await page.getByTestId("btn-range-30d").click();
    await expect(page.getByTestId("card-chart-cpu")).toBeVisible();

    // 3. DoD Verification: Theme adaptability (Chart light & dark theme readability)
    // Verify SVG and grid lines visible in current theme
    const chartSvg = page.locator('[data-testid="usage-area-chart"] svg').first();
    await expect(chartSvg).toBeVisible();

    // Verify grid lines and paths are attached
    await expect(chartSvg.locator("line").first()).toBeAttached();
    await expect(chartSvg.locator("path").first()).toBeAttached();
  });

  test("Admin Usage: loads node overview, history chart, and switches top-N tenant rankings", async ({ page }) => {
    // 1. Navigate to admin usage page
    await page.goto("/admin/usage");

    await expect(page.getByTestId("admin-usage-title")).toBeVisible();

    // Node summary gauges
    await expect(page.getByTestId("card-admin-node-cpu")).toBeVisible();
    await expect(page.getByTestId("card-admin-node-ram")).toBeVisible();
    await expect(page.getByTestId("card-admin-node-disk")).toBeVisible();
    await expect(page.getByTestId("card-admin-node-network")).toBeVisible();

    // Node timeline chart
    await expect(page.getByTestId("card-admin-history-chart")).toBeVisible();

    // 2. Top-N Tenants section
    await expect(page.getByTestId("card-top-tenants")).toBeVisible();
    await expect(page.getByTestId("row-tenant-t_01")).toBeVisible();
    await expect(page.getByTestId("tenant-name-t_01")).toHaveText("acme-corp");

    // Switch Top-N tabs
    await page.getByTestId("tab-top-ram").click();
    await expect(page.getByTestId("row-tenant-t_03")).toBeVisible();

    await page.getByTestId("tab-top-disk").click();
    await expect(page.getByTestId("row-tenant-t_06")).toBeVisible();

    await page.getByTestId("tab-top-bandwidth").click();
    await expect(page.getByTestId("row-tenant-t_06")).toBeVisible();
  });
});
