import { test, expect } from "@playwright/test";

test.describe("Support Report & Zero-Secrets Diagnostic Bundle (T18.12)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/admin/support-report");
  });

  test("loads support report page and displays metadata and system diagnostics", async ({ page }) => {
    await expect(page.getByTestId("support-report-page")).toBeVisible();
    await expect(page.getByText(/Support & Diagnostic Report|সাপোর্ট ও ডায়াগনস্টিক রিপোর্ট/i).first()).toBeVisible();

    // Verify metadata
    await expect(page.getByTestId("meta-bundle-id")).toHaveText("diag-20261114-8841");
    await expect(page.getByTestId("meta-panel-version")).toHaveText("v0.1.0");

    // Verify DoD Zero Secrets Badge
    await expect(page.getByTestId("badge-zero-secrets")).toHaveText(/Audited Clean|যাচাইকৃত/i);

    // Verify sanitized config & logs containers
    await expect(page.getByTestId("sanitized-config-preview")).toBeVisible();
    await expect(page.getByTestId("sanitized-logs-container")).toBeVisible();
  });

  test("DoD: verifies masked configuration contains zero plaintext passwords or secrets", async ({ page }) => {
    const configPreview = page.getByTestId("sanitized-config-preview");
    await expect(configPreview).toBeVisible();

    const configText = await configPreview.innerText();

    // Must contain masked credential tokens
    expect(configText).toContain("[REDACTED_CONFIDENTIAL]");

    // Must NEVER contain raw plaintext passwords
    expect(configText).not.toContain("super_secret");
    expect(configText).not.toContain("password123");
    expect(configText).not.toContain("BEGIN PRIVATE KEY");
  });

  test("refreshes diagnostic bundle on action click", async ({ page }) => {
    const refreshBtn = page.getByTestId("btn-generate-report");
    await expect(refreshBtn).toBeVisible();
    await refreshBtn.click();

    // Toast confirmation
    await expect(
      page.getByText(/Report Generated|রিপোর্ট তৈরি হয়েছে/i).first()
    ).toBeVisible();
  });

  test("filters sanitized log slices by keyword", async ({ page }) => {
    const filterInput = page.getByTestId("input-filter-logs");
    await expect(filterInput).toBeVisible();

    // Filter by auth service
    await filterInput.fill("auth");

    const logsContainer = page.getByTestId("sanitized-logs-container");
    await expect(logsContainer).toContainText("auth-gateway");
    await expect(logsContainer).toContainText("Bearer [REDACTED_TOKEN]");
  });
});
