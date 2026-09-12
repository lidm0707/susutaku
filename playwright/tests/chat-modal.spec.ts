// Chat as a global modal: the floating fab opens a role=dialog (ChatModal).
// Runs only against the backend-in-container stack with the mock model
// (same gating as chat-mock.spec.ts).
import { type APIRequestContext } from "@playwright/test";
import { expect, test } from "./fixtures";
import { API, loginToken } from "./helpers";

const MOCK_MODEL = "e2e-mock";
const AGENT_NAME = "e2e-modal-agent";
const CONTEXT_PREFIX = "[context: user is currently on the kanban page]";
const ECHO_TEXT = "hello modal";
const CHAT_TIMEOUT_MS = 20_000;

test.skip(
  process.env.E2E_MOCK_MODEL !== "1",
  "needs the mock-model backend stack (docker/compose/playwright-backend.yml)",
);

// The modal refuses to send without an agent ("no agent — create one under
// agents"); an agent whose model matches the mock model makes the modal post
// to /api/chat directly, and the mock echoes the full contexted message.
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

// Mirror of the modal send: same context prefix, same /api/chat body.
async function rawChatEcho(request: APIRequestContext, text: string): Promise<string> {
  const token = await loginToken();
  const res = await request.post(`${API}/api/chat`, {
    headers: { Authorization: `Bearer ${token}` },
    data: {
      message: `${CONTEXT_PREFIX}\n\n${text}`,
      max_tokens: 512,
      search: "off",
    },
  });
  expect(res.ok()).toBeTruthy();
  const body = (await res.json()) as { reply?: string };
  return body.reply ?? "";
}

async function openChatModal(page: import("@playwright/test").Page) {
  await page.goto("/kanban");
  const fab = page.locator("button[aria-label='open chat']");
  await expect(fab).toBeVisible();
  await fab.click();
  const dialog = page.locator('[role="dialog"]');
  await expect(dialog).toBeVisible();
  return dialog;
}

test.describe("chat modal", () => {
  test("fab opens the chat modal with agent select", async ({ page, login }) => {
    const dialog = await openChatModal(page);
    await expect(dialog.locator("h2", { hasText: "susutaku" })).toBeVisible();
    await expect(dialog.locator('select[title="agent"]')).toBeVisible();
    await expect(
      dialog.getByPlaceholder(/type a message|generating…/)
    ).toBeVisible();
    // The fab hides while the modal is open.
    await expect(page.locator("button[aria-label='open chat']")).toBeHidden();
  });

  test("sending from kanban carries page context in the echoed reply", async ({
    page,
    login,
    request,
  }) => {
    const agentName = await seedChatAgent(request);
    const dialog = await openChatModal(page);
    const select = dialog.locator('select[title="agent"]');
    const value = await select
      .locator("option", { hasText: agentName })
      .getAttribute("value");
    await select.selectOption(value);

    await dialog.getByPlaceholder(/type a message/).fill(ECHO_TEXT);
    await dialog.locator('form button[type="submit"]').click();

    // The contexted prompt echoes its first line — the context prefix.
    const reply = page.locator(".bubble.assistant p", {
      hasText: `echo: ${CONTEXT_PREFIX}`,
    });
    await expect(reply).toBeVisible({ timeout: CHAT_TIMEOUT_MS });
    // Assert the send path end-to-end via the API as well: same prefix,
    // same /api/chat body, mock echoes the contexted first line back.
    const echo = await rawChatEcho(request, ECHO_TEXT);
    expect(echo).toContain(CONTEXT_PREFIX);

    // Close button dismisses the modal and the fab comes back.
    await dialog.locator('button[aria-label="close"]').click();
    await expect(page.locator('[role="dialog"]')).toBeHidden();
    await expect(page.locator("button[aria-label='open chat']")).toBeVisible();
  });
});
