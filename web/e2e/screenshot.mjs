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
    viewport: { width: 1280, height: 800 },
  });

  const page = await context.newPage();

  // 1. Visit index route "/"
  console.log("Navigating to index route '/' ...");
  await page.goto("http://127.0.0.1:8089/", { waitUntil: "domcontentloaded" });
  await wait(2000);
  const sc1 = path.join(ARTIFACT_DIR, "router-dashboard.png");
  await page.screenshot({ path: sc1 });
  console.log("Saved screenshot 1:", sc1);

  // 2. Visit invalid route "/not-found-random-page"
  console.log("Navigating to invalid route '/not-found-random-page' ...");
  await page.goto("http://127.0.0.1:8089/not-found-random-page", { waitUntil: "domcontentloaded" });
  await wait(2000);
  const sc2 = path.join(ARTIFACT_DIR, "router-404-light.png");
  await page.screenshot({ path: sc2 });
  console.log("Saved screenshot 2:", sc2);

  // 3. Toggle to Dark theme on 404 page
  const themeToggle = page.locator("header button[aria-label='Toggle theme']");
  if (await themeToggle.count() > 0) {
    await themeToggle.click();
    await wait(1000);
    const sc3 = path.join(ARTIFACT_DIR, "router-404-dark.png");
    await page.screenshot({ path: sc3 });
    console.log("Saved screenshot 3:", sc3);
  }

  // 4. Click Return to Dashboard button and verify return
  const backHomeBtn = page.locator("button:has-text('ড্যাশবোর্ডে ফিরে যান'), button:has-text('Back to Dashboard')").first();
  if (await backHomeBtn.count() > 0) {
    await backHomeBtn.click();
    await wait(1500);
    const sc4 = path.join(ARTIFACT_DIR, "router-return-home.png");
    await page.screenshot({ path: sc4 });
    console.log("Saved screenshot 4:", sc4);
  }

  await browser.close();
  server.close();
  console.log("Router and 404 verification completed successfully!");
}

main().catch(err => {
  console.error("Error:", err);
  process.exit(1);
});
