import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { chromium } from "playwright";

const server = spawn("python3", ["-u", "-m", "http.server", "0", "--bind", "127.0.0.1", "--directory", "web"]);
let browser;
try {
  const [banner] = await once(server.stdout, "data");
  const port = /port (\d+)/.exec(String(banner))[1];
  browser = await chromium.launch(process.env.CHROME_PATH ? { executablePath: process.env.CHROME_PATH } : {});
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(`http://127.0.0.1:${port}/`);
  await page.locator("#app button").filter({ hasText: "+" }).click();
  await page.getByText("Count: 1", { exact: true }).waitFor();
  await page.goto(`http://127.0.0.1:${port}/playground/dist/`);
  await page.locator("textarea").waitFor();
  assert.deepEqual(errors, []);
} finally {
  await browser?.close();
  server.kill();
}
