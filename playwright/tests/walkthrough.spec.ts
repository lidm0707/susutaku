import { test, expect } from "./fixtures";
import type { Page } from "@playwright/test";

// Step-by-step walkthrough capture: login, kanban + task modal, chat modal,
// machines popover, agent inspect, activity popover, pipelines, settings.
// Screenshots land in playwright/screenshots/walkthrough/.

const OUT = "screenshots/walkthrough";

async function shot(page: Page, name: string) {
  await page.waitForTimeout(400);
  await page.screenshot({ path: `${OUT}/${name}.png`, fullPage: false });
}

test.describe("walkthrough", () => {
  test("capture the main flows", async ({ page, login }) => {
    // 1. kanban after login
    await page.goto("/kanban");
    await shot(page, "01-kanban");

    // 2. add-task modal
    await page.click('button:has-text("add task")');
    await expect(page.locator('[role="dialog"]')).toBeVisible();
    await page.fill('[role="dialog"] input', "walkthrough task");
    await shot(page, "02-kanban-add-task-modal");
    await page.click('[role="dialog"] button[type="submit"]');
    await expect(page.locator('[role="dialog"]')).toBeHidden();

    // 3. chat modal via fab
    const fab = page.locator("button[aria-label='open chat']");
    await fab.click();
    const chat = page.locator('[role="dialog"]');
    await expect(chat).toBeVisible();
    await shot(page, "03-chat-modal");
    // 4. send a message (web search off -> plain reply or error bubble)
    await chat.locator('select[title="web search mode"]').selectOption("off").catch(() => {});
    const input = chat.getByPlaceholder(/type a message/);
    if (await input.count()) {
      await input.fill("hello from walkthrough");
      await shot(page, "04-chat-typed");
      await chat.locator('form button[type="submit"]').click();
      await page.waitForTimeout(2500);
      await shot(page, "05-chat-after-send");
    }
    await chat.locator('button[aria-label="close"]').click();
    await expect(page.locator('[role="dialog"]')).toBeHidden();

    // 5. machines modal (dock button opens a Modal, not a popover)
    await page.click('button[title="machines"]');
    const machines = page.locator('[role="dialog"]').filter({
      has: page.locator("h2", { hasText: "machines" }),
    });
    await expect(machines).toBeVisible();
    await shot(page, "06-machines");
    // 6. agent inspect (first agent row if any)
    const agentRow = machines.locator(".dock-machine-row button, .dock-agent-row button").first();
    if (await agentRow.count()) {
      await agentRow.click();
      await page.waitForTimeout(800);
      await shot(page, "07-agent-inspect");
      const dlg = page.locator('[role="dialog"]');
      if (await dlg.count()) await dlg.locator('button[aria-label="close"]').first().click();
    }
    await page.keyboard.press("Escape");
    await page.mouse.click(30, 80);

    // 7. activity modal
    await page.click('button[title="activity"]');
    const act = page.locator('[role="dialog"]').filter({
      has: page.locator("h2", { hasText: "activity" }),
    });
    await expect(act).toBeVisible();
    await page.waitForTimeout(800);
    await shot(page, "08-activity");

    // close popovers
    await page.mouse.click(30, 80);
    await shot(page, "09-after-activity");
  });

  test("pipelines and settings", async ({ page, login }) => {
    await page.goto("/pipelines");
    await shot(page, "10-pipelines");
    await page.goto("/cronjobs");
    await shot(page, "11-cronjobs");
    await page.goto("/agents");
    await shot(page, "12-agents");
    await page.goto("/settings");
    await shot(page, "13-settings");
  });
});
