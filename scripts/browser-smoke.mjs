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
  await page.goto(`http://127.0.0.1:${port}/todos.html`);
  const draft = page.getByRole("textbox", { name: "New todo" });
  await page.getByRole("button", { name: "add", exact: true }).click();
  await page.getByText("Enter a title", { exact: true }).waitFor();
  assert.equal(await draft.evaluate((el) => el === document.activeElement), true);
  assert.equal(await draft.evaluate((el) => document.getElementById(el.getAttribute("aria-describedby")).textContent), "Enter a title");
  await draft.fill("Check composition");
  await draft.dispatchEvent("keydown", { key: "Enter", isComposing: true });
  assert.equal(await draft.inputValue(), "Check composition");
  await draft.press("Enter");
  await page.getByText("[ ] Check composition", { exact: true }).waitFor();
  assert.equal(await draft.inputValue(), "");
  await page.goto(`http://127.0.0.1:${port}/playground/dist/`);
  await page.locator("textarea").waitFor();
  assert.deepEqual(errors, []);
} finally {
  await browser?.close();
  server.kill();
}
