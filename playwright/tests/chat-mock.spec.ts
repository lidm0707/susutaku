import { type APIRequestContext } from "@playwright/test";
import { expect, test } from "./fixtures";
import { API, loginToken } from "./helpers";

// Runs only against the backend-in-container stack with the mock model:
//   docker compose -f docker/compose/playwright-backend.yml up --build --exit-code-from playwright
const MOCK_MODEL = "e2e-mock";
const TOOL_DONE_MARKER = "TOOLCALL-OK";
const TOOL_OUTPUT = "toolcall-ok";
const SUMMARY_MARKER = "MOCK-SUMMARY:";
const CHAT_TIMEOUT_MS = 20_000;
const AGENT_NAME = "e2e-agent";
const AGENT_OUTPUT_MARKER = "agent-run-ok";
const KANBAN_CONTEXT = "[context: user is currently on the kanban page]";

test.skip(
  process.env.E2E_MOCK_MODEL !== "1",
  "needs the mock-model backend stack (docker/compose/playwright-backend.yml)",
);

function mockChat(request: APIRequestContext, body: object) {
  return request.post(`${API}/api/chat`, { data: body });
}

// The chat modal only sends when an agent is selected; an agent whose model
// matches the mock model makes the modal post to /api/chat directly.
async function seedChatAgent(request: APIRequestContext): Promise<string> {
  const token = await loginToken();
  const name = `${AGENT_NAME}-${Date.now()}`;
  const res = await request.post(`${API}/api/agents`, {
    headers: { Authorization: `Bearer ${token}` },
    data: { name, model: MOCK_MODEL, persona: "", prompt: "", output: "" },
  });
  expect(res.ok()).toBeTruthy();
  return name;
}

async function selectAgent(page: import("@playwright/test").Page, name: string) {
  const select = page.locator('[role="dialog"] select[title="agent"]');
  const value = await select
    .locator("option", { hasText: name })
    .getAttribute("value");
  await select.selectOption(value);
}

async function openChatModal(page: import("@playwright/test").Page) {
  await page.goto("/kanban");
  await page.click("button.chat-fab");
  const dialog = page.locator('[role="dialog"]');
  await expect(dialog).toBeVisible();
  return dialog;
}

test.describe("chat against the mock model (backend in container)", () => {
  test("model catalog serves the mock model", async ({ request }) => {
    const res = await request.get(`${API}/api/models`);
    expect(res.ok()).toBeTruthy();
    const models = (await res.json()) as { name: string; selected: boolean }[];
    const mock = models.find((m) => m.name === MOCK_MODEL);
    expect(mock).toBeTruthy();
    expect(mock?.selected).toBe(true);
  });

  test("chat ui sends a message and shows the mock echo reply", async ({
    page,
    login,
    request,
  }) => {
    const agentName = await seedChatAgent(request);
    const dialog = await openChatModal(page);
    await selectAgent(page, agentName);
    // Plain chat (tools off): the mock echoes the contexted first line back.
    await dialog.locator('select[title="web search mode"]').selectOption("off");
    await dialog.getByPlaceholder(/type a message/).fill("hello mock");
    await dialog.locator('form button[type="submit"]').click();

    const reply = page.locator(".bubble.assistant p", {
      hasText: `echo: ${KANBAN_CONTEXT}`,
    });
    await expect(reply).toBeVisible({ timeout: CHAT_TIMEOUT_MS });
  });

  test("chat ui runs the toolcall round (TOOL: SHELL via backend sandbox)", async ({
    page,
    login,
    request,
  }) => {
    const agentName = await seedChatAgent(request);
    const dialog = await openChatModal(page);
    await selectAgent(page, agentName);
    // Auto mode: the mock answers with TOOL: SHELL echo toolcall-ok, the
    // backend runs it inside its sandbox and the mock then answers with the
    // marker.
    await dialog.locator('select[title="web search mode"]').selectOption("auto");
    await dialog.getByPlaceholder(/type a message/).fill("what can you do?");
    await dialog.locator('form button[type="submit"]').click();

    const reply = page.locator(".bubble.assistant p", {
      hasText: TOOL_DONE_MARKER,
    });
    await expect(reply).toBeVisible({ timeout: CHAT_TIMEOUT_MS });
    await expect(reply).toContainText(TOOL_OUTPUT);
  });

  test("summarize (compact) requests get the mock summary reply", async ({
    request,
  }) => {
    const token = await loginToken();
    const headers = { Authorization: `Bearer ${token}` };
    const longContext = "context ".repeat(200);
    const res = await mockChat(request, {
      headers,
      message: `summarize this context: ${longContext}`,
      max_tokens: 64,
      search: "off",
    });
    expect(res.ok()).toBeTruthy();
    const body = (await res.json()) as { reply: string };
    expect(body.reply.startsWith(SUMMARY_MARKER)).toBe(true);
  });

  test("an agent can be spawned and run inside that backend", async ({
    request,
  }) => {
    const res = await request.post(`${API}/api/manager/agents`, {
      data: { agent: AGENT_NAME },
    });
    expect(res.ok()).toBeTruthy();
    const spawned = (await res.json()) as { agent: string; work_tree: string };
    expect(spawned.agent).toBe(AGENT_NAME);
    expect(spawned.work_tree).toBeTruthy();

    const run = await request.post(
      `${API}/api/manager/agents/${AGENT_NAME}/run`,
      { data: { cmd: `echo ${AGENT_OUTPUT_MARKER}` } }
    );
    expect(run.ok()).toBeTruthy();
    const done = (await run.json()) as { output: string };
    expect(done.output).toContain(AGENT_OUTPUT_MARKER);
  });
});
