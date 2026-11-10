import { chromium } from "@playwright/test";
import http from "http";
import fs from "fs";
import path from "path";

const ARTIFACT_DIR = "/Users/khairulalam/.gemini/antigravity/brain/c35f3373-58dd-4e9d-9fe6-3d3e79e84e10";
const DIST_DIR = "/Volumes/SSD/DHPanel-ag/web/apps/admin/dist";

const MIME_TYPES = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".css": "text/css",
  ".json": "application/json",
  ".svg": "image/svg+xml",
  ".png": "image/png",
};

function createStaticServer(distDir, port = 8089) {
  const server = http.createServer((req, res) => {
    let filePath = path.join(distDir, req.url === "/" ? "index.html" : req.url.split("?")[0]);
    if (!fs.existsSync(filePath)) {
      filePath = path.join(distDir, "index.html");
    }
    const ext = path.extname(filePath).toLowerCase();
    const contentType = MIME_TYPES[ext] || "application/octet-stream";
    try {
      const content = fs.readFileSync(filePath);
      res.writeHead(200, { "Content-Type": contentType });
      res.end(content);
    } catch {
      res.writeHead(404);
      res.end("Not Found");
    }
  });

  return new Promise((resolve) => {
    server.listen(port, "127.0.0.1", () => {
      console.log(`Static server running on http://127.0.0.1:${port}`);
      resolve(server);
    });
  });
}

async function wait(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function main() {
  const server = await createStaticServer(DIST_DIR, 8089);

  console.log("Launching headless Chrome...");
  const browser = await chromium.launch({
    channel: "chrome",
    args: ["--headless=new", "--no-sandbox", "--disable-gpu"],
  });

  const context = await browser.newContext({
    viewport: { width: 1280, height: 950 },
  });

  const page = await context.newPage();

  console.log("Navigating to admin portal...");
  await page.goto("http://127.0.0.1:8089/", { waitUntil: "domcontentloaded" });
  await wait(2000);

  // Switch to UI Components Demo tab
  const demoTab = page.locator("button:has-text('UI Components Demo (T00.12)')");
  if (await demoTab.count() > 0) {
    await demoTab.click();
    await wait(1000);
  }

  // Trigger a success toast so toast is visible in screenshot
  const successToastBtn = page.locator("button:has-text('Success Toast')");
  if (await successToastBtn.count() > 0) {
    await successToastBtn.click();
    await wait(600);
  }

  // 1. Light Mode Demo Page
  const sc1 = path.join(ARTIFACT_DIR, "ui-demo-light.png");
  await page.screenshot({ path: sc1 });
  console.log("Saved screenshot:", sc1);

  // 2. Open ConfirmDialog to show high-stakes typed confirmation modal
  const deleteBtn = page.locator("button:has-text('Delete Tenant (Typed Confirm)')");
  if (await deleteBtn.count() > 0) {
    await deleteBtn.click();
    await wait(600);
    const sc2 = path.join(ARTIFACT_DIR, "ui-demo-confirm-dialog.png");
    await page.screenshot({ path: sc2 });
    console.log("Saved screenshot:", sc2);

    // Cancel modal
    const cancelBtn = page.locator("button:has-text('Cancel')");
    if (await cancelBtn.count() > 0) {
      await cancelBtn.click();
      await wait(600);
    }
  }

  // 3. Toggle to Dark Theme
  const themeToggle = page.locator("header button[aria-label='Toggle theme']");
  if (await themeToggle.count() > 0) {
    await themeToggle.click();
    await wait(1000);
    const sc3 = path.join(ARTIFACT_DIR, "ui-demo-dark.png");
    await page.screenshot({ path: sc3 });
    console.log("Saved screenshot:", sc3);
  }

  await browser.close();
  server.close();
  console.log("UI Demo screenshots captured successfully!");
}

main().catch(err => {
  console.error("Error:", err);
  process.exit(1);
});
