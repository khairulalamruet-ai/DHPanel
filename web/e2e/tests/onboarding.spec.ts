import { test, expect } from "@playwright/test";

test.describe("Onboarding & Tour (T18.07)", () => {
  test("admin quickstart checklist displays, toggles, dismisses, and stays dismissed on reload", async ({
    page,
  }) => {
    // Navigate to admin dashboard
    await page.goto("/");

    // 1. Quickstart card is visible
    const quickstartCard = page.getByTestId("admin-quickstart-card");
    await expect(quickstartCard).toBeVisible();

    // 2. Contains setup tasks
    await expect(page.getByTestId("quickstart-step-add_node")).toBeVisible();
    await expect(page.getByTestId("quickstart-step-create_plan")).toBeVisible();
    await expect(page.getByTestId("quickstart-step-configure_dns")).toBeVisible();
    await expect(page.getByTestId("quickstart-step-setup_smtp")).toBeVisible();

    // 3. Toggle a step
    const togglePlanBtn = page.getByTestId("btn-toggle-step-create_plan");
    await togglePlanBtn.click();

    // 4. Click dismiss button
    const dismissBtn = page.getByTestId("btn-dismiss-quickstart");
    await dismissBtn.click();

    // 5. Checklist is immediately hidden
    await expect(quickstartCard).not.toBeVisible();

    // 6. Reload page and verify it remains hidden (DoD)
    await page.reload();
    await expect(page.getByTestId("admin-quickstart-card")).not.toBeVisible();
  });

  test("client welcome tour navigates through steps, dismisses, and stays dismissed on reload", async ({
    page,
  }) => {
    // Navigate to client dashboard
    await page.goto("/client");

    // 1. Client tour card is visible
    const tourCard = page.getByTestId("client-welcome-tour-card");
    await expect(tourCard).toBeVisible();

    // 2. Step 1 is active with Domains link
    await expect(page.getByTestId("btn-tour-goto-tour_domains")).toBeVisible();

    // 3. Navigate forward with Next
    const nextBtn = page.getByTestId("btn-tour-next");
    await nextBtn.click();

    // Step 2 active with Files link
    await expect(page.getByTestId("btn-tour-goto-tour_files")).toBeVisible();

    // Navigate to step 3 and 4
    await nextBtn.click();
    await expect(page.getByTestId("btn-tour-goto-tour_databases")).toBeVisible();

    await nextBtn.click();
    await expect(page.getByTestId("btn-tour-goto-tour_security")).toBeVisible();

    // 4. Finish/Dismiss tour
    const finishBtn = page.getByTestId("btn-tour-finish");
    await finishBtn.click();

    // 5. Tour card is immediately hidden
    await expect(tourCard).not.toBeVisible();

    // 6. Reload page and verify it remains hidden (DoD)
    await page.reload();
    await expect(page.getByTestId("client-welcome-tour-card")).not.toBeVisible();
  });
});
