import { test, expect } from "@playwright/test";

test.describe("SSL / TLS Certificate Management UI (T05.19)", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate directly to the SSL management page
    await page.goto("/ssl");
  });

  test("loads SSL management page with metrics and certificates list", async ({ page }) => {
    // Page Header
    await expect(
      page.getByRole("heading", { level: 1, name: /SSL \/ TLS Certificates|এসএসএল \/ টিএলএস সার্টিফিকেট/i })
    ).toBeVisible();

    // Verify seeded certificates rendered by their heading
    await expect(page.getByRole("heading", { level: 3, name: "alpha-hosting.com", exact: true })).toBeVisible();
    await expect(page.getByRole("heading", { level: 3, name: "blog.alpha-hosting.com", exact: true })).toBeVisible();
    await expect(page.getByRole("heading", { level: 3, name: "beta-app.io", exact: true })).toBeVisible();
    await expect(page.getByRole("heading", { level: 3, name: "dev.internal-net.org", exact: true })).toBeVisible();

    // Verify Metric Cards
    await expect(page.getByTestId("ssl-page")).toBeVisible();
    await expect(page.getByText("4", { exact: true })).toBeVisible();
  });

  test("filters certificates by status pill and search input", async ({ page }) => {
    await expect(page.getByRole("heading", { level: 3, name: "alpha-hosting.com", exact: true })).toBeVisible();

    // Filter by expired status
    await page.getByTestId("btn-filter-ssl-expired").click();
    await expect(page.getByRole("heading", { level: 3, name: "beta-app.io", exact: true })).toBeVisible();
    await expect(page.getByRole("heading", { level: 3, name: "alpha-hosting.com", exact: true })).not.toBeVisible();

    // Reset filter
    await page.getByTestId("btn-filter-ssl-ALL").click();
    await expect(page.getByRole("heading", { level: 3, name: "alpha-hosting.com", exact: true })).toBeVisible();

    // Search query
    const searchInput = page.getByTestId("input-search-ssl");
    await searchInput.fill("blog");
    await expect(page.getByRole("heading", { level: 3, name: "blog.alpha-hosting.com", exact: true })).toBeVisible();
    await expect(page.getByRole("heading", { level: 3, name: "beta-app.io", exact: true })).not.toBeVisible();
  });

  test("DoD: re-issues / renews Let's Encrypt SSL certificate", async ({ page }) => {
    await expect(page.getByRole("heading", { level: 3, name: "blog.alpha-hosting.com", exact: true })).toBeVisible();

    // Click Reissue button on blog.alpha-hosting.com (dom_02)
    const reissueBtn = page.getByTestId("btn-reissue-ssl-dom_02");
    await reissueBtn.click();

    // Certificate renewed to 90 days
    await expect(page.getByText(/(90|৯০)\s*(days left|দিন বাকি)/)).toBeVisible();
  });

  test("DoD: installs custom commercial SSL certificate", async ({ page }) => {
    await expect(page.getByRole("heading", { level: 3, name: "beta-app.io", exact: true })).toBeVisible();

    // Open Custom Cert Dialog for beta-app.io (dom_03)
    await page.getByTestId("btn-upload-custom-ssl-dom_03").click();
    await expect(page.getByTestId("dialog-custom-ssl")).toBeVisible();

    // Fill PEM cert and key
    await page.getByTestId("textarea-ssl-cert-pem").fill(
      "-----BEGIN CERTIFICATE-----\nMIIDXTCCAkWgAwIBAgIJAP999EXAMPLE\n-----END CERTIFICATE-----"
    );
    await page.getByTestId("textarea-ssl-key-pem").fill(
      "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEAz999EXAMPLE\n-----END RSA PRIVATE KEY-----"
    );

    // Submit form
    await page.getByTestId("btn-submit-custom-ssl").click();

    // Modal closes and shows custom issuer
    await expect(page.getByTestId("dialog-custom-ssl")).not.toBeVisible();
    await expect(page.getByText("Custom Uploaded CA")).toBeVisible();
  });

  test("configures HTTPS redirection and HSTS security policy", async ({ page }) => {
    await expect(page.getByRole("heading", { level: 3, name: "alpha-hosting.com", exact: true })).toBeVisible();

    // Open settings for alpha-hosting.com (dom_01)
    await page.getByTestId("btn-settings-ssl-dom_01").click();
    await expect(page.getByTestId("dialog-ssl-settings")).toBeVisible();

    // Toggle checkboxes
    await page.getByTestId("checkbox-force-https").click();
    await page.getByTestId("checkbox-hsts").click();

    // Save
    await page.getByTestId("btn-save-ssl-settings").click();
    await expect(page.getByTestId("dialog-ssl-settings")).not.toBeVisible();
  });
});
