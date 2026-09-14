import { request as pwRequest, type APIRequestContext, type Page } from "@playwright/test";
import { test, expect } from "./fixtures";
import { API, e2eCredentials } from "./helpers";

// Covers the chat card chip (new card -> chip in chat -> click opens the card
// page) and the html fence preview/raw toggle. Runs against the dev stack on
// localhost:3334; the chat reply is mocked via route interception, so no model
// is needed.
const WS_NAME = `e2e-chatchip-${Date.now()}`;
const AGENT_NAME = `e2e-chatchip-agent-${Date.now()}`;
const CARD_TITLE = "chip card from e2e";
const HTML_REPLY = [
  "here is your page:",
  "```html",
  "<h1>e2e heading</h1><p>e2e paragraph</p>",
  "```",
].join("\n");

test.describe("chat card chip + html fence", () => {
  let token = "";
  let workspace_id = 0;
  let project_id = 0;
  let agent_id = 0;

  test.beforeAll(async ({ browser }) => {
    const page = await browser.newPage();
    const { username, password } = e2eCredentials();
    await page.goto("/");
    await page.fill('input[placeholder="username"]', username);
    await page.fill('input[placeholder="password"]', password);
    await page.click('button[type="submit"]');
    await page.waitForURL("**/kanban");
    token = await page.evaluate(() => localStorage.getItem("susutaku_token") || "");
    await page.close();

    const ctx: APIRequestContext = await pwRequest.newContext({ baseURL: API });
    const headers = { Authorization: `Bearer ${token}` };
    const ws = await (await ctx.post("/api/workspaces", { headers, data: { name: WS_NAME } })).json();
    workspace_id = ws.id;
    const proj = await (
      await ctx.post(`/api/workspaces/${workspace_id}/projects`, { headers, data: { name: "chatchip" } })
    ).json();
    project_id = proj.id;
    const agent = await (
      await ctx.post("/api/agents", {
        headers,
        data: { name: AGENT_NAME, model: "", persona: "", prompt: "", output: "" },
      })
    ).json();
    agent_id = agent.id;
    await ctx.dispose();
  });

  test.afterAll(async () => {
    const ctx = await pwRequest.newContext({ baseURL: API });
    const headers = { Authorization: `Bearer ${token}` };
    if (agent_id) await ctx.delete(`/api/agents/${agent_id}`, { headers });
    // Workspace delete cascades to the project and the created card.
    if (workspace_id) await ctx.delete(`/api/workspaces/${workspace_id}`, { headers });
    await ctx.dispose();
  });

  async function open_board(page: Page) {
    await page.goto("/kanban");
    // Workspace/project selection lives in the profile modal (radio groups).
    await page.click('button[title="profile"]');
    await page
      .locator('[role="radiogroup"][aria-label="workspaces"] [role="radio"]', {
        hasText: WS_NAME,
      })
      .click();
    await page
      .locator('[role="radiogroup"][aria-label="projects"] [role="radio"]', {
        hasText: "chatchip",
      })
      .click();
    await page.click('[role="dialog"][aria-modal] button[aria-label="close"]');
    await expect(page.locator('button[title="add task"]')).toBeEnabled();
  }

  async function open_chat(page: Page) {
    await page.click("button[aria-label='open chat']");
    const dialog = page.locator('[role="dialog"]');
    await expect(dialog).toBeVisible();
    return dialog;
  }

  async function select_only_agent(page: Page, dialog: ReturnType<Page["locator"]>) {
    await dialog.locator('button[title="choose agent(s)"]').click();
    const items = dialog.locator('[role="menuitemcheckbox"]');
    const count = await items.count();
    for (let i = 0; i < count; i++) {
      const item = items.nth(i);
      const checked = (await item.getAttribute("aria-checked")) === "true";
      const isTarget = (await item.textContent())?.includes(AGENT_NAME) ?? false;
      if (checked !== isTarget) await item.click();
    }
    await dialog.locator('button[title="choose agent(s)"]').click();
  }

  test("a new card shows a chip in chat that opens the card page", async ({ page, login }) => {
    await open_board(page);
    const dialog = await open_chat(page);

    await page.click('button[title="add task"]');
    await page.fill('input[placeholder="task title…"]', CARD_TITLE);
    await page.locator("form.modal-form button[type='submit']").click();
    await expect(page.locator(".toast", { hasText: "task created" })).toBeVisible({ timeout: 10_000 });

    // The chip appears in chat with the card number.
    const chip = dialog.locator(".card-chip", { hasText: CARD_TITLE });
    await expect(chip).toBeVisible({ timeout: 10_000 });
    const chipText = await chip.textContent();
    expect(chipText).toMatch(/card #\d+/);

    // Clicking the chip opens a new page focused on that card.
    const [popup] = await Promise.all([
      page.waitForEvent("popup"),
      chip.click(),
    ]);
    expect(popup.url()).toMatch(/kanban\?project=\d+&card=\d+$/);
    await expect(popup.locator(".kanban-detail")).toBeVisible({ timeout: 10_000 });
    await expect(
      popup.locator('.kanban-detail input[placeholder="title"]')
    ).toHaveValue(CARD_TITLE);
  });

  test("html fence renders a live preview and toggles to raw html", async ({ page, login }) => {
    await open_board(page);
    const dialog = await open_chat(page);
    await select_only_agent(page, dialog);
    await dialog.locator('select[title="web search mode"]').selectOption("off");

    await page.route("**/api/chat/zai", async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ reply: HTML_REPLY, model: "mock", searched: false }),
      });
    });

    await dialog.getByPlaceholder(/type a message/).fill("show me some html");
    await dialog.locator('form button[type="submit"]').click();

    // Preview first: a sandboxed iframe renders the html for real.
    const frame = page.locator("iframe.msg-html-frame");
    await expect(frame).toBeVisible({ timeout: 10_000 });
    await expect(
      page.frameLocator("iframe.msg-html-frame").locator("h1", { hasText: "e2e heading" })
    ).toBeVisible();

    // Toggle to raw html shows the source, then back to the preview.
    const toggle = page.locator(".msg-html .msg-code-head button");
    await expect(toggle).toContainText("raw html");
    await toggle.click();
    await expect(page.locator(".msg-html pre")).toContainText("<h1>");
    await expect(toggle).toContainText("preview");
    await toggle.click();
    await expect(frame).toBeVisible();
  });
});
