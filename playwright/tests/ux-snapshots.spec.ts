import { test, expect } from "./fixtures";
import { API, loginToken } from "./helpers";

// UX snapshot suite: captures reference screenshots of every main page.
// Run `npm run update:snapshots` after intentional UI changes; diff failures
// flag accidental visual regressions. Images also land in playwright/screenshots/
// for manual UX review.
const PAGES = [
  { path: "/", name: "login", auth: false },
  // /chat no longer exists as a page (redirects to /kanban); chat is the
  // global ChatModal, covered functionally in chat-modal.spec.ts.
  { path: "/kanban", name: "kanban", auth: true },
  { path: "/pipelines", name: "pipelines", auth: true },
  { path: "/prompts", name: "prompts", auth: true },
  { path: "/settings", name: "settings", auth: true },
];

// The e2e DB persists across runs (E2E_SKIP_DB_LIFECYCLE=1); cards left by
// other specs (walkthrough, kanban) would shift the board and break the
// baseline. Clear them so the snapshot only shows the page chrome.
async function clearCards() {
  const token = await loginToken();
  const headers = { Authorization: `Bearer ${token}` };
  const workspaces = (await (await fetch(`${API}/api/workspaces`, { headers })).json()) as {
    id: number;
  }[];
  for (const ws of workspaces) {
    const projects = (await (
      await fetch(`${API}/api/workspaces/${ws.id}/projects`, { headers })
    ).json()) as { id: number }[];
    for (const p of projects) {
      const cards = (await (
        await fetch(`${API}/api/kanban/cards?project_id=${p.id}`, { headers })
      ).json()) as { id: number }[];
      for (const c of cards) {
        await fetch(`${API}/api/kanban/cards/${c.id}`, {
          method: "DELETE",
          headers,
        });
      }
    }
  }
}

for (const { path, name, auth } of PAGES) {
  test(`snapshot: ${name}`, async ({ page, login }) => {
    if (!auth) {
      // Fixture logged us in; use a fresh context for the logged-out login page.
      await page.evaluate(() => localStorage.removeItem("susutaku_token"));
    }
    if (name === "kanban") await clearCards();
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
