import { test, expect } from "@playwright/test";

test.describe("Client Dashboard UI (T04.18)", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate directly to the client dashboard
    await page.goto("/client");
  });

  test("loads client dashboard with tenant greeting, plan, and container status", async ({ page }) => {
    // Welcome header with username
    await expect(page.getByRole("heading", { level: 1, name: /Welcome back,\s*client_alpha!|স্বাগতম,\s*client_alpha!/i })).toBeVisible();

    // Tenant status badge
    await expect(page.getByText(/Active|সক্রিয়/i).first()).toBeVisible();

    // Plan name badge
    await expect(page.getByText("Standard Business Plan")).toBeVisible();

    // Container stack runtime status pill
    await expect(page.getByText(/Stack:\s*running/i)).toBeVisible();
    await expect(page.getByText(/PHP\s*8\.3\.12/i)).toBeVisible();

    // Quick action buttons
    await expect(page.getByRole("button", { name: /Account Security|অ্যাকাউন্ট নিরাপত্তা/i })).toBeVisible();
    await expect(page.getByRole("button", { name: /View Website|ওয়েবসাইট দেখুন/i })).toBeVisible();
  });

  test("displays enforced hardware resource quotas and usage bars", async ({ page }) => {
    // Section header
    await expect(page.getByText(/Hardware & Quota Allocation|হার্ডওয়্যার ও কোটা বরাদ্দ/i)).toBeVisible();

    // Primary 4 quota cards
    await expect(page.getByText(/Storage Quota|স্টোরেজ কোটা/i)).toBeVisible();
    await expect(page.getByText("5%")).toBeVisible();
    await expect(page.getByText(/1\.[23]\s*GB/)).toBeVisible();

    await expect(page.getByText(/Monthly Traffic|মাসিক ট্রাফিক/i)).toBeVisible();

    await expect(page.getByText(/Memory \(RAM\)|মেমরি \(র‌্যাম\)/i)).toBeVisible();
    await expect(page.getByText("42%")).toBeVisible();
    await expect(page.getByText("850 MB")).toBeVisible();

    await expect(page.getByText(/CPU Cores|সিপিইউ কোর/i)).toBeVisible();
    await expect(page.getByText("28%", { exact: true })).toBeVisible();

    // Secondary metric counters
    await expect(page.getByText("Inodes (Files)")).toBeVisible();
    await expect(page.getByText("12,450 / 250,000")).toBeVisible();
    await expect(page.getByText("Databases").first()).toBeVisible();
    await expect(page.getByText("2 / 10")).toBeVisible();
    await expect(page.getByText("Domains").first()).toBeVisible();
    await expect(page.getByText("2 / 5")).toBeVisible();
  });

  test("displays quick tools and shortcuts with phase preview indicators", async ({ page }) => {
    // Section header
    await expect(page.getByText(/Quick Tools & Shortcuts|দ্রুত সরঞ্জাম ও শর্টকাট/i)).toBeVisible();

    // Shortcut cards
    await expect(page.getByRole("heading", { name: "File Manager" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Domain Names" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Databases" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "SSL / TLS" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Cron Jobs" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "PHP Configuration" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Backup & Restore" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Account Security" })).toBeVisible();

    // Click upcoming shortcut to verify notification toast
    const fileManagerCard = page.getByRole("heading", { name: "File Manager" });
    await fileManagerCard.click();
    await expect(page.getByText(/Scheduled for delivery in Phase 05/i)).toBeVisible();
  });

  test("displays primary web stack details and recent activity events", async ({ page }) => {
    // Primary Web Stack
    await expect(page.getByText(/Primary Web Stack|প্রাথমিক ওয়েব স্ট্যাক/i)).toBeVisible();
    await expect(page.getByText("alpha-hosting.com").first()).toBeVisible();
    await expect(page.getByText(/public_html/)).toBeVisible();
    await expect(page.getByText(/Let's Encrypt Authority X3/)).toBeVisible();
    await expect(page.getByText("ns1.dhpanel.net, ns2.dhpanel.net")).toBeVisible();

    // Recent activity stream
    await expect(page.getByText(/Recent Stack Events|সাম্প্রতিক স্ট্যাক ইভেন্ট/i)).toBeVisible();
    await expect(page.getByText("user.login")).toBeVisible();
    await expect(page.getByText("Client portal session active")).toBeVisible();
    await expect(page.getByText("ssl.renewed")).toBeVisible();
  });
});
