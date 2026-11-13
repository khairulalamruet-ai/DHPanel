import { test, expect } from "@playwright/test";

test.describe("Admin VHost Template UI (T06.07)", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate directly to the VHost templates management page
    await page.goto("/vhost-templates");
  });

  test("loads vhost templates page with available templates list and code editor", async ({ page }) => {
    // Page Header
    await expect(
      page.getByRole("heading", { level: 1, name: /VHost Templates & Directives|ভিহোস্ট টেমপ্লেট ও নির্দেশাবলী/i })
    ).toBeVisible();

    // Verify template cards in the sidebar
    await expect(page.getByTestId("card-vhost-template-nginx-php-fpm")).toBeVisible();
    await expect(page.getByTestId("card-vhost-template-nginx-static")).toBeVisible();
    await expect(page.getByTestId("card-vhost-template-caddy-reverse-proxy")).toBeVisible();

    // Verify Editor textarea
    await expect(page.getByTestId("textarea-vhost-content")).toBeVisible();
  });

  test("switches between templates and updates editor directives", async ({ page }) => {
    await expect(page.getByTestId("card-vhost-template-caddy-reverse-proxy")).toBeVisible();

    // Click Caddy template
    await page.getByTestId("card-vhost-template-caddy-reverse-proxy").click();

    // Verify textarea updates with Caddy directives
    const textarea = page.getByTestId("textarea-vhost-content");
    await expect(textarea).toHaveValue(/reverse_proxy 127\.0\.0\.1:\{\{port\}\}/);
  });

  test("switches to Live Preview and verifies variable interpolation", async ({ page }) => {
    await expect(page.getByTestId("tab-preview")).toBeVisible();

    // Switch to Preview tab
    await page.getByTestId("tab-preview").click();

    // Verify preview renders sample values
    const renderedPre = page.getByTestId("pre-vhost-rendered");
    await expect(renderedPre).toBeVisible();
    await expect(renderedPre).toContainText("my-awesome-site.com");
    await expect(renderedPre).toContainText("/home/demo/public_html");
  });

  test("opens version history dialog and views revisions", async ({ page }) => {
    await expect(page.getByTestId("btn-vhost-history")).toBeVisible();

    // Open History Dialog
    await page.getByTestId("btn-vhost-history").click();

    await expect(page.getByTestId("dialog-vhost-history")).toBeVisible();
    await expect(page.getByTestId("version-item-3")).toBeVisible();
    await expect(page.getByTestId("version-item-2")).toBeVisible();
  });

  test("DoD: executes 'Apply to all domains' background operation", async ({ page }) => {
    await expect(page.getByTestId("btn-apply-all-domains")).toBeVisible();

    // Open Apply All dialog
    await page.getByTestId("btn-apply-all-domains").click();
    await expect(page.getByTestId("dialog-vhost-apply-all")).toBeVisible();

    // Confirm deployment
    await page.getByTestId("btn-confirm-apply-all").click();

    // Success confirmation banner in modal
    await expect(page.getByText(/Deployment Succeeded!/i)).toBeVisible();
  });
});
