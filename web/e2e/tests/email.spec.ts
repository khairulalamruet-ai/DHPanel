import { test, expect } from "@playwright/test";

test.describe("Email Management UI (T13.12, T13.10, T13.11)", () => {
  test("Client: manages mailboxes, forwarders, autoresponders, and views IMAP/SMTP device config (T13.12 DoD)", async ({
    page,
  }) => {
    // 1. Navigate to client email portal
    await page.goto("/email");

    await expect(page.getByTestId("email-page-title")).toBeVisible();
    await expect(page.getByTestId("tab-mailboxes")).toBeVisible();
    await expect(page.getByTestId("tab-forwarders")).toBeVisible();
    await expect(page.getByTestId("tab-autoresponders")).toBeVisible();
    await expect(page.getByTestId("tab-config")).toBeVisible();

    // 2. Mailboxes Table (T13.12: Mailbox management)
    await expect(page.getByTestId("mailboxes-section")).toBeVisible();
    await expect(page.getByTestId("mailbox-row-mb-1")).toBeVisible();
    await expect(page.getByText("info@example.com")).toBeVisible();
    await expect(page.getByTestId("webmail-btn-mb-1")).toBeVisible();

    // Create new mailbox
    const createMailboxBtn = page.getByTestId("create-mailbox-btn");
    await createMailboxBtn.click();

    await expect(page.getByTestId("create-mailbox-dialog")).toBeVisible();
    await page.getByTestId("mailbox-username-input").fill("support-team");
    await page.getByTestId("mailbox-password-input").fill("SecurePass@2026!");
    await page.getByTestId("submit-mailbox-btn").click();

    // Verify created
    await expect(page.getByTestId("mailboxes-section").getByText("support-team@example.com")).toBeVisible();

    // 3. Forwarders Tab (T13.12: Forwarder management)
    const forwardersTab = page.getByTestId("tab-forwarders");
    await forwardersTab.click();

    await expect(page.getByTestId("forwarders-section")).toBeVisible();
    await expect(page.getByTestId("forwarder-row-fwd-1")).toBeVisible();
    await expect(page.getByText("sales@example.com")).toBeVisible();
    await expect(page.getByText("sales-team@external-crm.com")).toBeVisible();

    // Add Forwarder
    const addFwdBtn = page.getByTestId("add-forwarder-btn");
    await addFwdBtn.click();

    await expect(page.getByTestId("create-forwarder-dialog")).toBeVisible();
    await page.getByTestId("forwarder-source-input").fill("press");
    await page.getByTestId("forwarder-destination-input").fill("media@external.org");
    await page.getByTestId("submit-forwarder-btn").click();

    await expect(page.getByTestId("forwarders-section").getByText("press@example.com")).toBeVisible();
    await expect(page.getByTestId("forwarders-section").getByText("media@external.org")).toBeVisible();

    // 4. Autoresponders Tab (T13.12: Autoresponder management)
    const autorespondersTab = page.getByTestId("tab-autoresponders");
    await autorespondersTab.click();

    await expect(page.getByTestId("autoresponders-section")).toBeVisible();
    await expect(page.getByTestId("autoresponder-row-ar-1")).toBeVisible();
    await expect(page.getByText("support@example.com")).toBeVisible();
    await expect(page.getByText(/We received your ticket/i)).toBeVisible();

    // 5. Connect Devices / IMAP & SMTP Settings (T13.12 DoD: "client config তথ্য (IMAP/SMTP setting)")
    const configTab = page.getByTestId("tab-config");
    await configTab.click();

    await expect(page.getByTestId("client-config-card")).toBeVisible();
    await expect(page.getByTestId("incoming-server-val")).toContainText("mail.example.com");
    await expect(page.getByTestId("imap-port-val")).toContainText("993");
    await expect(page.getByTestId("pop3-port-val")).toContainText("995");
    await expect(page.getByTestId("outgoing-server-val")).toContainText("mail.example.com");
    await expect(page.getByTestId("smtp-port-val")).toContainText("465 (or 587)");
    await expect(page.getByTestId("copy-incoming-server")).toBeVisible();
  });

  test("Admin: monitors Postfix mail queue and checks domain deliverability authentication (T13.10 & T13.11)", async ({
    page,
  }) => {
    // 1. Navigate to admin mail queue & deliverability page
    await page.goto("/admin/mail");

    await expect(page.getByTestId("mail-queue-title")).toBeVisible();
    await expect(page.getByTestId("tab-admin-queue")).toBeVisible();
    await expect(page.getByTestId("tab-admin-deliverability")).toBeVisible();

    // 2. Mail queue view
    await expect(page.getByTestId("queue-table-section")).toBeVisible();
    await expect(page.getByTestId("queue-row-4YZ8Q91024B")).toBeVisible();
    await expect(page.getByText("notifications@example.com")).toBeVisible();
    await expect(page.getByText("user123@remote-domain.com")).toBeVisible();

    // 3. Flush queue action
    const flushBtn = page.getByTestId("flush-queue-btn");
    await expect(flushBtn).toBeVisible();
    await flushBtn.click();

    await expect(page.getByText(/Queue flush initiated|কিউ ফ্লাশ শুরু হয়েছে/i)).toBeVisible();

    // 4. Deliverability view (SPF/DKIM/DMARC)
    const deliverabilityTab = page.getByTestId("tab-admin-deliverability");
    await deliverabilityTab.click();

    await expect(page.getByTestId("deliverability-section")).toBeVisible();
    await expect(page.getByTestId("deliverability-card-example.com")).toBeVisible();
    await expect(page.getByTestId("deliverability-card-example.com").getByText(/DKIM selector/i)).toBeVisible();
    await expect(page.getByTestId("deliverability-card-example.com").getByText(/Clean IP Reputation|পরিচ্ছন্ন আইপি সুনাম/i)).toBeVisible();
  });
});
