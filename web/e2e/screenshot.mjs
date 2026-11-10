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
      console.log(`Server listening on http://127.0.0.1:${port}`);
      resolve(server);
    });
  });
}

async function wait(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function main() {
  const server = await createStaticServer(DIST_DIR, 8089);

  console.log("Launching Chrome...");
  const browser = await chromium.launch({
    channel: "chrome",
    args: ["--headless=new", "--no-sandbox", "--disable-gpu"],
  });

  const context = await browser.newContext({
    viewport: { width: 1280, height: 800 },
  });

  const page = await context.newPage();

  console.log("Navigating to http://127.0.0.1:8089/ ...");
  await page.goto("http://127.0.0.1:8089/", { waitUntil: "domcontentloaded" });
  await wait(3000);

  // 1. Bengali Light Mode (default)
  const sc1 = path.join(ARTIFACT_DIR, "admin-light-bn.png");
  await page.screenshot({ path: sc1 });
  console.log("Saved screenshot 1:", sc1);

  // 2. Toggle to English via desktop header
  const langBtn = page.locator("header button:has-text('বাং')");
  if (await langBtn.count() > 0) {
    await langBtn.click();
    await wait(1000);
    const sc2 = path.join(ARTIFACT_DIR, "admin-light-en.png");
    await page.screenshot({ path: sc2 });
    console.log("Saved screenshot 2:", sc2);

    // 3. Toggle to Dark Mode
    const themeBtn = page.locator("header button[aria-label='Toggle theme']");
    await themeBtn.click();
    await wait(1000);
    const sc3 = path.join(ARTIFACT_DIR, "admin-dark-en.png");
    await page.screenshot({ path: sc3 });
    console.log("Saved screenshot 3:", sc3);

    // 4. Toggle back to Bengali in Dark Mode
    const langBtnEn = page.locator("header button:has-text('EN')");
    await langBtnEn.click();
    await wait(1000);
    const sc4 = path.join(ARTIFACT_DIR, "admin-dark-bn.png");
    await page.screenshot({ path: sc4 });
    console.log("Saved screenshot 4:", sc4);
  }

  await browser.close();
  server.close();
  console.log("All 4 screenshots taken successfully!");
}

main().catch(err => {
  console.error("Error:", err);
  process.exit(1);
});
