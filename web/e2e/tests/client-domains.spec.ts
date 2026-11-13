import { test, expect } from "@playwright/test";

test.describe("Client Domains Management UI (T05.05)", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate directly to the client domains page
    await page.goto("/client/domains");
  });

  test("loads domains page with quota banner and configured domains", async ({ page }) => {
    // Header
    await expect(page.getByRole("heading", { level: 1, name: /Domains|ডোমেইন/i })).toBeVisible();

    // Primary domain row
    await expect(page.getByText("alpha-hosting.com", { exact: true })).toBeVisible();
    await expect(page.getByText(/Primary|প্রাথমিক/i).first()).toBeVisible();
    await expect(page.getByText(/Active|সক্রিয়/i).first()).toBeVisible();

    // Quota banner
    await expect(page.getByTestId("domain-quota-banner")).toBeVisible();
    await expect(page.getByText(/3 of 5 domains configured|5-এর মধ্যে 3টি ডোমেইন/i)).toBeVisible();

    // Buttons
    await expect(page.getByTestId("btn-refresh-domains")).toBeVisible();
    await expect(page.getByTestId("btn-add-domain")).toBeVisible();
  });

  test("DoD: creates a new domain and verifies it appears in the table", async ({ page }) => {
    // Click Add Domain button
    await page.getByTestId("btn-add-domain").click();

    // Dialog appears
    await expect(page.getByRole("dialog")).toBeVisible();
    await expect(page.getByTestId("input-domain-name")).toBeVisible();

    // Enter FQDN domain name
    await page.getByTestId("input-domain-name").fill("e2e-client-shop.com");

    // Verify docroot auto-generated
    await expect(page.getByTestId("input-docroot")).toHaveValue("/domains/e2e-client-shop.com/public_html");

    // Submit dialog
    await page.getByTestId("btn-submit-domain").click();

    // Dialog closes and new domain appears in table
    await expect(page.getByText("e2e-client-shop.com", { exact: true })).toBeVisible();
    await expect(page.getByText("/domains/e2e-client-shop.com/public_html")).toBeVisible();
  });

  test("edits document root for a domain", async ({ page }) => {
    // dom_02 is blog.alpha-hosting.com
    await expect(page.getByText("blog.alpha-hosting.com", { exact: true })).toBeVisible();

    // Click edit docroot
    await page.getByTestId("btn-edit-docroot-dom_02").click();

    // Dialog appears
    await expect(page.getByTestId("input-edit-docroot")).toBeVisible();
    await page.getByTestId("input-edit-docroot").fill("/domains/blog.alpha-hosting.com/public_build");

    // Save
    await page.getByTestId("btn-save-docroot").click();

    // Updated docroot visible in table
    await expect(page.getByText("/domains/blog.alpha-hosting.com/public_build")).toBeVisible();
  });

  test("configures URL redirection for a domain", async ({ page }) => {
    await expect(page.getByText("blog.alpha-hosting.com", { exact: true })).toBeVisible();

    // Click manage redirect
    await page.getByTestId("btn-manage-redirect-dom_02").click();

    // Dialog appears
    await expect(page.getByTestId("checkbox-manage-redirect-toggle")).toBeVisible();

    // Enable redirect
    await page.getByTestId("checkbox-manage-redirect-toggle").click();
    await expect(page.getByTestId("input-manage-redirect-url")).toBeVisible();

    await page.getByTestId("input-manage-redirect-url").fill("https://redirect-target.org");

    // Save
    await page.getByTestId("btn-save-redirect").click();

    // Verify redirect appears in table
    await expect(page.getByText("https://redirect-target.org")).toBeVisible();
  });
});
