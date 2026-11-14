import { test, expect } from "@playwright/test";

test.describe("Hosting Plans UI (T04.04)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/plans");
  });

  test("loads plans page with list, resources, and assigned users count", async ({ page }) => {
    await expect(page.getByText(/Hosting Plans|হোস্টিং প্ল্যান/i).first()).toBeVisible();

    // Verify seeded plans
    await expect(page.getByText("Standard Business")).toBeVisible();
    await expect(page.getByText("Starter Cloud")).toBeVisible();
    await expect(page.getByText("Pro Developer")).toBeVisible();
    await expect(page.getByText("Enterprise Dedicated")).toBeVisible();

    // Verify assigned users count (DoD)
    await expect(page.getByText(/24 জন ব্যবহারকারী|24 users/i)).toBeVisible();
  });

  test("DoD: creates a new hosting plan with unlimited toggles via UI dialog", async ({ page }) => {
    const newBtn = page.getByRole("button", { name: /New Plan|নতুন প্ল্যান/i });
    await newBtn.click();

    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();

    // Fill Plan Name
    const nameInput = dialog.getByLabel(/Plan Name|প্ল্যানের নাম/i);
    await nameInput.fill("E2E Super Cloud");

    // Toggle Unlimited on a resource
    const unlimitedBtn = dialog.getByRole("button", { name: /Unlimited|আনলিমিটেড/i }).first();
    await unlimitedBtn.click();

    // Save Plan
    const saveBtn = dialog.getByRole("button", { name: /Save Plan|সংরক্ষণ করুন/i });
    await saveBtn.click();

    await expect(dialog).not.toBeVisible();
    await expect(page.locator("table").getByText("E2E Super Cloud")).toBeVisible();
  });

  test("DoD: edits an existing hosting plan via UI dialog", async ({ page }) => {
    const starterRow = page.locator("tr", { hasText: "Starter Cloud" });
    await expect(starterRow).toBeVisible();

    const editBtn = starterRow.getByRole("button", { name: /Edit|সম্পাদনা/i });
    await editBtn.click();

    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();

    const nameInput = dialog.getByLabel(/Plan Name|প্ল্যানের নাম/i);
    await nameInput.fill("Starter Cloud E2E Renamed");

    const saveBtn = dialog.getByRole("button", { name: /Save Plan|সংরক্ষণ করুন/i });
    await saveBtn.click();

    await expect(dialog).not.toBeVisible();
    await expect(page.locator("table").getByText("Starter Cloud E2E Renamed")).toBeVisible();
  });

  test("DoD T16.03: binds a hosting plan to a node group with region and tags", async ({ page }) => {
    // Verify existing node groups in table
    await expect(page.locator("table").getByText("US-East General Pool")).toBeVisible();
    await expect(page.locator("table").getByText("Singapore High-RAM Pool")).toBeVisible();

    const newBtn = page.getByRole("button", { name: /New Plan|নতুন প্ল্যান/i });
    await newBtn.click();

    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();

    // Fill Plan Name
    const nameInput = dialog.getByLabel(/Plan Name|প্ল্যানের নাম/i);
    await nameInput.fill("Frankfurt High-Security");

    // Select Frankfurt EU-West Group
    const groupSelect = dialog.getByTestId("plan-node-group-select");
    await groupSelect.selectOption("ng_fra_isolated");

    // Region badge in preview
    await expect(dialog.getByTestId("selected-group-region")).toContainText("eu-central-1");

    // Save
    const saveBtn = dialog.getByRole("button", { name: /Save Plan|সংরক্ষণ করুন/i });
    await saveBtn.click();

    await expect(dialog).not.toBeVisible();

    // Verify row has Frankfurt EU-West Group badge and region
    const row = page.locator("tr", { hasText: "Frankfurt High-Security" });
    await expect(row).toBeVisible();
    await expect(row.getByTestId("plan-node-group-badge")).toContainText("Frankfurt EU-West Group");
    await expect(row.getByText("eu-central-1")).toBeVisible();
  });
});
