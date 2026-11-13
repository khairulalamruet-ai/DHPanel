import { test, expect } from "@playwright/test";

test.describe("Admin Domains Management UI (T05.06)", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate directly to the admin domains page
    await page.goto("/domains");
  });

  test("loads admin domains page with metric cards and cluster domains table", async ({ page }) => {
    // Page Header
    await expect(page.getByRole("heading", { level: 1, name: /All Domains|সকল ডোমেইন/i })).toBeVisible();

    // Metric cards
    await expect(page.getByText(/Total Domains|মোট ডোমেইন/i)).toBeVisible();
    await expect(page.getByText(/Active SSL|সক্রিয় এসএসএল/i)).toBeVisible();

    // Verify initial seeded domains
    await expect(page.getByText("alpha-hosting.com", { exact: true })).toBeVisible();
    await expect(page.getByText("beta-hosting.com", { exact: true })).toBeVisible();

    // Verify tenant owner column
    await expect(page.locator("table").getByText("alpha", { exact: true }).first()).toBeVisible();
    await expect(page.locator("table").getByText("beta", { exact: true }).first()).toBeVisible();

    // Buttons
    await expect(page.getByTestId("btn-refresh-admin-domains")).toBeVisible();
    await expect(page.getByTestId("btn-bulk-add-domains")).toBeVisible();
  });

  test("filters domains table by search keyword", async ({ page }) => {
    await expect(page.getByText("alpha-hosting.com", { exact: true })).toBeVisible();

    const searchInput = page.getByTestId("input-search-admin-domains");
    await searchInput.fill("beta");

    // Only beta appears
    await expect(page.getByText("beta-hosting.com", { exact: true })).toBeVisible();
    await expect(page.getByText("alpha-hosting.com", { exact: true })).not.toBeVisible();
  });

  test("toggles suspend/unsuspend state of a domain", async ({ page }) => {
    await expect(page.getByText("alpha-hosting.com", { exact: true })).toBeVisible();

    // Click suspend button for adm_dom_1
    const suspendBtn = page.getByTestId("btn-toggle-suspend-adm_dom_1");
    await suspendBtn.click();

    // Status changes to Suspended
    const row = page.getByTestId("admin-domain-row-adm_dom_1");
    await expect(row.getByTestId("domain-status-badge-adm_dom_1")).toHaveText(/Suspended|স্থগিত/i);

    // Click Unsuspend
    await suspendBtn.click();
    await expect(row.getByTestId("domain-status-badge-adm_dom_1")).toHaveText(/Active|সক্রিয়/i);
  });

  test("DoD: Bulk adds 100 domains through operation queue dialog", async ({ page }) => {
    await expect(page.getByText("alpha-hosting.com", { exact: true })).toBeVisible();

    // Click Bulk Add button
    await page.getByTestId("btn-bulk-add-domains").click();

    // Dialog appears
    await expect(page.getByRole("dialog")).toBeVisible();
    await expect(page.getByTestId("btn-fill-100-samples")).toBeVisible();

    // Click quick fill 100 sample domains
    await page.getByTestId("btn-fill-100-samples").click();

    // Verify 100 lines populated
    const textarea = page.getByTestId("textarea-bulk-domains");
    await expect(textarea).toContainText("bulk-site-001.example.com");
    await expect(textarea).toContainText("bulk-site-100.example.com");

    // Submit bulk operation
    await page.getByTestId("btn-submit-bulk-domains").click();

    // Operation view appears with Succeeded status
    await expect(page.getByTestId("bulk-op-succeeded-badge")).toBeVisible();

    // Click Done & Close
    await page.getByTestId("btn-done-bulk-domains").click();

    // Dialog closes and new domain appears in table
    await expect(page.getByText("bulk-site-001.example.com", { exact: true })).toBeVisible();
  });
});
