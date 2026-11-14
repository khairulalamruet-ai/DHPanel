import { test, expect } from "@playwright/test";

test.describe("Autoinstaller & WordPress Manager UI (T12.06 & T12.03)", () => {
  test("Client: views installed apps, installs new app via wizard (T12.06 DoD), and updates WordPress plugin (T12.03 DoD)", async ({
    page,
  }) => {
    // 1. Navigate to client apps portal
    await page.goto("/apps");

    await expect(page.getByTestId("apps-page-title")).toBeVisible();
    await expect(page.getByTestId("tab-installed-apps")).toBeVisible();
    await expect(page.getByTestId("tab-app-catalog")).toBeVisible();

    // 2. Verify Installed Apps table (T12.06 DoD: "installed app তালিকা")
    await expect(page.getByTestId("installed-apps-table")).toBeVisible();
    await expect(page.getByTestId("app-row-app-wp-01")).toBeVisible();
    await expect(page.getByTestId("installed-app-name-app-wp-01")).toContainText("WordPress");
    await expect(page.getByTestId("link-visit-app-app-wp-01")).toBeVisible();

    // 3. Switch to App Store / Catalog tab (T12.06 DoD: "Catalog grid")
    const catalogTab = page.getByTestId("tab-app-catalog");
    await catalogTab.click();

    await expect(page.getByTestId("catalog-grid")).toBeVisible();
    await expect(page.getByTestId("catalog-card-wordpress")).toBeVisible();
    await expect(page.getByTestId("catalog-card-joomla")).toBeVisible();
    await expect(page.getByTestId("catalog-card-laravel")).toBeVisible();
    await expect(page.getByTestId("catalog-card-static")).toBeVisible();

    // 4. Install wizard flow (T12.06 DoD: "install wizard")
    const installJoomlaBtn = page.getByTestId("btn-install-joomla");
    await installJoomlaBtn.click();

    await expect(page.getByTestId("install-wizard-dialog")).toBeVisible();
    await expect(page.getByTestId("install-wizard-title")).toContainText(/Joomla/i);

    // Fill form
    await page.getByTestId("input-site-name").fill("My Corporate Joomla Portal");
    await page.getByTestId("input-install-path").fill("joomla");
    await page.getByTestId("input-admin-username").fill("joomla_admin");
    await page.getByTestId("input-admin-password").fill("SuperSecret!2026");

    // Submit installation
    const submitInstallBtn = page.getByTestId("btn-submit-install");
    await submitInstallBtn.click();

    // Verify success view
    await expect(page.getByTestId("install-success-view")).toBeVisible();
    await expect(page.getByTestId("link-installed-site")).toBeVisible();

    // Finish install and return to installed apps table
    await page.getByTestId("btn-finish-install").click();
    await expect(page.getByTestId("installed-apps-table")).toBeVisible();

    // 5. WordPress Manager: plugin update (T12.03 DoD: "Plugin update UI থেকে হয়।")
    const manageWpBtn = page.getByTestId("btn-manage-app-app-wp-01");
    await manageWpBtn.click();

    await expect(page.getByTestId("wp-manager-dialog")).toBeVisible();
    await expect(page.getByTestId("wp-manager-title")).toContainText("WordPress Manager");

    // Switch to Plugins tab
    const pluginsTab = page.getByTestId("tab-wp-plugins");
    await pluginsTab.click();

    await expect(page.getByTestId("wp-plugins-table")).toBeVisible();
    await expect(page.getByTestId("plugin-row-woocommerce")).toBeVisible();

    // Verify update button is present
    const updateWooBtn = page.getByTestId("btn-update-plugin-woocommerce");
    await expect(updateWooBtn).toBeVisible();

    // Click Update Plugin (DoD verification)
    await updateWooBtn.click();

    // Verify toast or updated state
    await expect(
      page.getByText(/Plugin Updated|প্লাগইন আপডেট সম্পন্ন/i)
    ).toBeVisible();
  });
});
