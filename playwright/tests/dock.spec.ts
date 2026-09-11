// Bottom dock popovers (SideNav): machines (Monitor icon) and activity.
// No mock-model gating — these only need the logged-in stack.
import { test, expect } from "./fixtures";

const POPOVER_VISIBLE_MS = 10_000;

test.describe("dock popovers", () => {
  test("machines popover shows the local server host section", async ({
    page,
    login,
  }) => {
    await page.goto("/kanban");
    await page.click('button[title="machines"]');
    const menu = page.locator('[role="menu"][aria-label="machines menu"]');
    await expect(menu).toBeVisible();

    // Host section: hostname + os/arch + cpu line render once loaded.
    const host = menu.locator(".dock-host");
    await expect(host).toBeVisible();
    await expect(host.locator(".dock-machine-name")).not.toHaveText(
      "host specs unavailable"
    );
    await expect(host).toContainText(/cores/);

    // Agents/sandboxes list settles (this stack may hold manager agents
    // from the chat specs, so accept rows or the empty state).
    await expect(
      menu.locator(".dock-machines-list").locator(".dock-machine-row, .dock-ws-empty").first()
    ).toBeVisible({ timeout: POPOVER_VISIBLE_MS });

    // Refresh re-fetches without closing the popover.
    await menu.locator('button[aria-label="refresh machines"]').click();
    await expect(menu).toBeVisible();
  });

  test("creating a workspace from the dock shows up in the activity feed", async ({
    page,
    login,
  }) => {
    await page.goto("/kanban");
    const name = `e2e-dock-ws-${Date.now()}`;

    // Dock "+ new workspace" opens the PromptModal.
    await page.click('button[aria-label="new workspace"]');
    const dialog = page.locator('[role="dialog"]', { hasText: "new workspace" });
    await expect(dialog).toBeVisible();
    await dialog.locator('input[placeholder="name…"]').fill(name);
    await dialog.locator("form").getByRole("button", { name: "create" }).click();
    await expect(dialog).toBeHidden();

    // Activity popover fetches on open; newest-first feed carries the event.
    await page.click('button[title="activity"]');
    const menu = page.locator('[role="menu"][aria-label="activity menu"]');
    await expect(menu).toBeVisible();
    await expect(
      menu.locator(".dock-activity-msg", { hasText: `created workspace ${name}` })
    ).toBeVisible({ timeout: POPOVER_VISIBLE_MS });
  });
});
