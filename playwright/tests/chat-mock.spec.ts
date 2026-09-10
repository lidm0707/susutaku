import { type APIRequestContext } from "@playwright/test";
import { expect, test } from "./fixtures";
import { API, loginToken } from "./helpers";

// Runs only against the backend-in-container stack with the mock model:
//   docker compose -f docker/docker-compose.playwright-backend.yml up --build --exit-code-from playwright
const MOCK_MODEL = "e2e-mock";
const TOOL_DONE_MARKER = "TOOLCALL-OK";
const TOOL_OUTPUT = "toolcall-ok";
const SUMMARY_MARKER = "MOCK-SUMMARY:";
const CHAT_TIMEOUT_MS = 20_000;
const CHAT_POLL_MS = 500;
const AGENT_NAME = "e2e-agent";
const AGENT_OUTPUT_MARKER = "agent-run-ok";

test.skip(
  process.env.E2E_MOCK_MODEL !== "1",
  "needs the mock-model backend stack (docker/docker-compose.playwright-backend.yml)",
);

function mockChat(request: APIRequestContext, body: object) {
  return request.post(`${API}/api/chat`, { data: body });
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
  }) => {
    await page.goto("/chat");
    // Plain chat: no tool offer, the mock echoes the message back.
    await page.selectOption('select[title="web search mode"]', "off");
    await page.getByPlaceholder(/type a message/).fill("hello mock");
    await page.click('button[title="send"]');

    await expect(
      page.locator(".bubble.assistant p", { hasText: "echo: hello mock" })
    ).toBeVisible({ timeout: CHAT_TIMEOUT_MS });
    await expect(page.locator(".bubble.assistant p").last()).toContainText(
      "echo: hello mock"
    );
  });

  test("chat ui runs the toolcall round (TOOL: SHELL via backend sandbox)", async ({
    page,
    login,
  }) => {
    await page.goto("/chat");
    // auto: the mock answers with TOOL: SHELL echo toolcall-ok, the backend
    // runs it inside its sandbox and the mock then answers with the marker.
    await page.getByPlaceholder(/type a message/).fill("what can you do?");
    await page.click('button[title="send"]');

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
