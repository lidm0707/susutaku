// Chat dock resize: dragging .chat-dock-resize changes the docked panel width
// and the width persists across reloads (regression: saved "Npx" string was
// parsed with Number() -> NaN -> width reset on every load).
// No mock-model gating — only needs the logged-in stack.
import { test, expect } from "./fixtures";

const HANDLE = '.chat-dock-resize';

async function dock_width(page: import("@playwright/test").Page): Promise<number> {
  const box = page.locator(".overlay-box.modal.docked-right");
  await expect(box).toBeVisible();
  return (await box.boundingBox())!.width;
}

test.describe("chat dock resize", () => {
  test("dragging the handle resizes the docked chat and persists after reload", async ({
    page,
    login,
  }) => {
    await page.goto("/kanban");
    await page.click('button[aria-label="open chat"]');
    const dialog = page.locator('[role="dialog"]');
    await expect(dialog).toBeVisible();

    // Wait out the slideover-in animation: while it runs a transform on the
    // box shifts the handle, so hit-testing/measurements would be off.
    await page.waitForTimeout(400);

    const before = await dock_width(page);

    // Drag the handle left by 200px -> panel grows by 200px.
    const handle = dialog.locator(HANDLE);
    await expect(handle).toBeVisible();
    const hb = (await handle.boundingBox())!;
    await page.mouse.move(hb.x + hb.width / 2, hb.y + hb.height / 2);
    await page.mouse.down();
    await page.mouse.move(hb.x + hb.width / 2 - 200, hb.y + hb.height / 2, { steps: 5 });
    await page.mouse.up();

    const after = await dock_width(page);
    expect(after).toBeGreaterThanOrEqual(before + 150);

    // Width survives a reload (read back from localStorage on mount).
    await page.reload();
    await page.click('button[aria-label="open chat"]');
    await page.waitForTimeout(400);
    const persisted = await dock_width(page);
    expect(Math.abs(persisted - after)).toBeLessThanOrEqual(2);

    // Tidy up so other tests start from the default width.
    await page.evaluate(() => localStorage.removeItem("chat_dock_w_v2"));
  });
});
