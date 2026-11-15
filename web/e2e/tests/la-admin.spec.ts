import { test, expect } from "@playwright/test";

test.describe("License Authority (LA) Admin UI (T17.06)", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate directly to the LA page
    await page.goto("/la");
  });

  test("loads license authority console and displays metrics and licenses", async ({ page }) => {
    // Header & title
    await expect(page.getByText(/License Authority \(LA\) Console|লাইসেন্স অথরিটি \(LA\) কনসোল/i)).toBeVisible();

    // Metrics cards
    await expect(page.getByTestId("metric-total-licenses")).toBeVisible();
    await expect(page.getByTestId("metric-active-licenses")).toBeVisible();

    // Table rows
    await expect(page.locator("table").getByText("DHP-ENT-2026-A1B2-C3D4").first()).toBeVisible();
    await expect(page.locator("table").getByText("Acme Global Hosting").first()).toBeVisible();
  });

  test("issues a new license through the dialog (DoD T17.06)", async ({ page }) => {
    // Click Issue License button
    const issueBtn = page.getByTestId("btn-open-issue-license");
    await expect(issueBtn).toBeVisible();
    await issueBtn.click();

    // Verify dialog opens
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await expect(dialog.getByText(/Issue New Customer License|নতুন গ্রাহক লাইসেন্স তৈরি করুন/i)).toBeVisible();

    // Fill form
    await page.getByTestId("input-customer-name").fill("HyperScale Cloud Ltd");
    await page.getByTestId("input-customer-email").fill("license@hyperscale.io");

    // Submit
    const submitBtn = page.getByTestId("btn-confirm-issue-license");
    await expect(submitBtn).toBeVisible();
    await submitBtn.click();

    // Verify dialog closes
    await expect(dialog).not.toBeVisible();

    // Verify new license appears in table
    await expect(page.locator("table").getByText("HyperScale Cloud Ltd").first()).toBeVisible();
  });

  test("revokes an active license with reason (DoD T17.06)", async ({ page }) => {
    // Find active license row and click revoke button
    const revokeBtn = page.getByTestId("btn-revoke-license-lic_01");
    await expect(revokeBtn).toBeVisible();
    await revokeBtn.click();

    // Verify revoke dialog opens
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await expect(dialog.getByText(/Revoke Customer License|গ্রাহক লাইসেন্স বাতিল করুন/i)).toBeVisible();

    // Confirm revocation
    const confirmBtn = page.getByTestId("btn-confirm-revoke-license");
    await expect(confirmBtn).toBeVisible();
    await confirmBtn.click();

    // Verify dialog closes
    await expect(dialog).not.toBeVisible();

    // Verify license shows revoked badge
    await expect(page.locator("table").getByTestId("license-revoked-badge").first()).toBeVisible();
  });

  test("switches between licenses, activations, and customer tabs", async ({ page }) => {
    // Activations Tab
    const actTab = page.getByTestId("tab-activations");
    await actTab.click();
    await expect(page.locator("table").getByText("167.104.101.81").first()).toBeVisible();

    // Customers Tab
    const custTab = page.getByTestId("tab-customers");
    await custTab.click();
    await expect(page.locator("table").getByText("Acme Global Hosting").first()).toBeVisible();

    // Back to Licenses Tab
    const licTab = page.getByTestId("tab-licenses");
    await licTab.click();
    await expect(page.locator("table").getByText("DHP-ENT-2026-A1B2-C3D4").first()).toBeVisible();
  });
});
