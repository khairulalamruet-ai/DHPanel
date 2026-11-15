import { test, expect } from "@playwright/test";

test.describe("Firewall & IP Access Control UI (T11.08)", () => {
  test("Client: blocks and unblocks IP for site at Caddy level (DoD)", async ({ page }) => {
    // 1. Navigate to client firewall page
    await page.goto("/firewall");

    await expect(page.getByTestId("firewall-page-title")).toBeVisible();
    await expect(page.getByTestId("select-firewall-domain")).toBeVisible();
    await expect(page.getByTestId("card-blocked-ips-count")).toBeVisible();

    // Verify initial rules
    await expect(page.getByTestId("title-ip-rules")).toBeVisible();
    await expect(page.getByTestId("ip-rule-row-ip-rule-1")).toBeVisible();
    await expect(page.getByText("198.51.100.42")).toBeVisible();

    // 2. DoD: Block a new IP for site
    const openBlockBtn = page.getByTestId("btn-open-add-ip-rule");
    await expect(openBlockBtn).toBeVisible();
    await openBlockBtn.click();

    await expect(page.getByTestId("add-ip-rule-title")).toBeVisible();

    const ipInput = page.getByTestId("input-ip-cidr");
    await ipInput.fill("203.0.113.88");

    const notesInput = page.getByTestId("input-rule-notes");
    await notesInput.fill("Malicious brute force bot");

    const submitBtn = page.getByTestId("btn-confirm-add-ip-rule");
    await expect(submitBtn).toBeEnabled();
    await submitBtn.click();

    // 3. Verify success toast
    await expect(
      page.getByText(/has been blocked for|সফলভাবে.*ব্লক করা হয়েছে/i)
    ).toBeVisible();

    // 4. Verify unblock / delete action
    const deleteBtn = page.getByTestId("btn-delete-rule-ip-rule-1");
    await expect(deleteBtn).toBeVisible();
    await deleteBtn.click();

    await expect(
      page.getByText(/rule for.*has been removed|রুল সফলভাবে মুছে ফেলা হয়েছে/i)
    ).toBeVisible();
  });

  test("Admin: inspects node ports, verifies SSH lockout protection, and manages global bans", async ({ page }) => {
    // Navigate to admin firewall
    await page.goto("/admin/firewall");

    await expect(page.getByTestId("admin-firewall-title")).toBeVisible();
    await expect(page.getByTestId("select-admin-firewall-node")).toBeVisible();
    await expect(page.getByTestId("card-admin-firewall-stats")).toBeVisible();

    // Verify SSH port 22 is present and protected
    const port22Row = page.getByTestId("port-row-22");
    await expect(port22Row).toBeVisible();
    await expect(port22Row.getByText(/Protected \(SSH \/ Web\)|সুরক্ষিত \(SSH \/ Web\)/i)).toBeVisible();

    // Switch to IP bans tab
    const bansTab = page.getByTestId("tab-firewall-bans");
    await expect(bansTab).toBeVisible();
    await bansTab.click();

    await expect(page.getByTestId("title-global-bans")).toBeVisible();
    await expect(page.getByTestId("ban-row-ban-1")).toBeVisible();

    // Add a manual global ban
    const addBanBtn = page.getByTestId("btn-open-add-ban");
    await addBanBtn.click();

    await expect(page.getByTestId("add-global-ban-title")).toBeVisible();
    await page.getByTestId("input-global-ban-ip").fill("198.51.100.200");
    await page.getByTestId("input-global-ban-reason").fill("DDoS botnet node");

    await page.getByTestId("btn-confirm-add-global-ban").click();

    await expect(
      page.getByText(/has been banned across the node|নোডে ব্যান করা হয়েছে/i)
    ).toBeVisible();

    // Unban IP
    const unbanBtn = page.getByTestId("btn-unban-ban-1");
    await expect(unbanBtn).toBeVisible();
    await unbanBtn.click();

    await expect(
      page.getByText(/has been unbanned|আনব্যান করা হয়েছে/i)
    ).toBeVisible();
  });
});
