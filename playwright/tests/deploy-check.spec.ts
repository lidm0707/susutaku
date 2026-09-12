// Ad-hoc visual check against a live deploy stack (e.g. http://localhost:3334):
// login → machines modal → chat "hi" → machines shows the chat agent.
// Gated behind E2E_DEPLOY_CHECK=1 so normal suites never run it.
import { test, expect } from "./fixtures";
import { loginToken } from "./helpers";
import type { Page } from "@playwright/test";

const OUT = "screenshots/deploy-check";
const CHAT_TEXT = "hi from playwright";
const CHAT_WAIT_MS = 15_000;

function dialogWithTitle(page: Page, title: string) {
  return page.locator('[role="dialog"]').filter({
    has: page.locator("h2", { hasText: title }),
  });
}

async function shot(page: Page, name: string) {
  await page.waitForTimeout(400);
  await page.screenshot({ path: `${OUT}/${name}.png` });
}

test.skip(process.env.E2E_DEPLOY_CHECK !== "1", "deploy-stack visual check only");

test("machines and chat show the chat agent", async ({ page }) => {
  const token = await loginToken(
    process.env.E2E_ADMIN_USER || "e2e-admin",
    process.env.E2E_ADMIN_PASSWORD || "e2e-admin-e2e-admin",
  );
  await page.addInitScript((t) => localStorage.setItem("susutaku_token", t), token);
  await page.goto("/agents");
  await shot(page, "01-agents-page");

  // Machines modal: both tabs.
  await page.click('button[title="machines"]');
  const machines = dialogWithTitle(page, "machines");
  await expect(machines).toBeVisible();
  await shot(page, "02-machines-users-hosts");
  await machines.locator('[role="tab"]', { hasText: "agent sandboxes" }).click();
  await page.waitForTimeout(800);
  await shot(page, "03-machines-sandboxes-before");
  await machines.locator('button[aria-label="close"]').click();

  // Chat: pick exactly one agent, send, wait for the reply.
  await page.click("button[aria-label='open chat']");
  const chat = page.locator('[role="dialog"]').filter({
    has: page.locator("h2", { hasText: "susutaku" }),
  });
  await expect(chat).toBeVisible();
  await chat.locator('button[title="choose agent(s)"]').click();
  const items = chat.locator('[role="menuitemcheckbox"]');
  const count = await items.count();
  test.skip(count === 0, "no agents configured on this stack");
  for (let i = 0; i < count; i++) {
    const item = items.nth(i);
    if ((await item.getAttribute("aria-checked")) !== "true") await item.click();
  }
  await chat.locator('button[title="choose agent(s)"]').click();
  await chat.locator('select[title="web search mode"]').selectOption("off").catch(() => {});
  await chat.getByPlaceholder(/type a message/).fill(CHAT_TEXT);
  await shot(page, "04-chat-typed");
  await chat.locator('form button[type="submit"]').click();
  await page.waitForTimeout(CHAT_WAIT_MS);
  await shot(page, "05-chat-reply");
  await chat.locator('button[aria-label="close"]').click();

  // Machines again: the agent list should now show the chat agent.
  await page.click('button[title="machines"]');
  await machines.locator('[role="tab"]', { hasText: "agent sandboxes" }).click();
  await page.waitForTimeout(800);
  await shot(page, "06-machines-sandboxes-after");
  await machines.locator('button[aria-label="close"]').click();
});
