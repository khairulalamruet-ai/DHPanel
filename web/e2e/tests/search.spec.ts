import { test, expect } from "@playwright/test";

test.describe("Global Search & Command Palette (T18.09)", () => {
  test("Admin/Client: opens palette via Ctrl+K and header trigger, searches pages and remote domains, and navigates", async ({
    page,
  }) => {
    // 1. Navigate to Admin Panel
    await page.goto("/");

    // Verify search trigger in header
    const header = page.locator("header");
    const searchTrigger = header.getByTestId("command-palette-trigger");
    await expect(searchTrigger).toBeVisible();

    // 2. Open palette via keyboard shortcut Ctrl+K
    await page.keyboard.press("Control+k");

    await expect(page.getByTestId("command-palette-dialog")).toBeVisible();
    const searchInput = page.getByTestId("command-palette-input");
    await expect(searchInput).toBeFocused();

    // 3. Search for pages (e.g., "domains")
    await searchInput.fill("domains");
    await expect(page.getByTestId("search-result-item-page-domains")).toBeVisible();

    // Select with Enter
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("command-palette-dialog")).not.toBeVisible();
    await expect(page).toHaveURL(/\/domains/);

    // 4. Open palette via header trigger button
    await page.locator("header").getByTestId("command-palette-trigger").click();
    await expect(page.getByTestId("command-palette-dialog")).toBeVisible();

    // Search for remote domain "apexstore"
    const input2 = page.getByTestId("command-palette-input");
    await input2.fill("apexstore");

    const domainResult = page.getByTestId("search-result-item-domain-apexstore");
    await expect(domainResult).toBeVisible();
    await expect(domainResult).toContainText("apexstore.com");

    // Click result
    await domainResult.click();
    await expect(page.getByTestId("command-palette-dialog")).not.toBeVisible();

    // 5. Test Escape closes command palette
    await page.keyboard.press("Control+k");
    await expect(page.getByTestId("command-palette-dialog")).toBeVisible();

    await page.keyboard.press("Escape");
    await expect(page.getByTestId("command-palette-dialog")).not.toBeVisible();
  });
});
