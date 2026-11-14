import { test, expect } from "@playwright/test";

test.describe("API Tokens & Public Developer API (T18.11)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/admin/api-tokens");
  });

  test("generates new API token with scopes, reveals secret key, and shows in list", async ({
    page,
  }) => {
    // 1. Table is visible
    await expect(page.getByTestId("api-tokens-page")).toBeVisible();
    await expect(page.getByTestId("api-tokens-table-card")).toBeVisible();

    // 2. Open generate modal
    const generateBtn = page.getByTestId("btn-open-create-token");
    await generateBtn.click();

    // 3. Fill name
    const nameInput = page.getByTestId("input-token-name");
    await nameInput.fill("Billing Production Gateway");

    // 4. Select scope
    const billingScopeItem = page.getByTestId("scope-item-billing:whmcs");
    await billingScopeItem.click();

    // 5. Submit
    const submitBtn = page.getByTestId("btn-submit-create-token");
    await submitBtn.click();

    // 6. Secret key is revealed
    await expect(page.getByTestId("token-secret-revealed-view")).toBeVisible();
    const secretInput = page.getByTestId("created-secret-input");
    await expect(secretInput).toHaveValue(/dhp_live_/);

    // 7. Copy button
    const copyBtn = page.getByTestId("btn-copy-secret-key");
    await copyBtn.click();

    // 8. Close modal
    const doneBtn = page.getByTestId("btn-token-done");
    await doneBtn.click();

    // 9. Appears in table
    await expect(page.getByTestId("tokens-table").getByText("Billing Production Gateway")).toBeVisible();
  });

  test("runs scope authorization tester: GET /users succeeds with 200, out-of-scope call returns 403 (DoD)", async ({
    page,
  }) => {
    await expect(page.getByTestId("api-docs-explorer-card")).toBeVisible();

    // 1. Test GET /users with token having users:read scope
    const usersEndpoint = page.getByTestId("endpoint-btn-users");
    await usersEndpoint.click();

    const runBtn = page.getByTestId("btn-run-api-test");
    await runBtn.click();

    // Verify 200 OK
    const responseConsole = page.getByTestId("api-test-response-console");
    await expect(responseConsole).toBeVisible();
    await expect(responseConsole).toContainText(/HTTP 200 OK/i);
    await expect(responseConsole).toContainText(/users_count/i);

    // 2. Test out-of-scope endpoint: POST /admin/nodes
    const nodesEndpoint = page.getByTestId("endpoint-btn-nodes");
    await nodesEndpoint.click();
    await runBtn.click();

    // Verify 403 Forbidden (DoD)
    await expect(responseConsole).toContainText(/HTTP 403 Forbidden/i);
    await expect(responseConsole).toContainText(/Token lacks required scope/i);
  });
});
