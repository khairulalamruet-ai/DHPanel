import { test, expect } from "@playwright/test";

test.describe("Client File Manager UI (T08.09)", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate directly to client file manager
    await page.goto("/files");
  });

  test("loads file manager page with toolbar, shortcuts, and default file list", async ({ page }) => {
    // Page Title
    await expect(page.getByTestId("file-manager-title")).toBeVisible();

    // Toolbar buttons
    await expect(page.getByTestId("new-file-button")).toBeVisible();
    await expect(page.getByTestId("new-folder-button")).toBeVisible();
    await expect(page.getByTestId("upload-button")).toBeVisible();
    await expect(page.getByTestId("trash-view-button")).toBeVisible();

    // Directory shortcuts
    await expect(page.getByTestId("shortcut-home")).toBeVisible();
    await expect(page.getByTestId("shortcut-public-html")).toBeVisible();
    await expect(page.getByTestId("shortcut-logs")).toBeVisible();
    await expect(page.getByTestId("shortcut-ssl")).toBeVisible();

    // Seed files in /home/demo/public_html
    await expect(page.getByTestId("file-name-index.php")).toBeVisible();
    await expect(page.getByTestId("file-name-style.css")).toBeVisible();
    await expect(page.getByTestId("file-name-.htaccess")).toBeVisible();
    await expect(page.getByTestId("file-name-backup.zip")).toBeVisible();
  });

  test("DoD cycle: upload/create -> edit -> delete to trash -> restore", async ({ page }) => {
    // Accept dialog for delete confirmation
    page.on("dialog", (dialog) => dialog.accept());

    // 1. Create a new file (or upload)
    await page.getByTestId("new-file-button").click();
    await expect(page.getByTestId("create-dialog-title")).toBeVisible();

    const fileName = "demo-document.txt";
    await page.getByTestId("create-item-name-input").fill(fileName);
    await page.getByTestId("create-item-submit-button").click();

    // Verify file appeared in the list
    await expect(page.getByTestId(`file-name-${fileName}`)).toBeVisible();

    // 2. Edit the file
    await page.getByTestId(`edit-button-${fileName}`).click();
    await expect(page.getByTestId("file-editor-modal")).toBeVisible();
    await expect(page.getByTestId("file-editor-filename")).toHaveText(fileName);

    const editorTextarea = page.getByTestId("textarea-file-editor");
    await editorTextarea.fill("Initial content from E2E test.\nLine 2 updated.");
    await page.getByTestId("btn-save-file-editor").click();

    // Close editor modal
    await page.getByTestId("btn-close-file-editor").click();
    await expect(page.getByTestId("file-editor-modal")).not.toBeVisible();

    // 3. Delete the file to trash
    await page.getByTestId(`delete-button-${fileName}`).click();

    // Verify file is removed from current directory
    await expect(page.getByTestId(`file-name-${fileName}`)).not.toBeVisible();

    // 4. Open Trash modal and verify file is in trash
    await page.getByTestId("trash-view-button").click();
    await expect(page.getByTestId("trash-modal-title")).toBeVisible();
    await expect(page.getByText(fileName)).toBeVisible();

    // 5. Restore the file from trash
    const restoreBtn = page.locator(`button:has-text("Restore"), button:has-text("পুনরুদ্ধার")`).first();
    await restoreBtn.click();

    // Close trash modal
    await page.getByTestId("close-trash-modal").click();
    await expect(page.getByTestId("trash-modal-title")).not.toBeVisible();

    // 6. Verify file is restored back in public_html
    await expect(page.getByTestId(`file-name-${fileName}`)).toBeVisible();
  });

  test("updates file permissions via chmod dialog", async ({ page }) => {
    await expect(page.getByTestId("chmod-trigger-index.php")).toBeVisible();
    await page.getByTestId("chmod-trigger-index.php").click();

    await expect(page.getByTestId("permissions-dialog")).toBeVisible();
    await expect(page.getByTestId("chmod-octal-value")).toHaveText("0644");

    // Toggle owner execute bit
    await page.getByTestId("chmod-owner-exec").click();
    await expect(page.getByTestId("chmod-octal-value")).toHaveText("0744");

    // Save
    await page.getByTestId("btn-save-chmod").click();
    await expect(page.getByTestId("permissions-dialog")).not.toBeVisible();
  });
});
