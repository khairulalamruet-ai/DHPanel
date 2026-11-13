import { test, expect } from "@playwright/test";

test.describe("Client Database Management UI (T07.10)", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate directly to the client databases page
    await page.goto("/databases");
  });

  test("loads databases page with metric cards, tables, and phpMyAdmin SSO button", async ({ page }) => {
    // Page Header
    await expect(page.getByTestId("databases-title")).toBeVisible();

    // Verify phpMyAdmin SSO button
    await expect(page.getByTestId("btn-phpmyadmin-sso")).toBeVisible();

    // Verify Metric Cards
    await expect(page.getByTestId("metric-db-count")).toBeVisible();
    await expect(page.getByTestId("metric-user-count")).toBeVisible();
    await expect(page.getByTestId("metric-db-size")).toBeVisible();

    // Seed database rows
    await expect(page.getByTestId("db-row-demo_wordpress")).toBeVisible();
    await expect(page.getByTestId("db-row-demo_analytics")).toBeVisible();
  });

  test("creates a new database with prefix enforcement", async ({ page }) => {
    await expect(page.getByTestId("btn-open-create-db")).toBeVisible();
    await page.getByTestId("btn-open-create-db").click();

    // Fill form
    const nameInput = page.getByTestId("input-db-name");
    await expect(nameInput).toBeVisible();
    await nameInput.fill("store");

    // Submit
    await page.getByTestId("btn-submit-create-db").click();

    // Toast and new row in table
    await expect(page.getByText(/created successfully|সফলভাবে তৈরি হয়েছে/i)).toBeVisible();
    await expect(page.getByTestId("db-row-demo_store")).toBeVisible();
  });

  test("creates a new database user and assigns privileges", async ({ page }) => {
    // Switch to Database Users tab
    await page.getByTestId("subtab-users").click();

    await expect(page.getByTestId("btn-open-create-user")).toBeVisible();
    await page.getByTestId("btn-open-create-user").click();

    // Fill username
    const usernameInput = page.getByTestId("input-db-username");
    await expect(usernameInput).toBeVisible();
    await usernameInput.fill("appworker");

    // Generate password
    await page.getByTestId("btn-generate-db-password").click();

    // Submit
    await page.getByTestId("btn-submit-create-user").click();

    // Toast and new row in users table
    await expect(page.getByText(/created successfully|সফলভাবে তৈরি হয়েছে/i)).toBeVisible();
    await expect(page.getByTestId("user-row-demo_appworker")).toBeVisible();
  });

  test("manages user database privileges using preset", async ({ page }) => {
    await page.getByTestId("subtab-users").click();

    // Open grants dialog for demo_wpuser
    await expect(page.getByTestId("btn-manage-grants-dbusr_01")).toBeVisible();
    await page.getByTestId("btn-manage-grants-dbusr_01").click();

    // Preset Read Only
    await expect(page.getByTestId("preset-readonly")).toBeVisible();
    await page.getByTestId("preset-readonly").click();

    // Save
    await page.getByTestId("btn-save-grants").click();

    await expect(page.getByText(/Privileges updated|অনুমতি আপডেট করা হয়েছে/i)).toBeVisible();
  });

  test("configures remote database CIDR access allowlist", async ({ page }) => {
    await expect(page.getByTestId("btn-remote-access")).toBeVisible();
    await page.getByTestId("btn-remote-access").click();

    const input = page.getByTestId("input-remote-ip");
    await expect(input).toBeVisible();
    await input.fill("192.0.2.144/28");
    await page.getByTestId("btn-add-remote-ip").click();

    // Save
    await page.getByTestId("btn-save-remote-hosts").click();

    await expect(page.getByText(/allowlist updated|তালিকা আপডেট হয়েছে/i)).toBeVisible();
  });

  test("deletes a database with confirmation dialog", async ({ page }) => {
    await expect(page.getByTestId("btn-delete-db-db_01")).toBeVisible();
    await page.getByTestId("btn-delete-db-db_01").click();

    const confirmBtn = page.getByTestId("btn-confirm-delete-db");
    await expect(confirmBtn).toBeVisible();
    await confirmBtn.click();

    await expect(page.getByText(/deleted|মুছে ফেলা হয়েছে/i)).toBeVisible();
    await expect(page.getByTestId("db-row-demo_wordpress")).not.toBeVisible();
  });
});
