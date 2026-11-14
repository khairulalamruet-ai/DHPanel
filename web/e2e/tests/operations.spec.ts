import { test, expect } from "@playwright/test";

test.describe("Operation Ledger UI (T02.17)", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate directly to the operations page
    await page.goto("/operations");
  });

  test("loads operation ledger page and displays operations", async ({ page }) => {
    // Verify header
    await expect(page.getByText(/Operation Ledger|অপারেশন লেজার/i).first()).toBeVisible();

    // Verify operations table rows
    await expect(page.getByText("Ping").first()).toBeVisible();
    await expect(page.getByText("PullImage").first()).toBeVisible();

    // Verify status badges in the table
    await expect(page.locator("table").getByText(/Succeeded|সফল/i).first()).toBeVisible();
    await expect(page.locator("table").getByText(/Failed|ব্যর্থ/i).first()).toBeVisible();
  });

  test("filters operations by status", async ({ page }) => {
    const statusSelect = page.locator("#statusFilter");
    await expect(statusSelect).toBeVisible();

    // Select 'failed'
    await statusSelect.selectOption("failed");

    // Only failed operations should be visible
    await expect(page.getByText("PullImage").first()).toBeVisible();
    await expect(page.getByText("Ping")).not.toBeVisible();

    // Reset filters
    const resetBtn = page.getByRole("button", { name: /Reset|রিসেট/i });
    await resetBtn.click();
    await statusSelect.selectOption("all");

    await expect(page.getByText("Ping").first()).toBeVisible();
  });

  test("opens operation details modal and inspects payload and error", async ({ page }) => {
    // Find the view details buttons
    const viewButtons = page.getByRole("button", { name: /View Payload & Logs|পেলোড ও লগ দেখুন/i });
    await expect(viewButtons.first()).toBeVisible();
    await viewButtons.first().click();

    // Verify details modal
    await expect(page.getByText(/Operation Details|অপারেশন বিস্তারিত বিবরণ/i)).toBeVisible();
    await expect(page.getByText(/Operation Payload|অপারেশন পেলোড/i)).toBeVisible();

    // Close modal
    const closeBtn = page.getByRole("button", { name: /Close|বন্ধ করুন/i });
    await closeBtn.click();
    await expect(page.getByText(/Operation Details|অপারেশন বিস্তারিত বিবরণ/i)).not.toBeVisible();
  });

  test("retries a failed operation directly from table", async ({ page }) => {
    // Look for the retry button on a failed operation
    const retryButtons = page.getByRole("button", { name: /^Retry$|^পুনরায় চেষ্টা$/i });
    await expect(retryButtons.first()).toBeVisible();
    await retryButtons.first().click();

    // Verify success toast
    await expect(page.getByText(/Operation re-queued for execution successfully|অপারেশনটি সফলভাবে পুনরায় কিউতে যুক্ত করা হয়েছে/i)).toBeVisible();
  });

  test("executes apply-to-all rolling orchestrator across 10 nodes (T16.06)", async ({ page }) => {
    // Click Apply to All button
    const openBtn = page.getByTestId("btn-open-apply-all");
    await expect(openBtn).toBeVisible();
    await openBtn.click();

    // Verify dialog opens
    await expect(page.getByText(/Rolling Cluster Orchestrator|রোলিং ক্লাস্টার অর্কেস্ট্রেটর/i)).toBeVisible();

    // Select batch size of 2
    const batchSelect = page.getByTestId("select-batch-size");
    await expect(batchSelect).toBeVisible();
    await batchSelect.selectOption("2");

    // Click start rolling apply
    const startBtn = page.getByTestId("btn-start-rolling-apply");
    await expect(startBtn).toBeVisible();
    await startBtn.click();

    // Verify 10 nodes completed with succeeded status
    await expect(page.getByTestId("rolling-status-title")).toBeVisible();
    await expect(page.getByTestId("metric-total-nodes")).toHaveText("10");
    await expect(page.getByTestId("metric-succeeded-nodes")).toHaveText("10");
    await expect(page.getByTestId("rolling-result-badge")).toHaveText(/succeeded/i);

    // Verify 10 node rows rendered
    for (let i = 1; i <= 10; i++) {
      const nodeId = i === 1 ? "node-master-01" : `node-worker-${String(i).padStart(2, "0")}`;
      await expect(page.getByTestId(`node-progress-row-${nodeId}`)).toBeVisible();
    }
  });

  test("trips circuit breaker and skips subsequent nodes on failure (T16.06)", async ({ page }) => {
    const openBtn = page.getByTestId("btn-open-apply-all");
    await expect(openBtn).toBeVisible();
    await openBtn.click();

    // Simulate failure on node-worker-04
    const simFailSelect = page.getByTestId("select-simulate-fail");
    await expect(simFailSelect).toBeVisible();
    await simFailSelect.selectOption("node-04");

    // Click start rolling apply
    const startBtn = page.getByTestId("btn-start-rolling-apply");
    await startBtn.click();

    // Verify circuit breaker halted execution
    await expect(page.getByTestId("rolling-status-title")).toHaveText(/Execution Halted by Circuit Breaker|সার্কিট ব্রেকার দ্বারা কার্যক্রম স্থগিত/i);
    await expect(page.getByTestId("rolling-result-badge")).toHaveText(/halted/i);
    await expect(page.getByTestId("metric-skipped-nodes")).not.toHaveText("0");
  });
});
