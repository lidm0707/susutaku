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

  // The claude provider selector used to live on the /chat page
  // (select[title="model"] + a claude login button). Chat is now a global
  // modal (ChatModal.tsx) with only an agent select — the provider UI was
  // dropped, so there is nothing left to assert in the UI here.
});
