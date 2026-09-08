import { test, expect } from "./fixtures";

test.describe("settings", () => {
  test("client env tab shows server-detected host and shares workspace path", async ({
    page,
    login,
  }) => {
    await page.goto("/settings");
    const hostCard = page.locator(".env-card", { hasText: "host machine" });
    await expect(hostCard.locator("dd").first()).not.toHaveText("—");

    const path = `/tmp/e2e-workspace-${Date.now()}`;
    await page.getByLabel("workspace path on this computer").fill(path);
    await page.getByRole("button", { name: "share env with backend" }).click();
    await expect(page.locator(".saved-mark")).toHaveText("shared with backend");
    await expect(hostCard.locator("dd").nth(3)).toHaveText(path);
  });
});
