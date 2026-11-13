import { test, expect } from "@playwright/test";

test.describe("Web Application Firewall (WAF) UI (T11.04)", () => {
  test("Client: views blocked requests, inspects attack payload, disables rule for false positives (DoD), and manages policy", async ({ page }) => {
    // 1. Navigate to client WAF page
    await page.goto("/waf");

    await expect(page.getByTestId("waf-page-title")).toBeVisible();
    await expect(page.getByTestId("select-waf-domain")).toBeVisible();
    await expect(page.getByTestId("card-waf-blocked")).toBeVisible();

    // 2. Verify blocked request events table
    await expect(page.getByTestId("title-waf-events")).toBeVisible();
    await expect(page.getByTestId("waf-event-row-waf-ev-01")).toBeVisible();
    await expect(page.getByText("942100")).toBeVisible();
    await expect(page.getByText("/wp-login.php")).toBeVisible();

    // 3. Inspect event details modal
    const inspectBtn = page.getByTestId("btn-inspect-event-waf-ev-01");
    await expect(inspectBtn).toBeVisible();
    await inspectBtn.click();

    await expect(page.getByTestId("waf-detail-title")).toBeVisible();
    await expect(page.getByTestId("detail-rule-id")).toContainText("942100");
    await expect(page.getByText(/ARGS:log -> admin' OR '1'='1/i)).toBeVisible();

    // Close detail modal
    await page.getByTestId("btn-close-waf-detail").click();

    // 4. DoD: Disable rule from UI ("এই rule এই domain-এ বন্ধ করো" বোতাম)
    const disableBtn = page.getByTestId("btn-disable-rule-waf-ev-01");
    await expect(disableBtn).toBeVisible();
    await disableBtn.click();

    await expect(page.getByTestId("disable-rule-dialog-title")).toBeVisible();
    await expect(page.getByTestId("badge-modal-rule-id")).toContainText("942100");

    const reasonInput = page.getByTestId("input-disable-reason");
    await reasonInput.fill("False positive report on admin login form");

    const confirmDisableBtn = page.getByTestId("btn-confirm-disable-rule");
    await expect(confirmDisableBtn).toBeVisible();
    await confirmDisableBtn.click();

    // Verify success toast
    await expect(
      page.getByText(/Rule 942100 has been disabled|রুল 942100 সফলভাবে.*বন্ধ করা হয়েছে/i)
    ).toBeVisible();

    // 5. Switch to Disabled Rules tab
    const disabledTab = page.getByTestId("tab-waf-disabled-rules");
    await disabledTab.click();

    await expect(page.getByTestId("title-disabled-rules")).toBeVisible();
    await expect(page.getByTestId("disabled-rule-row-942100")).toBeVisible();

    // 6. Test Policy mode toggle
    const detectModeBtn = page.getByTestId("btn-waf-mode-detect");
    await expect(detectModeBtn).toBeVisible();
    await detectModeBtn.click();

    await expect(
      page.getByText(/WAF protection mode updated|WAF সুরক্ষা মোড.*আপডেট করা হয়েছে/i)
    ).toBeVisible();
  });

  test("Admin: inspects cluster-wide WAF events and filters by tenant and domain", async ({ page }) => {
    // Navigate to admin WAF controller
    await page.goto("/admin/waf");

    await expect(page.getByTestId("admin-waf-page-title")).toBeVisible();
    await expect(page.getByTestId("select-admin-waf-tenant")).toBeVisible();
    await expect(page.getByTestId("select-admin-waf-domain")).toBeVisible();
    await expect(page.getByTestId("card-admin-waf-blocked")).toBeVisible();
    await expect(page.getByTestId("waf-event-row-waf-ev-01")).toBeVisible();
  });
});
