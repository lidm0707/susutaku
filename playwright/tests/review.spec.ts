import { request as pwRequest, type APIRequestContext } from "@playwright/test";
import { expect, test } from "./fixtures";
import { API, loginToken } from "./helpers";

// Review page e2e against a running stack (deploy or dev):
//   E2E_SKIP_DB_LIFECYCLE=1 npx playwright test tests/review.spec.ts
// Covers the git review path: agent pick -> status -> diff -> commit, plus
// the fresh-work-tree cases (unborn HEAD must render "(no changes)", not 400).
const CHAT_TIMEOUT_MS = 15_000;
const COMMIT_MESSAGE_PREFIX = "review e2e commit";

let token: string;
let agentName: string;
let agentId: number | null = null;

async function api(): Promise<APIRequestContext> {
  return pwRequest.newContext({ baseURL: API });
}

test.beforeAll(async () => {
  token = await loginToken();
  const ctx = await api();
  const headers = { Authorization: `Bearer ${token}` };
  agentName = `e2e-review-${Date.now()}`;
  const res = await ctx.post(`${API}/api/agents`, {
    headers,
    data: { name: agentName, model: "", persona: "", prompt: "", output: "" },
  });
  expect(res.ok()).toBeTruthy();
  agentId = (await res.json()).id;
  // First git op auto-spawns the agent's slot + work tree (empty repo).
  const st = await ctx.post(`${API}/api/manager/agents/${agentName}/git`, {
    headers,
    data: { op: "status" },
  });
  expect(st.ok()).toBeTruthy();
  await ctx.dispose();
});

test.afterAll(async () => {
  if (!agentId) return;
  const ctx = await api();
  await ctx.delete(`${API}/api/agents/${agentId}`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  await ctx.dispose();
});

async function open_review(page: import("@playwright/test").Page) {
  await page.goto("/review");
  const list = page.locator(".review-agent-list");
  await expect(list).toBeVisible();
  const item = list.locator(".agent-item", { hasText: agentName });
  await item.click();
  await expect(item).toHaveClass(/active/);
}

test("fresh agent work tree: status shows no commits, diff renders empty (no 400)", async ({
  page,
  login,
}) => {
  await open_review(page);
  // Unborn HEAD: the diff panel must render its empty state (not an error
  // toast, not a 400), and the status badge must resolve without crashing.
  const panel = page.locator(".review-right .review-diff-panel");
  await expect(panel).toBeVisible();
  await expect(panel.locator(".review-badge")).toBeVisible();
  await expect(page.locator(".review-right .review-diff-none")).toBeVisible();
});

test("commit through the ui creates the initial commit and stays clean", async ({
  page,
  login,
}) => {
  await open_review(page);
  const right = page.locator(".review-right");
  const output = right.locator(".agent-review-output");

  // Actions are the 3 icon circles at the top left; commit opens the modal.
  const circles = page.locator(".review-left .review-action-circle");
  await expect(circles).toHaveCount(3);

  async function ui_commit(message: string) {
    await circles.first().click();
    const dialog = page.locator("[role=dialog]");
    await expect(dialog).toBeVisible();
    await dialog.locator("input").fill(message);
    await dialog.locator("button[type=submit]").click();
    await expect(output).toContainText("$ commit", { timeout: CHAT_TIMEOUT_MS });
  }

  // Empty work tree: commit must SUCCEED (no-op), not 400 with a cryptic
  // podman error — regression guard for the "nothing to commit" exit 1.
  await ui_commit("empty commit is a no-op");

  // Put a file in the work tree via a sandbox run, then the UI commit lands.
  const ctx = await api();
  const run = await ctx.post(`${API}/api/manager/agents/${agentName}/run`, {
    headers: { Authorization: `Bearer ${token}` },
    data: { cmd: "echo e2e > note.txt" },
  });
  await ctx.dispose();
  expect(run.ok()).toBeTruthy();

  await ui_commit(`${COMMIT_MESSAGE_PREFIX} ${Date.now()}`);

  // Status flips from "no commits" to a real HEAD and the tree is clean:
  // the badge turns ok/clean after the post-commit refresh.
  await expect(page.locator(".review-right .review-badge.ok")).toHaveText("clean", {
    timeout: CHAT_TIMEOUT_MS,
  });

  // Clean tree + no base branch: diff stays in the empty state.
  await expect(page.locator(".review-right .review-diff-none")).toBeVisible();
});
