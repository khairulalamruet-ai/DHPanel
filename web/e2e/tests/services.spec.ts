import { test, expect } from "@playwright/test";

test.describe("Web Services Restart & Monitoring UI (T06.12)", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate directly to the services management page
    await page.goto("/services");
  });

  test("loads services page with running services and metrics", async ({ page }) => {
    // Page Header
    await expect(
      page.getByRole("heading", { level: 1, name: /Web Services Management|ওয়েব সার্ভিস ব্যবস্থাপনা/i })
    ).toBeVisible();

    // Verify service cards
    await expect(page.getByTestId("service-card-nginx")).toBeVisible();
    await expect(page.getByTestId("service-card-php-fpm")).toBeVisible();

    // Verify status badges
    await expect(page.getByTestId("badge-status-nginx")).toContainText("RUNNING");
    await expect(page.getByTestId("badge-status-php-fpm")).toContainText("RUNNING");
  });

  test("restarts service successfully and increments restart quota", async ({ page }) => {
    await expect(page.getByTestId("btn-restart-nginx")).toBeVisible();

    // Initial quota: 0 / 3
    await expect(page.getByTestId("quota-restarts-nginx")).toContainText("0 / 3");

    // Click restart
    await page.getByTestId("btn-restart-nginx").click();

    // Quota updates to 1 / 3
    await expect(page.getByTestId("quota-restarts-nginx")).toContainText("1 / 3");
  });

  test("DoD: triggers HTTP 429 rate limit when exceeding 3 restarts per minute", async ({ page }) => {
    await expect(page.getByTestId("btn-restart-php-fpm")).toBeVisible();

    const restartBtn = page.getByTestId("btn-restart-php-fpm");

    // Restart 1
    await restartBtn.click();
    await expect(page.getByTestId("quota-restarts-php-fpm")).toContainText("1 / 3");

    // Restart 2
    await restartBtn.click();
    await expect(page.getByTestId("quota-restarts-php-fpm")).toContainText("2 / 3");

    // Restart 3
    await restartBtn.click();
    await expect(page.getByTestId("quota-restarts-php-fpm")).toContainText("3 / 3");

    // Restart 4 (exceeds 3/min limit -> HTTP 429)
    await restartBtn.click();

    // Rate limit error banner appears
    await expect(page.getByTestId("banner-rate-limit-php-fpm")).toBeVisible();
    await expect(page.getByTestId("banner-rate-limit-php-fpm")).toContainText(/Rate limit exceeded|You can restart this service at most 3 times per minute/i);
  });
});
