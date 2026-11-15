import { test, expect } from "@playwright/test";

test.describe("Users Management UI (T04.15)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/users");
  });

  test("DoD: loads users table with pagination for 1,000 users", async ({ page }) => {
    // Header & title
    await expect(page.getByText(/User Accounts|ব্যবহারকারী অ্যাকাউন্টসমূহ/i).first()).toBeVisible();

    // Stats cards
    await expect(page.getByText(/Total Users|মোট ব্যবহারকারী/i)).toBeVisible();
    await expect(page.getByText(/1,000|১,০০০/).first()).toBeVisible();

    // Table rows
    await expect(page.locator("table").getByText("alpha", { exact: true })).toBeVisible();
    await expect(page.locator("table").getByText("beta", { exact: true })).toBeVisible();

    // Pagination info
    await expect(page.getByText(/Page 1 of 100|[1১][0০][0০] পৃষ্ঠার মধ্যে [1১] নং পৃষ্ঠা/i)).toBeVisible();

    // Navigate to page 2
    const nextBtn = page.getByRole("button", { name: /Next page|পরবর্তী পৃষ্ঠা/i });
    await expect(nextBtn).toBeEnabled();
    await nextBtn.click();

    await expect(page.getByText(/Page 2 of 100|[1১][0০][0০] পৃষ্ঠার মধ্যে [2২] নং পৃষ্ঠা/i)).toBeVisible({ timeout: 10000 });
  });

  test("filters users table by search keyword", async ({ page }) => {
    const searchInput = page.getByPlaceholder(/Search by username or email|ইউজারনেম বা ইমেইল দিয়ে খুঁজুন/i);
    await searchInput.fill("solaris");

    await expect(page.locator("table").getByText("solaris", { exact: true })).toBeVisible();
    await expect(page.locator("table").getByText("alpha", { exact: true })).not.toBeVisible();
  });

  test("performs bulk selection and bulk plan change", async ({ page }) => {
    // Select first row checkbox
    const firstCheckbox = page.locator("tbody tr").first().getByRole("checkbox");
    await firstCheckbox.check();

    // Bulk bar appears
    await expect(
      page.getByText(/[1১] (users selected|জন ব্যবহারকারী নির্বাচিত)/i)
    ).toBeVisible();

    // Click Change Plan button
    const changePlanBtn = page.getByRole("button", { name: /^(Change Plan|প্ল্যান পরিবর্তন)$/i });
    await changePlanBtn.click();

    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();

    // Select target plan
    const planSelect = dialog.locator("select");
    await planSelect.selectOption("plan_pro");

    // Click apply
    const applyBtn = dialog.getByRole("button", { name: /Apply Plan|প্ল্যান প্রয়োগ করুন/i });
    await applyBtn.click();

    await expect(dialog).not.toBeVisible();
    // Selection is cleared
    await expect(page.getByText(/users selected|ব্যবহারকারী নির্বাচিত/i)).not.toBeVisible();
  });

  test("performs bulk suspend on selected accounts", async ({ page }) => {
    const firstCheckbox = page.locator("tbody tr").first().getByRole("checkbox");
    await firstCheckbox.check();

    const suspendBtn = page.getByRole("button", { name: /^(Suspend|স্থগিত করুন)$/i });
    await suspendBtn.click();

    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();

    const confirmBtn = dialog.getByRole("button", { name: /Suspend Account|স্থগিত করুন/i });
    await confirmBtn.click();

    await expect(dialog).not.toBeVisible();
  });

  test("displays node offline badge for tenants hosted on offline nodes (T16.07)", async ({ page }) => {
    // In mockUsers, users on node-nyc-01 (e.g. gamma) are on offline nodes
    await expect(page.locator("table").getByText("gamma", { exact: true })).toBeVisible();

    // Verify tenant-node-offline-badge is visible
    const offlineBadge = page.getByTestId("tenant-node-offline-badge").first();
    await expect(offlineBadge).toBeVisible();
    await expect(offlineBadge).toContainText(/Node Offline|নোড অফলাইন/i);
  });
});
