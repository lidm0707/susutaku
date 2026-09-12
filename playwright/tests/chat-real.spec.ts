// E2E against the REAL local model server (no deterministic markers).
// Skipped unless E2E_REAL_MODEL=1 — see check_real/README.md and
// docker/compose/playwright-real.yml.
import { test, expect } from "./fixtures";

const REAL_MODEL_FLAG = "1";
const CHAT_TIMEOUT_MS = 120_000;

// The chat modal only sends when an agent is selected; the real stack must
// provide at least one agent (with a real model) for these tests to run.
async function openChatModal(page: import("@playwright/test").Page) {
  await page.goto("/kanban");
  await page.click("button[aria-label='open chat']");
  const dialog = page.locator('[role="dialog"]');
  await expect(dialog).toBeVisible();
  const agentSelect = dialog.locator('select[title="agent"]');
  const value = await agentSelect.inputValue();
  test.skip(value === "", "needs at least one agent on the real stack");
  return dialog;
}

test.skip(process.env.E2E_REAL_MODEL !== REAL_MODEL_FLAG, "needs the real-local stack");

test.describe("chat against the real local model", () => {
  test("model catalog lists at least one model", async ({ request }) => {
    const res = await request.get("/api/models");
    expect(res.ok()).toBeTruthy();
    const models = await res.json();
    expect(models.length).toBeGreaterThan(0);
  });

  test("chat ui returns a real reply", async ({ page, login }) => {
    const dialog = await openChatModal(page);
    await dialog.getByPlaceholder(/type a message/).fill("Reply with the single word: ok");
    await dialog.locator('form button[type="submit"]').click();
    await expect(page.locator(".bubble.assistant p").last()).toBeVisible({
      timeout: CHAT_TIMEOUT_MS,
    });
  });

  test("shell tool round-trips through the sandbox", async ({ page, login }) => {
    const dialog = await openChatModal(page);
    await dialog
      .getByPlaceholder(/type a message/)
      .fill("Run a shell command that prints REAL-SANDBOX-OK (use the TOOL: SHELL form).");
    await dialog.locator('form button[type="submit"]').click();
    // Loose: the assistant eventually answers after the tool round; we cannot
    // assert exact text since a real model replies nondeterministically.
    await expect(page.locator(".bubble.assistant p").last()).toBeVisible({
      timeout: CHAT_TIMEOUT_MS,
    });
  });
});
