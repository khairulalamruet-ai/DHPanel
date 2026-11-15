import { test, expect } from "@playwright/test";

test.describe("Frontend Module Registry & Feature Gate (T03.11)", () => {
  test("dynamically renders only enabled modules in sidebar navigation (DoD)", async ({ page }) => {
    await page.goto("/");

    // Verify core navigation items are present
    await expect(page.getByRole("link", { name: /Dashboard|ড্যাশবোর্ড/i })).toBeVisible();
    await expect(page.getByRole("link", { name: /^Security$|^নিরাপত্তা$/i })).toBeVisible();

    // Verify that non-active/dummy module is not rendered in the sidebar by default
    await expect(page.getByRole("link", { name: /Dummy Module|ডামি মডিউল/i })).not.toBeVisible();
  });

  test("displays 'Plan-এ নেই' (Feature Required) fallback when accessing feature-gated route directly", async ({ page }) => {
    // Navigate directly to feature-gated route
    await page.goto("/dummy");

    // Expect the feature disabled fallback page
    await expect(
      page.getByText(/This feature is not enabled for your plan|আপনার প্যাকেজে এই সুবিধাটি চালু নেই/i).first()
    ).toBeVisible();

    // Expect upgrade plan and back to dashboard buttons
    const backBtn = page.getByRole("button", { name: /Back to Dashboard|ড্যাশবোর্ডে ফিরে যান/i });
    await expect(backBtn).toBeVisible();

    // Clicking back to dashboard redirects home
    await backBtn.click();
    await expect(page).toHaveURL("/");
  });
});
