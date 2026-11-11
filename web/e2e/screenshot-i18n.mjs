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
    timeout: 60000,
  });

  const context = await browser.newContext({
    viewport: { width: 1280, height: 900 },
  });

  const page = await context.newPage();

  // 1. Visit index route "/" (in default Bengali)
  console.log("Navigating to Dashboard '/' in Bengali...");
  await page.goto("http://127.0.0.1:8089/", { waitUntil: "domcontentloaded", timeout: 45000 });
  await wait(2000);
  const sc1 = path.join(ARTIFACT_DIR, "i18n-dashboard-bn.png");
  await page.screenshot({ path: sc1 });
  console.log("Saved screenshot 1 (Dashboard BN):", sc1);

  // 2. Toggle language to English via LanguageSwitcher button
  console.log("Toggling language to English on Dashboard...");
  const langBtn = page.locator("header button[aria-label='ভাষা'], header button[aria-label='Language']").first();
  if (await langBtn.count() > 0) {
    await langBtn.click();
    await wait(1500);
    const sc2 = path.join(ARTIFACT_DIR, "i18n-dashboard-en.png");
    await page.screenshot({ path: sc2 });
    console.log("Saved screenshot 2 (Dashboard EN):", sc2);
  }

  // 3. Click Demo nav link
  console.log("Navigating to UI Foundation Demo page...");
  const demoNavLink = page.locator("nav a[href='/demo']").first();
  if (await demoNavLink.count() > 0) {
    await demoNavLink.click();
    await wait(2000);
    const sc3 = path.join(ARTIFACT_DIR, "i18n-demo-en.png");
    await page.screenshot({ path: sc3, fullPage: true });
    console.log("Saved screenshot 3 (Demo EN):", sc3);
  }

  // 4. Toggle language back to Bengali on Demo page
  console.log("Toggling language back to Bengali on Demo page...");
  const langBtn2 = page.locator("header button[aria-label='Language'], header button[aria-label='ভাষা']").first();
  if (await langBtn2.count() > 0) {
    await langBtn2.click();
    await wait(1500);
    const sc4 = path.join(ARTIFACT_DIR, "i18n-demo-bn.png");
    await page.screenshot({ path: sc4, fullPage: true });
    console.log("Saved screenshot 4 (Demo BN):", sc4);
  }

  await browser.close();
  server.close();
  console.log("i18n visual verification completed successfully!");
}

main().catch(err => {
  console.error("Error:", err);
  process.exit(1);
});
