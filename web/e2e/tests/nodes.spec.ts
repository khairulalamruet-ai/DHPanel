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

  test("configures placement scheduler and toggles drain mode (DoD T16.02)", async ({ page }) => {
    // Open placement scheduler dialog
    const schedulerBtn = page.getByTestId("configure-scheduler-btn");
    await expect(schedulerBtn).toBeVisible();
    await schedulerBtn.click();

    // Verify dialog content and simulation card
    await expect(page.getByText(/Placement Scheduler & Policies|প্লেসমেন্ট শিডিউলার ও নীতি/i)).toBeVisible();
    await expect(page.getByTestId("placement-simulation-card")).toBeVisible();

    // Verify drainage invariant is mentioned and draining nodes are disqualified
    await expect(page.getByText(/Disqualified Draining \/ Offline Nodes|অযোগ্য ড্রেনিং \/ অফলাইন নোডসমূহ/i)).toBeVisible();

    // Save policy
    const saveBtn = page.getByTestId("save-scheduler-policy-btn");
    await expect(saveBtn).toBeVisible();
    await saveBtn.click();

    // Modal closes
    await expect(page.getByText(/Placement Scheduler & Policies|প্লেসমেন্ট শিডিউলার ও নীতি/i)).not.toBeVisible();

    // Toggle drain on online node
    const drainBtn = page.getByRole("button", { name: /^Drain$|^ড্রেন$/i }).first();
    await expect(drainBtn).toBeVisible();
    await drainBtn.click();

    await expect(
      page.getByText(/Node set to draining mode|নোডটি ড্রেনিং মোডে নেওয়া হয়েছে/i).first()
    ).toBeVisible();

    // Toggle undrain on draining node
    const undrainBtn = page.getByRole("button", { name: /^Undrain$|^সচল করুন$/i }).first();
    await expect(undrainBtn).toBeVisible();
    await undrainBtn.click();

    await expect(
      page.getByText(/Node restored to active cluster service|নোডটি পুনরায় ক্লাস্টার সার্ভিসে সচল করা হয়েছে/i).first()
    ).toBeVisible();
  });
});
