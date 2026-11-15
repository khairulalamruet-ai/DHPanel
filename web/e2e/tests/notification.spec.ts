import { test, expect } from "@playwright/test";

test.describe("Notification System & Preferences UI (T18.05, T18.03)", () => {
  test("Admin/Client: views notification bell, checks unread dropdown, marks all as read, and configures alert channels & snooze", async ({
    page,
  }) => {
    // 1. Navigate to Admin Panel
    await page.goto("/");

    const header = page.locator("header");

    // Verify Notification Bell in header
    const bellBtn = header.getByTestId("notification-bell-btn");
    await expect(bellBtn).toBeVisible();

    const unreadBadge = header.getByTestId("unread-count-badge");
    await expect(unreadBadge).toBeVisible();
    await expect(unreadBadge).toContainText("3");

    // 2. Open Notification Dropdown Flyout
    await bellBtn.click();

    await expect(page.getByTestId("notification-dropdown")).toBeVisible();
    await expect(page.getByTestId("notification-dropdown-title")).toBeVisible();
    await expect(page.getByText("SSL Certificate Expiring Soon")).toBeVisible();
    await expect(page.getByText("Nightly Cloud Backup Succeeded")).toBeVisible();
    await expect(page.getByText("Worker Node lon-02 Connection Warning")).toBeVisible();

    // 3. Filter tabs: Critical vs All
    await page.getByTestId("tab-notif-critical").click();
    await expect(page.getByText("SSL Certificate Expiring Soon")).toBeVisible();
    await expect(page.getByText("Worker Node lon-02 Connection Warning")).toBeVisible();
    await expect(page.getByText("Nightly Cloud Backup Succeeded")).not.toBeVisible();

    // Switch back to All
    await page.getByTestId("tab-notif-all").click();
    await expect(page.getByText("Nightly Cloud Backup Succeeded")).toBeVisible();

    // 4. Mark all as read
    const markAllBtn = page.getByTestId("btn-mark-all-read");
    await markAllBtn.click();

    // Verify unread badge clears in header
    await expect(header.getByTestId("unread-count-badge")).not.toBeVisible();

    // 5. Navigate to Notification Settings & Channels
    const manageLink = page.getByTestId("link-view-all-notifications");
    await manageLink.click();

    await expect(page.getByTestId("notification-preferences-page")).toBeVisible();
    await expect(page.getByTestId("notification-settings-title")).toBeVisible();
    await expect(page.getByTestId("notification-channels-card")).toBeVisible();
    await expect(page.getByTestId("notification-triggers-card")).toBeVisible();

    // 6. Test Snooze Window
    const snoozeCard = page.getByTestId("snooze-control-card");
    await expect(snoozeCard).toBeVisible();

    const snooze1hBtn = page.getByTestId("btn-snooze-1h");
    await snooze1hBtn.click();

    // Verify snooze banner updates and Resume button appears
    await expect(
      page.getByText(/Notifications Snoozed|নোটিফিকেশন স্থগিত করা হয়েছে/i).first()
    ).toBeVisible();
    await expect(page.getByTestId("btn-clear-snooze")).toBeVisible();

    // Clear snooze
    await page.getByTestId("btn-clear-snooze").click();
    await expect(
      page.getByText(/Snooze Cleared|স্থগিতাদেশ প্রত্যাহার করা হয়েছে/i).first()
    ).toBeVisible();
    await expect(page.getByTestId("btn-snooze-1h")).toBeVisible();

    // 7. Dispatch Test Alert
    const testAlertBtn = page.getByTestId("btn-test-notification");
    await testAlertBtn.click();
    await expect(
      page.getByText(/Test Notification Dispatched|টেস্ট নোটিফিকেশন পাঠানো হয়েছে/i).first()
    ).toBeVisible();

    // 8. Save Notification Preferences
    const saveBtn = page.getByTestId("btn-save-notification-settings");
    await saveBtn.click();
    await expect(
      page.getByText(/Preferences Saved|পছন্দসমূহ সংরক্ষিত/i).first()
    ).toBeVisible();
  });
});
