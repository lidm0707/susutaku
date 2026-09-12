// Chat as a global modal: the floating fab opens a role=dialog (ChatModal).
// Runs only against the backend-in-container stack with the mock model
// (same gating as chat-mock.spec.ts).
import { type APIRequestContext } from "@playwright/test";
import { expect, test } from "./fixtures";
import { API, loginToken } from "./helpers";

const MOCK_MODEL = "e2e-mock";
const AGENT_NAME_PREFIX = "e2e-modal-agent";
const CONTEXT_PREFIX = "[context: user is currently on the kanban page]";
const ECHO_TEXT = "hello modal";
const CHAT_TIMEOUT_MS = 20_000;

test.skip(
  process.env.E2E_MOCK_MODEL !== "1",
  "needs the mock-model backend stack (docker/compose/playwright-backend.yml)",
);

// The modal refuses to send without an agent; an agent whose model matches
// the mock model makes the modal post to /api/chat directly, and the mock
// echoes the full contexted message.
async function seedChatAgent(request: APIRequestContext): Promise<string> {
  const token = await loginToken();
  const headers = { Authorization: `Bearer ${token}` };
  // The DB persists across runs (E2E_SKIP_DB_LIFECYCLE=1); stale seeded agents
  // would pile up and the modal sends one message per selected agent.
  const existing = (await (await request.get(`${API}/api/agents`, { headers })).json()) as {
    id: number;
    name: string;
  }[];
  for (const a of existing.filter((a) => a.name.startsWith(AGENT_NAME_PREFIX))) {
    await request.delete(`${API}/api/agents/${a.id}`, { headers });
  }
  const name = `${AGENT_NAME_PREFIX}-${Date.now()}`;
  const res = await request.post(`${API}/api/agents`, {
    headers,
    data: { name, model: MOCK_MODEL, persona: "", prompt: "", output: "" },
  });
  expect(res.ok()).toBeTruthy();
  return name;
}

// Pick exactly one agent in the picker menu — the modal auto-selects agents
// on open, and any extra checked agent would duplicate reply bubbles.
async function selectOnlyAgent(page: import("@playwright/test").Page, name: string) {
  const dialog = page.locator('[role="dialog"]');
  await dialog.locator('button[title="choose agent(s)"]').click();
  const items = dialog.locator('[role="menuitemcheckbox"]');
  const count = await items.count();
  for (let i = 0; i < count; i++) {
    const item = items.nth(i);
    const checked = (await item.getAttribute("aria-checked")) === "true";
    const isTarget = (await item.textContent())?.includes(name) ?? false;
    if (checked !== isTarget) await item.click();
  }
  await dialog.locator('button[title="choose agent(s)"]').click();
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
  test("fab opens the chat modal with agent picker", async ({ page, login }) => {
    const dialog = await openChatModal(page);
    await expect(dialog.locator("h2", { hasText: "susutaku" })).toBeVisible();
    await expect(dialog.locator('button[title="choose agent(s)"]')).toBeVisible();
    await expect(
      dialog.getByPlaceholder(/type a message|generating…/)
    ).toBeVisible();
  });

  test("sending from kanban carries page context in the echoed reply", async ({
    page,
    login,
    request,
  }) => {
    const agentName = await seedChatAgent(request);
    const dialog = await openChatModal(page);
    await selectOnlyAgent(page, agentName);

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
