import { test, expect } from "@playwright/test";

test.describe("Admin Database Overview UI (T07.11)", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate directly to the admin databases overview page
    await page.goto("/admin/databases");
  });

  test("loads admin database overview with summary metrics and node distributions", async ({ page }) => {
    // Page Header
    await expect(page.getByTestId("admin-db-title")).toBeVisible();

    // Summary metrics
    await expect(page.getByTestId("metric-total-dbs")).toHaveText("48");
    await expect(page.getByTestId("metric-total-storage")).toHaveText("1.85 GB");
    await expect(page.getByTestId("metric-active-nodes")).toHaveText("3");

    // DoD: Node-wise distribution cards
    await expect(page.getByTestId("card-db-node-node_01")).toBeVisible();
    await expect(page.getByTestId("node-count-node_01")).toHaveText("24");
    await expect(page.getByTestId("node-size-node_01")).toHaveText("1000.0 MB");

    await expect(page.getByTestId("card-db-node-node_02")).toBeVisible();
    await expect(page.getByTestId("card-db-node-node_03")).toBeVisible();
  });

  test("DoD: displays largest databases table with top consumption items and filters", async ({ page }) => {
    // Verify high-consumption databases are listed
    await expect(page.getByTestId("largest-db-row-ecomm_magento_prod")).toBeVisible();
    await expect(page.getByTestId("largest-db-row-portal_lms_db")).toBeVisible();
    await expect(page.getByTestId("db-size-ecomm_magento_prod")).toHaveText("500.0 MB");

    // Test Search input
    const searchInput = page.getByTestId("input-search-largest-dbs");
    await expect(searchInput).toBeVisible();
    await searchInput.fill("magento");

    await expect(page.getByTestId("largest-db-row-ecomm_magento_prod")).toBeVisible();
    await expect(page.getByTestId("largest-db-row-portal_lms_db")).not.toBeVisible();

    // Clear search and test node filter
    await searchInput.fill("");
    const nodeFilter = page.getByTestId("select-filter-node");
    await nodeFilter.selectOption("node_02");

    await expect(page.getByTestId("largest-db-row-crm_enterprise_leads")).toBeVisible();
    await expect(page.getByTestId("largest-db-row-ecomm_magento_prod")).not.toBeVisible();
  });
});
