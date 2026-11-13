import { test, expect } from "@playwright/test";

test.describe("Security Center UI (T11.11)", () => {
  test("Admin: inspects security health score, views 6 domain KPIs, and resolves issues via Fix links (DoD)", async ({
    page,
  }) => {
    // Navigate to admin security center
    await page.goto("/admin/security");

    // 1. Verify Page Header & Score Card
    await expect(page.getByTestId("security-center-title")).toBeVisible();
    await expect(page.getByTestId("security-score-card")).toBeVisible();
    await expect(page.getByTestId("security-score-value")).toContainText("78");
    await expect(page.getByTestId("btn-rescan-security")).toBeVisible();

    // 2. Verify 6 Core Security Domain KPIs (T11.11 spec)
    await expect(page.getByTestId("security-stats-grid")).toBeVisible();
    await expect(page.getByTestId("kpi-login-failures")).toBeVisible();
    await expect(page.getByTestId("kpi-waf-blocks")).toBeVisible();
    await expect(page.getByTestId("kpi-malware")).toBeVisible();
    await expect(page.getByTestId("kpi-admins-without-2fa")).toBeVisible();
    await expect(page.getByTestId("kpi-expiring-ssl")).toBeVisible();
    await expect(page.getByTestId("kpi-outdated-images")).toBeVisible();

    // 3. Verify Detected Issues Table
    await expect(page.getByTestId("security-issues-card")).toBeVisible();

    // Verify all 6 mandatory categories are represented in the list
    await expect(page.getByTestId("security-issue-row-sec-issue-auth-failures")).toBeVisible();
    await expect(page.getByTestId("security-issue-row-sec-issue-waf-attacks")).toBeVisible();
    await expect(page.getByTestId("security-issue-row-sec-issue-malware-eicar")).toBeVisible();
    await expect(page.getByTestId("security-issue-row-sec-issue-admins-no-2fa")).toBeVisible();
    await expect(page.getByTestId("security-issue-row-sec-issue-ssl-expiring")).toBeVisible();
    await expect(page.getByTestId("security-issue-row-sec-issue-outdated-image")).toBeVisible();

    // 4. Verify DoD: "প্রতিটা সমস্যার পাশে 'ঠিক করো' link আছে।"
    const authFixBtn = page.getByTestId("btn-fix-issue-sec-issue-auth-failures");
    const wafFixBtn = page.getByTestId("btn-fix-issue-sec-issue-waf-attacks");
    const malwareFixBtn = page.getByTestId("btn-fix-issue-sec-issue-malware-eicar");
    const admin2faFixBtn = page.getByTestId("btn-fix-issue-sec-issue-admins-no-2fa");
    const sslFixBtn = page.getByTestId("btn-fix-issue-sec-issue-ssl-expiring");
    const imageFixBtn = page.getByTestId("btn-fix-issue-sec-issue-outdated-image");

    await expect(authFixBtn).toBeVisible();
    await expect(wafFixBtn).toBeVisible();
    await expect(malwareFixBtn).toBeVisible();
    await expect(admin2faFixBtn).toBeVisible();
    await expect(sslFixBtn).toBeVisible();
    await expect(imageFixBtn).toBeVisible();

    // 5. Test Remediation Flow on Malware Issue
    await malwareFixBtn.click();
    await expect(page.getByTestId("fix-issue-dialog")).toBeVisible();
    await expect(page.getByTestId("fix-dialog-title")).toBeVisible();
    await expect(page.getByTestId("btn-confirm-fix")).toBeVisible();

    // Confirm the remediation
    await page.getByTestId("btn-confirm-fix").click();

    // Verify toast notification appears
    await expect(
      page.getByText(/Remediation Applied|সমাধান প্রয়োগ করা হয়েছে/i)
    ).toBeVisible();

    // 6. Test Category Filter
    const catWafBtn = page.getByTestId("btn-filter-cat-waf");
    await catWafBtn.click();
    await expect(page.getByTestId("security-issue-row-sec-issue-waf-attacks")).toBeVisible();
    await expect(page.getByTestId("security-issue-row-sec-issue-auth-failures")).not.toBeVisible();

    // Reset filter
    const catAllBtn = page.getByTestId("btn-filter-cat-all");
    await catAllBtn.click();
    await expect(page.getByTestId("security-issue-row-sec-issue-auth-failures")).toBeVisible();

    // 7. Verify Resolved Filter Tab
    const resolvedTab = page.getByTestId("filter-status-resolved");
    await resolvedTab.click();
    await expect(page.getByTestId("security-issue-row-sec-issue-malware-eicar")).toBeVisible();
  });
});
