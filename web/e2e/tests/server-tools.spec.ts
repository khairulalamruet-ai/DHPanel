import { test, expect } from "@playwright/test";

test.describe("Server Tools & Maintenance UI (T10.15)", () => {
  test("Admin: configures timezone, swap, drops caches, verifies SSH config, initiates reboot, and audits every action (DoD)", async ({ page }) => {
    // 1. Navigate to admin server tools page
    await page.goto("/admin/server-tools");

    await expect(page.getByTestId("server-tools-title")).toBeVisible();
    await expect(page.getByTestId("card-server-overview")).toBeVisible();
    await expect(page.getByTestId("text-node-hostname")).toBeVisible();

    // Verify all tool cards are mounted
    await expect(page.getByTestId("title-timezone-config")).toBeVisible();
    await expect(page.getByTestId("title-swap-management")).toBeVisible();
    await expect(page.getByTestId("title-drop-caches")).toBeVisible();
    await expect(page.getByTestId("title-ssh-config")).toBeVisible();
    await expect(page.getByTestId("title-reboot-server")).toBeVisible();
    await expect(page.getByTestId("title-server-audit-trail")).toBeVisible();

    // 2. Test Timezone update with sudo password
    const tzSelect = page.getByTestId("select-timezone");
    await expect(tzSelect).toBeVisible();
    await tzSelect.selectOption("Asia/Dhaka");

    const tzSudoInput = page.getByTestId("input-tz-sudo");
    await tzSudoInput.fill("SuperSudo123!");

    const saveTzBtn = page.getByTestId("btn-save-timezone");
    await expect(saveTzBtn).toBeEnabled();
    await saveTzBtn.click();

    await expect(
      page.getByText(/timezone successfully updated|টাইমজোন সফলভাবে/i)
    ).toBeVisible();

    // 3. Test Swap configuration
    const preset8gb = page.getByTestId("btn-swap-preset-8192");
    await preset8gb.click();

    const swapSudoInput = page.getByTestId("input-swap-sudo");
    await swapSudoInput.fill("SuperSudo123!");

    const saveSwapBtn = page.getByTestId("btn-save-swap");
    await expect(saveSwapBtn).toBeEnabled();
    await saveSwapBtn.click();

    await expect(
      page.getByText(/Swap configured to|সোয়াপ.*কনফিগার করা হয়েছে/i)
    ).toBeVisible();

    // 4. Test Drop Caches
    const dropCachesSudo = page.getByTestId("input-drop-caches-sudo");
    await dropCachesSudo.fill("SuperSudo123!");

    const dropCachesBtn = page.getByTestId("btn-execute-drop-caches");
    await expect(dropCachesBtn).toBeEnabled();
    await dropCachesBtn.click();

    await expect(
      page.getByText(/Dropped.*clean kernel caches|ক্লিন কার্নেল ক্যাশ সফলভাবে ড্রপ/i)
    ).toBeVisible();

    // 5. Test Reboot Dialog with confirmation text
    const openRebootBtn = page.getByTestId("btn-open-reboot");
    await openRebootBtn.click();

    await expect(page.getByTestId("reboot-dialog-title")).toBeVisible();
    const confirmRebootBtn = page.getByTestId("btn-confirm-reboot");
    await expect(confirmRebootBtn).toBeDisabled();

    // Fill typed confirmation and sudo password
    await page.getByTestId("input-reboot-confirm").fill("REBOOT");
    await page.getByTestId("input-reboot-sudo").fill("SuperSudo123!");

    await expect(confirmRebootBtn).toBeEnabled();
    await confirmRebootBtn.click();

    await expect(
      page.getByText(/Reboot Initiated|রিবুট শুরু হয়েছে/i)
    ).toBeVisible();

    // 6. Verify DoD: All operations appear in the audit trail
    await expect(page.getByText(/server\.set_timezone/i).first()).toBeVisible();
    await expect(page.getByText(/server\.configure_swap/i).first()).toBeVisible();
    await expect(page.getByText(/server\.drop_caches/i).first()).toBeVisible();
    await expect(page.getByText(/server\.reboot/i).first()).toBeVisible();
  });
});
