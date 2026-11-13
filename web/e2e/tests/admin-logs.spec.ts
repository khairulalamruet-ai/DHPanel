import { test, expect } from "@playwright/test";

test.describe("Admin System Logs (T10.07)", () => {
  test("DoD: Admin views control plane, agent, Caddy, and auth system logs", async ({ page }) => {
    // 1. Navigate to Admin System Logs
    await page.goto("/admin/system-logs");

    await expect(page.getByTestId("admin-system-logs-title")).toBeVisible();
    await expect(page.getByTestId("virtualized-log-container")).toBeVisible();
    await expect(page.getByTestId("metric-admin-lines")).toBeVisible();

    // Verify service tabs exist
    await expect(page.getByTestId("tab-service-control-plane")).toBeVisible();
    await expect(page.getByTestId("tab-service-agent")).toBeVisible();
    await expect(page.getByTestId("tab-service-caddy")).toBeVisible();
    await expect(page.getByTestId("tab-service-auth")).toBeVisible();

    // 2. Switch to Node Agent service log
    await page.getByTestId("tab-service-agent").click();
    await expect(page.getByTestId("badge-service-active")).toHaveText("agent");
    await expect(page.getByTestId("virtualized-log-container")).toBeVisible();

    // 3. Switch to Caddy Server log
    await page.getByTestId("tab-service-caddy").click();
    await expect(page.getByTestId("badge-service-active")).toHaveText("caddy");

    // 4. Switch to Auth & Security log
    await page.getByTestId("tab-service-auth").click();
    await expect(page.getByTestId("badge-service-active")).toHaveText("auth");

    // 5. Filter by level
    await page.getByTestId("select-admin-log-level").selectOption("error");
    await expect(page.getByTestId("virtualized-log-container")).toBeVisible();

    // 6. Toggle Live Mode
    const liveTailBtn = page.getByTestId("btn-toggle-admin-live-mode");
    await liveTailBtn.click();
    await expect(page.getByTestId("badge-admin-live-status")).toContainText(
      /System Stream Active|সিস্টেম স্ট্রিম সক্রিয়/i
    );
  });

  test("DoD: Client cannot access admin system logs route", async ({ page }) => {
    // In client portal, admin-system-logs nav item does NOT exist
    await page.goto("/");
    await expect(page.locator('[data-testid="nav-admin-system-logs"]')).not.toBeVisible();
  });
});
