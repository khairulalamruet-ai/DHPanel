import { test, expect } from "@playwright/test";

test.describe("Node Management UI (T02.16)", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate directly to the nodes page
    await page.goto("/nodes");
  });

  test("loads cluster nodes page and displays enrolled nodes", async ({ page }) => {
    // Verify header
    await expect(page.getByText(/Cluster Nodes|ক্লাস্টার নোডসমূহ/i).first()).toBeVisible();

    // Verify node items from mock data
    await expect(page.getByText("node-master-01").first()).toBeVisible();
    await expect(page.getByText("node-worker-02").first()).toBeVisible();

    // Verify status badges
    await expect(page.getByText(/Online|সচল/i).first()).toBeVisible();
  });

  test("opens Add Node wizard, inputs name, generates enrollment token, and displays install command", async ({ page }) => {
    // Click Add Node button
    const addBtn = page.getByRole("button", { name: /Add Node|নতুন নোড যুক্ত করুন/i });
    await expect(addBtn).toBeVisible();
    await addBtn.click();

    // Verify wizard dialog title
    await expect(page.getByText(/Enroll New Node|নতুন নোড অন্তর্ভুক্তকরণ/i)).toBeVisible();

    // Input node name
    const nameInput = page.locator("#node-name-input");
    await expect(nameInput).toBeVisible();
    await nameInput.fill("node-worker-04");

    // Click Generate Token
    const generateBtn = page.getByRole("button", { name: /Generate Enrollment Token|এনরোলমেন্ট টোকেন তৈরি করুন/i });
    await expect(generateBtn).toBeVisible();
    await generateBtn.click();

    // Verify token is displayed
    await expect(page.getByText("node-worker-04")).toBeVisible();

    // Verify command block appears
    const codeBlock = page.locator("pre code");
    await expect(codeBlock).toBeVisible();
    await expect(codeBlock).toContainText("curl -sSL https://get.dhpanel.com/agent.sh");

    // Click copy command button
    const copyBtn = page.getByRole("button", { name: /Copy Command|কমান্ড কপি করুন/i });
    await expect(copyBtn).toBeVisible();
    await copyBtn.click();

    // Verify copy feedback
    await expect(page.getByText(/Copied!|কমান্ড কপি হয়েছে!|কপি করা হয়েছে!/i)).toBeVisible();

    // Close wizard
    const doneBtn = page.getByRole("button", { name: /Done & Refresh|সম্পন্ন ও রিফ্রেশ/i });
    await doneBtn.click();
    await expect(page.getByText(/Enroll New Node|নতুন নোড অন্তর্ভুক্তকরণ/i)).not.toBeVisible();
  });

  test("opens node specifications dialog and displays system details", async ({ page }) => {
    const viewSpecsBtns = page.getByRole("button", { name: /View Specs|বিবরণ/i });
    await expect(viewSpecsBtns.first()).toBeVisible();
    await viewSpecsBtns.first().click();

    // Verify modal content
    await expect(page.getByText(/Docker Engine|ডকার ইঞ্জিন/i)).toBeVisible();
    await expect(page.getByText(/CPU Model|সিপিইউ মডেল/i)).toBeVisible();
    await expect(page.getByText(/Certificate Fingerprint|সার্টিফিকেট ফিঙ্গারপ্রিন্ট/i)).toBeVisible();

    // Close dialog
    const closeBtn = page.getByRole("button", { name: /Close|বন্ধ করুন/i });
    await closeBtn.click();
    await expect(page.getByText(/Docker Engine|ডকার ইঞ্জিন/i)).not.toBeVisible();
  });

  test("opens revoke confirmation dialog and requires typed node name", async ({ page }) => {
    const revokeBtns = page.getByRole("button", { name: /^Revoke$|^বাতিল করুন$/i });
    await expect(revokeBtns.first()).toBeVisible();
    await revokeBtns.first().click();

    // Verify revoke dialog
    await expect(page.getByText(/Revoke Node Certificate|নোড সার্টিফিকেট বাতিল করুন/i)).toBeVisible();

    // The confirm button should be disabled before typing
    const confirmBtn = page.getByRole("button", { name: /^Confirm$|^নোড বাতিল করুন$/i });
    await expect(confirmBtn).toBeDisabled();

    // Fill the node name
    const input = page.locator("input[placeholder='node-master-01']");
    await expect(input).toBeVisible();
    await input.fill("node-master-01");

    // The confirm button should now be enabled
    await expect(confirmBtn).toBeEnabled();
    await confirmBtn.click();

    // Verify modal closes
    await expect(page.getByText(/Revoke Node Certificate|নোড সার্টিফিকেট বাতিল করুন/i)).not.toBeVisible();
  });

  test("displays node fullness/saturation percentage in table and cluster capacity overview (T16.01 DoD)", async ({ page }) => {
    // Verify Cluster Capacity Overview card is rendered
    const clusterOverview = page.getByTestId("cluster-capacity-overview");
    await expect(clusterOverview).toBeVisible();
    await expect(clusterOverview.getByText(/Cluster Fullness|ক্লাস্টার পূর্ণতা/i)).toBeVisible();

    // Verify Fullness (%) column header (DoD requirement: Admin UI shows node saturation/fullness %)
    await expect(page.getByText(/Fullness \(%\)|পূর্ণতা \(%\)/i)).toBeVisible();

    // Verify node saturation percentages are visible
    await expect(page.getByText("68%").first()).toBeVisible();
    await expect(page.getByText("85%").first()).toBeVisible();

    // Verify saturation progress bar exists
    const saturationBars = page.getByTestId("node-saturation-bar");
    await expect(saturationBars.first()).toBeVisible();
  });

  test("toggles node drain and undrain mode (T16.01, T16.02)", async ({ page }) => {
    // Find Drain button for active node
    const drainBtn = page.getByRole("button", { name: /^Drain$|^ড্রেন$/i }).first();
    await expect(drainBtn).toBeVisible();
    await drainBtn.click();

    // Verify toast feedback for draining
    await expect(
      page.getByText(/Node set to draining mode|নোডটি ড্রেনিং মোডে নেওয়া হয়েছে/i).first()
    ).toBeVisible();

    // Find Undrain button for draining node
    const undrainBtn = page.getByRole("button", { name: /^Undrain$|^সচল করুন$/i }).first();
    await expect(undrainBtn).toBeVisible();
    await undrainBtn.click();

    // Verify toast feedback for undraining
    await expect(
      page.getByText(/Node restored to active service|নোডটি পুনরায় সফলভাবে সচল করা হয়েছে/i).first()
    ).toBeVisible();
  });

  test("opens oversell configuration dialog, modifies ratio, and saves (T16.01)", async ({ page }) => {
    // Click capacity / oversell button
    const editBtn = page.getByRole("button", { name: /Capacity \/ Oversell|ধারণক্ষমতা \/ ওভারসেল/i }).first();
    await expect(editBtn).toBeVisible();
    await editBtn.click();

    // Verify dialog opens
    await expect(page.getByText(/Oversell Settings|ওভারসেল সেটিংস/i)).toBeVisible();
    await expect(page.getByText(/CPU Oversell Multiplier|সিপিইউ ওভারসেল মাল্টিপ্লায়ার/i)).toBeVisible();

    // Modify CPU ratio
    const cpuInput = page.locator("#cpu-ratio-input");
    await expect(cpuInput).toBeVisible();
    await cpuInput.fill("2.5");

    // Click Save
    const saveBtn = page.getByRole("button", { name: /Save|সংরক্ষণ করুন/i });
    await expect(saveBtn).toBeVisible();
    await saveBtn.click();

    // Verify toast feedback
    await expect(
      page.getByText(/Node capacity & oversell settings updated|নোড ধারণক্ষমতা ও ওভারসেল সেটিংস সফলভাবে সংরক্ষিত হয়েছে/i).first()
    ).toBeVisible();
  });
});
