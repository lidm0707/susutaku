import { test, expect } from "./fixtures";
import { API } from "./helpers";

test.describe("claude provider", () => {
  test("claude status endpoint reports token state and cli availability", async ({
    request,
  }) => {
    const res = await request.get(`${API}/api/auth/claude/status`);
    expect(res.ok()).toBeTruthy();
    const body = await res.json();
    expect([
      "logged_in",
      "expired",
      "missing",
      "awaiting_login",
      "failed",
    ]).toContain(body.status);
    expect(typeof body.cli_available).toBe("boolean");
  });

  test("claude callback rejects empty code", async ({ request }) => {
    const res = await request.post(`${API}/api/auth/claude/callback`, {
      data: { code: "" },
    });
    expect(res.status()).toBe(400);
  });

  test("claude callback without a started login fails", async ({ request }) => {
    const res = await request.post(`${API}/api/auth/claude/callback`, {
      data: { code: "bogus-code" },
    });
    expect(res.ok()).toBeFalsy();
  });

  test("chat page shows claude provider option and login button", async ({
    page,
    login,
  }) => {
    await page.goto("/chat");
    await expect(
      page.locator('select[title="model"] option[value="claude"]')
    ).toHaveCount(1);
    await expect(page.getByRole("button", { name: /claude/ }).first()).toBeVisible();
  });

  test("selecting claude provider switches the status line", async ({
    page,
    login,
  }) => {
    await page.goto("/chat");
    await page.selectOption('select[title="model"]', "claude");
    await expect(page.locator("header .sub")).toHaveText(/claude/);
  });
});
