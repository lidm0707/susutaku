import { test, expect } from "./fixtures";

// UX snapshot suite: captures reference screenshots of every main page.
// Run `npm run update:snapshots` after intentional UI changes; diff failures
// flag accidental visual regressions. Images also land in playwright/screenshots/
// for manual UX review.
const PAGES = [
  { path: "/", name: "login", auth: false },
  { path: "/chat", name: "chat", auth: true },
  { path: "/kanban", name: "kanban", auth: true },
  { path: "/pipelines", name: "pipelines", auth: true },
  { path: "/prompts", name: "prompts", auth: true },
  { path: "/settings", name: "settings", auth: true },
];

for (const { path, name, auth } of PAGES) {
  test(`snapshot: ${name}`, async ({ page, login }) => {
    if (!auth) {
      // Fixture logged us in; use a fresh context for the logged-out login page.
      await page.evaluate(() => localStorage.removeItem("susutaku_token"));
    }
    await page.goto(path);
    await page.waitForLoadState("networkidle");
    await page.waitForTimeout(300); // settle animations
    await page.screenshot({
      path: `screenshots/pages/${name}.png`,
      fullPage: true,
    });
    await expect(page).toHaveScreenshot(`${name}.png`, { fullPage: true });
  });
}
