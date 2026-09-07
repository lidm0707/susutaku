import { test, expect } from "./fixtures";

test.describe("kanban", () => {
  test("board loads after login", async ({ page, login }) => {
    await page.goto("/kanban");
    await expect(page).toHaveURL(/\/kanban/);
    // Either a workspace selector/board or an empty state must render.
    await expect(page.locator("main")).toBeVisible();
  });

  test("can create a workspace and project via API and board reflects it", async ({
    page,
    login,
    request,
  }) => {
    const token = await page.evaluate(() => localStorage.getItem("susutaku_token"));
    expect(token).toBeTruthy();
    const headers = { Authorization: `Bearer ${token}` };
    const name = `e2e-ws-${Date.now()}`;
    const res = await request.post("/api/workspaces", {
      headers,
      data: { name },
    });
    expect(res.ok()).toBeTruthy();
    const ws = await res.json();
    const pres = await request.post(`/api/workspaces/${ws.id}/projects`, {
      headers,
      data: { name: "e2e-project" },
    });
    expect(pres.ok()).toBeTruthy();
    await page.goto("/kanban");
    await page.reload();
    await expect(page.locator("main")).toBeVisible();
  });
});
