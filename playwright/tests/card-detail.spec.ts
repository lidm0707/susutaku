import { test, expect } from "./fixtures";
import { request as pwRequest, type APIRequestContext, type Page } from "@playwright/test";
import { API } from "./helpers";

test.describe("task card detail", () => {
  let token: string;
  let project_id: number;
  let workspace_id: number;
  let card_id: number;
  let agent_id: number | null = null;
  const wsName = `e2e-carddetail-${Date.now()}`;

  test.beforeAll(async ({ browser }) => {
    const page = await browser.newPage();
    // Login once (fixture login is per-test, so replicate it here).
    const { e2eCredentials } = await import("./helpers");
    const { username, password } = e2eCredentials();
    await page.goto("/");
    await page.fill('input[placeholder="username"]', username);
    await page.fill('input[placeholder="password"]', password);
    await page.click('button[type="submit"]');
    await page.waitForURL("**/task");
    token = await page.evaluate(() => localStorage.getItem("susutaku_token") || "");
    await page.close();

    const headers = { Authorization: `Bearer ${token}` };
    const ctx: APIRequestContext = await pwRequest.newContext({ baseURL: API });
    const ws = await (await ctx.post("/api/workspaces", { headers, data: { name: wsName } })).json();
    workspace_id = ws.id;
    const proj = await (await ctx.post(`/api/workspaces/${ws.id}/projects`, { headers, data: { name: "card-detail" } })).json();
    project_id = proj.id;
    const card = await (
      await ctx.post("/api/task/cards", {
        headers,
        data: { project_id, column_id: "todo", title: "detail card", description: "", priority: "normal" },
      })
    ).json();
    card_id = card.id;
    await ctx.dispose();
  });

  test.afterAll(async () => {
    const ctx = await pwRequest.newContext({ baseURL: API });
    const headers = { Authorization: `Bearer ${token}` };
    if (card_id) await ctx.delete(`/api/task/cards/${card_id}`, { headers });
    if (agent_id) await ctx.delete(`/api/agents/${agent_id}`, { headers });
    if (workspace_id) await ctx.delete(`/api/workspaces/${workspace_id}`, { headers });
    await ctx.dispose();
  });

  async function open_board(page: import("@playwright/test").Page) {
    await page.goto("/task");
    // Pick our workspace + project in the dock so the board shows our card.
    await page.selectOption('select[aria-label="workspace"]', { label: wsName });
    await page.selectOption('select[aria-label="project"]', { label: "card-detail" });
    const card = page.locator(".task-card", { hasText: "detail card" });
    await expect(card).toBeVisible();
    return card;
  }

  async function open_detail(page: import("@playwright/test").Page) {
    const card = await open_board(page);
    await card.locator(".task-title-link").click();
    await expect(page.locator(".task-detail")).toBeVisible();
  }

  async function close_detail(page: import("@playwright/test").Page) {
    await page.click('button[aria-label="close"]');
    await expect(page.locator(".task-detail")).toBeHidden();
  }

  test("two-pane layout renders with left form, tabs and right fields", async ({ page, login }) => {
    await open_detail(page);
    const left = page.locator(".task-detail-left");
    const right = page.locator(".task-detail-right");
    await expect(left.locator('input[placeholder="title"]')).toBeVisible();
    await expect(left.locator('textarea[placeholder="description…"]')).toBeVisible();
    await expect(left.locator('[role="tab"]', { hasText: "comments" })).toBeVisible();
    await expect(left.locator('[role="tab"]', { hasText: "history" })).toBeVisible();
    for (const id of [
      "#task-detail-priority",
      "#task-detail-bot",
      "#task-detail-person",
      "#task-detail-deadline",
      "#task-detail-schedule",
    ]) {
      await expect(right.locator(id)).toBeVisible();
    }
  });

  test("edit title/description and save updates the card", async ({ page, login }) => {
    await open_detail(page);
    const left = page.locator(".task-detail-left");
    await left.locator('input[placeholder="title"]').fill("renamed via e2e");
    await left.locator('textarea[placeholder="description…"]').fill("description from e2e");
    await left.locator('.task-detail-actions button[type="submit"]').click();
    await expect(page.locator(".task-card", { hasText: "renamed via e2e" })).toBeVisible();
    // Restore the shared title — other tests locate the card by it.
    await page.locator(".task-card", { hasText: "renamed via e2e" }).locator(".task-title-link").click();
    await left.locator('input[placeholder="title"]').fill("detail card");
    await left.locator('.task-detail-actions button[type="submit"]').click();
    await expect(page.locator(".task-card", { hasText: "detail card" })).toBeVisible();
  });

  test("priority change in right pane applies immediately", async ({ page, login }) => {
    await open_detail(page);
    await page.selectOption("#task-detail-priority", "high");
    await close_detail(page);
    const card = page.locator(".task-card", { hasText: "detail card" });
    await expect(card.locator(".task-prio")).toHaveText(/high/);
    // Reopen: the select still shows high.
    await card.locator(".task-title-link").click();
    await expect(page.locator("#task-detail-priority")).toHaveValue("high");
    await page.selectOption("#task-detail-priority", "normal");
  });

  test("deadline set in right pane persists across reopen", async ({ page, login }) => {
    await open_detail(page);
    await page.fill("#task-detail-deadline", "2030-01-31");
    // wait for the immediate PUT to land
    await expect
      .poll(async () => {
        const res = await page.request.get("/api/task/cards", {
          headers: { Authorization: `Bearer ${token}` },
        });
        const cards = (await res.json()) as Array<{ id: number; deadline: string | null }>;
        return cards.find((c) => c.id === card_id)?.deadline;
      })
      .toBe("2030-01-31");
    await close_detail(page);
    const card = page.locator(".task-card", { hasText: "detail card" });
    await card.locator(".task-title-link").click();
    await expect(page.locator("#task-detail-deadline")).toHaveValue("2030-01-31");
    await page.fill("#task-detail-deadline", "");
  });

  test("comments tab posts a comment", async ({ page, login }) => {
    await open_detail(page);
    const body = "plain comment from e2e";
    await page.fill(".task-comments input", body);
    await page.click(".task-comments form button[type='submit']");
    await expect(page.locator(".task-comment", { hasText: body })).toBeVisible();
  });

  test("mentioning an agent in a comment gets a chat reply", async ({ page, login }) => {
    // Seed a saved agent, then attach it to the card via the right pane.
    const headers = { Authorization: `Bearer ${token}` };
    const agentName = `e2e-agent-${Date.now()}`;
    const res = await page.request.post("/api/agents", {
      headers,
      data: { name: agentName, model: "", persona: "", prompt: "", output: "" },
    });
    expect(res.ok()).toBeTruthy();
    agent_id = (await res.json()).id;

    await open_detail(page);
    await page.selectOption("#task-detail-bot", { label: agentName });
    await close_detail(page);
    await expect(
      page.locator(".task-card", { hasText: "detail card" }).locator(".task-agent")
    ).toContainText(agentName);
    await page
      .locator(".task-card", { hasText: "detail card" })
      .locator(".task-title-link")
      .click();
    await expect(page.locator(".task-detail")).toBeVisible();

    // Deterministic chat: intercept the engine call.
    await page.route("**/api/chat", async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ reply: "mocked agent answer", model: "mock", searched: false }),
      });
    });

    await page.fill(".task-comments input", `@${agentName} what is the status?`);
    await page.click(".task-comments form button[type='submit']");
    await expect(page.locator(".task-comment.task-comment-agent", { hasText: "mocked agent answer" })).toBeVisible({ timeout: 10_000 });
  });

  test("history tab shows the run timeline after a run", async ({ page, login }) => {
    // The card has no assigned agent, so the run fails fast ("no agent
    // assigned") — deterministic, no model or sandbox needed.
    await open_detail(page);
    await page.click(".task-run-inline");
    await page.locator('[role="tab"]', { hasText: "history" }).click();
    await expect(page.locator(".run-timeline")).toBeVisible();
    await expect(page.locator(".run-timeline")).toContainText(/passed|failed/);
  });
});
