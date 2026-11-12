import { test, expect } from "@playwright/test";

test.describe("User Detail Page (T04.17)", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate directly to user detail page
    await page.goto("/users/usr_0001");
  });

  test("loads user header with identity, badges, and container status", async ({ page }) => {
    // Username heading
    await expect(page.getByRole("heading", { level: 1, name: "alpha" })).toBeVisible();

    // Active status badge
    await expect(page.getByText(/Active|সক্রিয়/i).first()).toBeVisible();

    // Plan name badge
    await expect(page.getByText(/Standard Business/).first()).toBeVisible();

    // Running container status indicator
    await expect(page.getByText(/running/i).first()).toBeVisible();

    // Action buttons toolbar
    await expect(page.getByRole("button", { name: /Login as User|ব্যবহারকারী হিসেবে লগইন/i })).toBeVisible();
    await expect(page.getByRole("button", { name: /Suspend Account|অ্যাকাউন্ট স্থগিত/i })).toBeVisible();
    await expect(page.getByRole("button", { name: /Reset Password|পাসওয়ার্ড রিসেট/i })).toBeVisible();
  });

  test("renders all 6 tabs and verifies data in each tab", async ({ page }) => {
    // 1. Overview tab (default)
    await expect(page.getByText(/Account Identity & Root|অ্যাকাউন্টের পরিচয় ও রুট/i)).toBeVisible();
    await expect(page.getByText("/srv/tenants/alpha")).toBeVisible();
    await expect(page.getByText("1001", { exact: true })).toBeVisible();
    await expect(page.getByText(/Container Runtime Stack|কন্টেইনার রানটাইম স্ট্যাক/i)).toBeVisible();

    // 2. Resources tab
    const resTab = page.getByRole("button", { name: /Resources & Quota|রিসোর্স ও কোটা/i });
    await resTab.click();
    await expect(page.getByText(/Hardware Resource Allocations|হার্ডওয়্যার রিসোর্স বরাদ্দ/i)).toBeVisible();
    await expect(page.getByText(/CPU Cores & Frequency|সিপিইউ কোর/i)).toBeVisible();
    await expect(page.getByText(/Memory \(RAM\)|মেমরি \(র‌্যাম\)/i)).toBeVisible();

    // 3. Domains tab
    const domainsTab = page.getByRole("button", { name: /Domains|ডোমেইনসমূহ/i });
    await domainsTab.click();
    await expect(page.getByText(/Configured Web Domains|কনফিগার করা ওয়েব ডোমেইন/i)).toBeVisible();
    await expect(page.getByText("alpha.com").first()).toBeVisible();
    await expect(page.getByText("PHP 8.3").first()).toBeVisible();

    // 4. Databases tab
    const dbTab = page.getByRole("button", { name: /Databases|ডাটাবেজ/i });
    await dbTab.click();
    await expect(page.getByText(/Database Allocations|ডাটাবেজ বরাদ্দ/i)).toBeVisible();
    await expect(page.getByText("alpha_wp")).toBeVisible();

    // 5. Logs tab
    const logsTab = page.getByRole("button", { name: /Container Logs|কন্টেইনার লগ/i });
    await logsTab.click();
    await expect(page.getByText(/Container Log Streams|কন্টেইনার লগ স্ট্রিম/i)).toBeVisible();
    await expect(page.getByText(/GET \/ HTTP\/2\.0/i)).toBeVisible();

    // 6. Activity tab
    const actTab = page.getByRole("button", { name: /Activity Ledger|অ্যাক্টিভিটি লেজার/i });
    await actTab.click();
    await expect(page.getByText(/Tenant Activity & Audit Ledger|টেন্যান্ট অ্যাক্টিভিটি ও অডিট লেজার/i)).toBeVisible();
    await expect(page.getByText("user.login")).toBeVisible();
  });

  test("toggles suspend/unsuspend status", async ({ page }) => {
    // Click Suspend button
    const suspendBtn = page.getByRole("button", { name: /Suspend Account|অ্যাকাউন্ট স্থগিত/i });
    await suspendBtn.click();

    // Status changes to Suspended
    await expect(page.getByText(/Suspended|স্থগিত/i).first()).toBeVisible();

    // Button changes to Unsuspend
    const unsuspendBtn = page.getByRole("button", { name: /Unsuspend Account|অ্যাকাউন্ট পুনঃসক্রিয়/i });
    await expect(unsuspendBtn).toBeVisible();

    // Click Unsuspend
    await unsuspendBtn.click();
    await expect(page.getByText(/Active|সক্রিয়/i).first()).toBeVisible();
  });

  test("opens reset password dialog and submits new password", async ({ page }) => {
    const resetBtn = page.getByRole("button", { name: /Reset Password|পাসওয়ার্ড রিসেট/i });
    await resetBtn.click();

    // Dialog title
    await expect(page.getByRole("heading", { name: /Reset Password|পাসওয়ার্ড পরিবর্তন/i })).toBeVisible();

    // Input new password
    const passwordInput = page.getByPlaceholder(/Enter new password|নতুন পাসওয়ার্ড দিন/i);
    await passwordInput.fill("SuperSecret!2026");

    // Save
    const saveBtn = page.getByRole("button", { name: /Save New Password|নতুন পাসওয়ার্ড সংরক্ষণ/i });
    await saveBtn.click();

    // Dialog closes
    await expect(page.getByRole("heading", { name: /Reset Password|পাসওয়ার্ড পরিবর্তন/i })).not.toBeVisible();
  });
});
