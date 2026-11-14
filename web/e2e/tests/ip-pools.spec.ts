import { test, expect } from "@playwright/test";

test.describe("Node IP Pool & Allocation UI (T16.05)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/admin/ip-pools");
  });

  test("loads IP pools page with metrics and table across cluster nodes", async ({ page }) => {
    await expect(page.getByText(/Cluster IP Pools|আইপি পুল/i).first()).toBeVisible();

    // Verify initial IPs in table
    await expect(page.locator("table").getByText("167.104.101.81")).toBeVisible();
    await expect(page.locator("table").getByText("node-master-01").first()).toBeVisible();
    await expect(page.locator("table").getByText("node-worker-02").first()).toBeVisible();

    // Verify metrics card
    await expect(page.getByTestId("metric-total-ips")).toHaveText("6");
  });

  test("DoD T16.05: allocates an available dedicated IP to a tenant", async ({ page }) => {
    // ip_04 is available
    const allocateBtn = page.getByTestId("btn-allocate-ip-ip_04");
    await expect(allocateBtn).toBeVisible();
    await allocateBtn.click();

    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();

    // Fill optional domain
    await dialog.getByTestId("input-allocate-domain").fill("e2e-store.alex.com");

    // Submit allocation
    const confirmBtn = dialog.getByTestId("btn-confirm-allocate-ip");
    await confirmBtn.click();

    await expect(dialog).not.toBeVisible();

    // Verify assigned tenant displayed in table row
    const tenantBadge = page.getByTestId("assigned-tenant-ip_04");
    await expect(tenantBadge).toBeVisible();
    await expect(page.locator("table").getByText("e2e-store.alex.com")).toBeVisible();
  });

  test("DoD T16.05: releases an assigned dedicated IP back to node pool", async ({ page }) => {
    // ip_03 is initially assigned to @alex
    await expect(page.getByTestId("assigned-tenant-ip_03")).toBeVisible();

    // Auto-accept confirm dialog
    page.on("dialog", (dialog) => dialog.accept());

    const releaseBtn = page.getByTestId("btn-release-ip-ip_03");
    await expect(releaseBtn).toBeVisible();
    await releaseBtn.click();

    // Verify IP is released and displays unassigned label
    const unassignedLabel = page.getByTestId("unassigned-label-ip_03");
    await expect(unassignedLabel).toBeVisible();
  });
});
