// Dock modals (SideNav): machines (Monitor icon) and activity. The old
// popovers became Modals — machines opens MachinesModal, workspace creation
// lives in ProfileModal → PromptModal.
// No mock-model gating — these only need the logged-in stack.
import { test, expect } from "./fixtures";
import type { Page } from "@playwright/test";

const MODAL_SETTLE_MS = 10_000;

function dialogWithTitle(page: Page, title: string) {
  return page.locator('[role="dialog"]').filter({
    has: page.locator("h2", { hasText: title }),
  });
}

test.describe("dock modals", () => {
  test("machines modal shows the machine register", async ({ page, login }) => {
    await page.goto("/kanban");
    await page.click('button[title="machines"]');
    const modal = dialogWithTitle(page, "machines");
    await expect(modal).toBeVisible();

    // Register settles to rows or the empty state; the local backend host
    // always appears as a machine-group head.
    const list = modal.locator(".dock-machines-list");
    await expect(
      list.locator(".dock-machine-row, .dock-ws-empty").first()
    ).toBeVisible({ timeout: MODAL_SETTLE_MS });

    // Refresh re-fetches without closing the modal.
    await modal.locator('button[aria-label="refresh machines"]').click();
    await expect(modal).toBeVisible();
  });

  test("creating a workspace from the profile modal shows up in the activity feed", async ({
    page,
    login,
  }) => {
    await page.goto("/kanban");
    const name = `e2e-dock-ws-${Date.now()}`;

    // Profile modal → "+ new workspace" opens the PromptModal.
    await page.click('button[title="profile"]');
    const profile = dialogWithTitle(page, "profile");
    await expect(profile).toBeVisible();
    await profile.locator('button[aria-label="new workspace"]').click();
    const prompt = dialogWithTitle(page, "new workspace");
    await expect(prompt).toBeVisible();
    await prompt.locator('input[placeholder="name…"]').fill(name);
    await prompt.locator("form").getByRole("button", { name: "create" }).click();
    await expect(prompt).toBeHidden();
    await profile.locator('button[aria-label="close"]').click();
    await expect(page.locator('[role="dialog"]')).toBeHidden();

    // Activity modal fetches on open; newest-first feed carries the event.
    await page.click('button[title="activity"]');
    const activity = dialogWithTitle(page, "activity");
    await expect(activity).toBeVisible();
    await expect(
      activity.locator(".dock-activity-msg", { hasText: `created workspace ${name}` })
    ).toBeVisible({ timeout: MODAL_SETTLE_MS });
  });
});
