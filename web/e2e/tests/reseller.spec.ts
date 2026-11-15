import { test, expect } from "@playwright/test";

test.describe("Reseller Management UI (T14.05, T14.02, T14.07)", () => {
  test("Reseller: views quota stats, manages client accounts, creates custom plan, and customizes white-label branding (T14.05 DoD)", async ({
    page,
  }) => {
    // 1. Navigate to Reseller portal
    await page.goto("/reseller");

    await expect(page.getByTestId("reseller-portal-title")).toBeVisible();
    await expect(page.getByTestId("reseller-stats-grid")).toBeVisible();
    await expect(page.getByTestId("stat-accounts-count")).toBeVisible();
    await expect(page.getByTestId("stat-disk-used")).toBeVisible();

    // 2. Client Accounts Table (T14.05)
    await expect(page.getByTestId("reseller-users-section")).toBeVisible();
    await expect(page.getByTestId("reseller-user-row-ru-1")).toBeVisible();
    await expect(page.getByRole("link", { name: "novatech.io" })).toBeVisible();

    // Add Client Account
    const addUserBtn = page.getByTestId("btn-add-reseller-user");
    await addUserBtn.click();

    await expect(page.getByTestId("create-reseller-user-dialog")).toBeVisible();
    await page.getByTestId("input-reseller-user-username").fill("skydrive_inc");
    await page.getByTestId("input-reseller-user-domain").fill("skydrive.cloud");
    await page.getByTestId("btn-submit-reseller-user").click();

    // Verify newly added client
    await expect(page.getByTestId("reseller-users-section").getByText("skydrive_inc", { exact: true })).toBeVisible();
    await expect(page.getByRole("link", { name: "skydrive.cloud" })).toBeVisible();

    // 3. Custom Plans Tab (T14.05)
    const plansTab = page.getByTestId("tab-reseller-plans");
    await plansTab.click();

    await expect(page.getByTestId("reseller-plans-section")).toBeVisible();
    await expect(page.getByTestId("reseller-plan-row-rp-1")).toBeVisible();
    await expect(page.getByText("Client Starter")).toBeVisible();

    // Create new plan
    const addPlanBtn = page.getByTestId("btn-add-reseller-plan");
    await addPlanBtn.click();

    await expect(page.getByTestId("create-reseller-plan-dialog")).toBeVisible();
    await page.getByTestId("input-reseller-plan-name").fill("Executive Enterprise VIP");
    await page.getByTestId("input-reseller-plan-disk").fill("30");
    await page.getByTestId("btn-submit-reseller-plan").click();

    await expect(page.getByTestId("reseller-plans-section").getByText("Executive Enterprise VIP")).toBeVisible();

    // 4. White-Label Branding Tab (T14.05: "branding: নিজস্ব logo ও রং, যা তাঁর client দেখবেন")
    const brandingTab = page.getByTestId("tab-reseller-branding");
    await brandingTab.click();

    await expect(page.getByTestId("reseller-branding-section")).toBeVisible();
    await expect(page.getByTestId("branding-preview-card")).toBeVisible();

    // Check initial preview
    await expect(page.getByTestId("preview-brand-name")).toContainText("Apex Cloud Solutions");

    // Modify Brand Name & Color and watch live preview update immediately
    const brandNameInput = page.getByTestId("input-brand-name");
    await brandNameInput.fill("Skyline HyperCloud");
    await expect(page.getByTestId("preview-brand-name")).toHaveText("Skyline HyperCloud");

    // Save branding
    const saveBrandingBtn = page.getByTestId("btn-save-branding");
    await saveBrandingBtn.click();

    await expect(
      page.getByText(/Branding saved successfully|ব্র্যান্ডিং সফলভাবে সংরক্ষিত হয়েছে/i)
    ).toBeVisible();
  });

  test("Admin: manages reseller tiers, suspends accounts (T14.07), and creates reseller packages (T14.02)", async ({
    page,
  }) => {
    // 1. Navigate to Admin Resellers page
    await page.goto("/admin/resellers");

    await expect(page.getByTestId("admin-resellers-title")).toBeVisible();
    await expect(page.getByTestId("tab-admin-resellers")).toBeVisible();
    await expect(page.getByTestId("tab-admin-packages")).toBeVisible();

    // 2. Resellers table & suspend action (T14.07)
    await expect(page.getByTestId("resellers-table-container")).toBeVisible();
    await expect(page.getByTestId("admin-reseller-row-reseller-1")).toBeVisible();
    await expect(page.getByTestId("reseller-company-reseller-1")).toHaveText("Apex Cloud Solutions");

    // Suspend reseller
    const suspendBtn = page.getByTestId("btn-suspend-reseller-reseller-1");
    await suspendBtn.click();

    await expect(
      page.getByText(/Reseller Suspended|রিসেলার স্থগিত করা হয়েছে/i)
    ).toBeVisible();

    // 3. Switch to Reseller Packages tab (T14.02 DoD)
    const pkgTab = page.getByTestId("tab-admin-packages");
    await pkgTab.click();

    await expect(page.getByTestId("packages-table-container")).toBeVisible();
    await expect(page.getByTestId("reseller-pkg-row-pkg-1")).toBeVisible();
    await expect(page.getByText("Growth Agency Tier")).toBeVisible();

    // Create Reseller Package (T14.02)
    const openCreatePkgBtn = page.getByTestId("btn-open-create-pkg");
    await openCreatePkgBtn.click();

    await expect(page.getByTestId("create-reseller-package-dialog")).toBeVisible();
    await page.getByTestId("input-package-name").fill("Mega Agency Cluster");
    await page.getByTestId("input-package-accounts").fill("75");
    await page.getByTestId("input-package-disk").fill("400");
    await page.getByTestId("input-package-bw").fill("3000");

    const submitPkgBtn = page.getByTestId("btn-submit-reseller-package");
    await submitPkgBtn.click();

    // Verify package appears in table
    await expect(page.getByTestId("packages-table-container").getByText("Mega Agency Cluster")).toBeVisible();
  });
});
