import { type Page } from "@playwright/test";
import { expect, test } from "./fixtures";
import { API, loginToken } from "./helpers";

// Graph rendering in the chat modal: an assistant reply with a ```plot fence
// must render as a canvas via the wgpu wasm Plotter (or the 2D-canvas
// fallback) — and never throw wasm errors ("null pointer passed to rust",
// "wasm is undefined"). The chat backend is intercepted, so this runs against
// any stack (make deploy target included).

const AGENT_NAME_PREFIX = "e2e-graph-agent";
const PLOT_REPLY = [
  "here is your graph:",
  "```plot",
  '{"exprs":["y=x/2"],"x":[-10,10]}',
  "```",
].join("\n");

async function seedAgent(request: import("@playwright/test").APIRequestContext): Promise<string> {
  const token = await loginToken();
  const headers = { Authorization: `Bearer ${token}` };
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
    data: { name, model: "e2e-mock", persona: "", prompt: "", output: "" },
  });
  expect(res.ok()).toBeTruthy();
  return name;
}

async function selectAgent(page: Page, name: string) {
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

test.describe("chat plot fence renders a graph", () => {
  test("plot fence reply renders a graph canvas without wasm errors", async ({
    page,
    login,
    request,
  }) => {
    const errors: string[] = [];
    page.on("pageerror", (err) => errors.push(String(err)));

    const agentName = await seedAgent(request);
    // Intercept the chat call: the reply carries the plot fence, so the test
    // does not depend on any model actually speaking ```plot.
    await page.route("**/api/chat", (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ reply: PLOT_REPLY }),
      })
    );

    await page.goto("/task");
    await page.click("button[aria-label='open chat']");
    const dialog = page.locator('[role="dialog"]');
    await expect(dialog).toBeVisible();
    await selectAgent(page, agentName);
    await dialog.getByPlaceholder(/type a message/).fill("plot x/2");
    await dialog.locator('form button[type="submit"]').click();

    const plot = dialog.locator(".msg-plot");
    await expect(plot).toBeVisible();
    const canvas = plot.locator("canvas");
    await expect(canvas).toBeVisible();
    // The plotter (wasm or fallback) must actually paint a sized canvas.
    const box = await canvas.boundingBox();
    expect(box?.width ?? 0).toBeGreaterThan(50);
    expect(box?.height ?? 0).toBeGreaterThan(50);

    expect(errors, errors.join("\n")).toEqual([]);
  });
});
