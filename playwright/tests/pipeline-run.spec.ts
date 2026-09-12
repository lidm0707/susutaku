import { test, expect } from "./fixtures";
import type { Page, Locator } from "@playwright/test";

// Pipeline editor end-to-end: create a pipeline, add transform + output nodes,
// set params, wire them, save, and run a test — capturing a screenshot at
// every step into screenshots/pipeline-run/.
//
// transform(op=upper) runs deterministically on the backend (no model, no
// network), so the whole run must finish with status ok.
//
// The node icon (lucide svg inside a react-flow node) can disappear when
// react-flow re-renders / virtualizes during a run update. expect_node_ok
// polls for the run status class AND icon visibility in a loop, snapping a
// debug screenshot each time the icon is missing, until both are stable.

const OUT = "screenshots/pipeline-run";
const POLL_TIMEOUT_MS = 15_000;
const POLL_INTERVAL_MS = 250;

const PIPELINE_NAME = "e2e-pipe-run";
const SEED_TEXT = "hello pipeline run";
const TRANSFORM_OP = "upper";

async function shot(page: Page, name: string) {
  await page.waitForTimeout(400);
  await page.screenshot({ path: `${OUT}/${name}.png`, fullPage: false });
}

function node(page: Page, stage: string): Locator {
  return page.locator(`.react-flow__node:has-text("${stage}")`);
}

function icon(page: Page, stage: string): Locator {
  return node(page, stage).locator(".pipe-node-stage svg");
}

/// Poll until the node shows the given run status AND its stage icon is
/// visible. If the icon vanishes mid-poll (react-flow re-render), capture a
/// debug screenshot and keep polling — the icon comes back on the next render.
async function expect_node_ok(page: Page, stage: string) {
  const n = node(page, stage);
  await expect
    .poll(
      async () => {
        const hasStatus = await n
          .locator(".pipe-node")
          .evaluate((el) => el.classList.contains("pipe-node-ok"));
        const iconVisible = await icon(page, stage).isVisible().catch(() => false);
        if (hasStatus && !iconVisible) {
          await page.screenshot({
            path: `${OUT}/debug-icon-missing-${stage}.png`,
          });
        }
        return hasStatus && iconVisible;
      },
      { timeout: POLL_TIMEOUT_MS, intervals: [POLL_INTERVAL_MS] }
    )
    .toBe(true);
}

test.describe("pipeline run", () => {
  test("create, wire, save and test-run a pipeline", async ({ page, login }) => {
    await page.goto("/pipelines");
    await expect(page.locator('button:has-text("new pipeline")').first()).toBeVisible();
    await shot(page, "01-pipelines-list");

    // 1. create the pipeline
    await page.locator('button:has-text("new pipeline")').first().click();
    const newDlg = page.locator('[role="dialog"]');
    await expect(newDlg).toBeVisible();
    await newDlg.locator("input").fill(PIPELINE_NAME);
    await newDlg.locator('button[type="submit"]').click();
    await expect(newDlg).toBeHidden();
    await expect(page.locator(".pipeline-name")).toHaveValue(PIPELINE_NAME);
    await shot(page, "02-empty-canvas");

    // 2. add nodes from the dock
    await page.locator('button[aria-label="add transform node"]').click();
    await expect(node(page, "transform")).toHaveCount(1);
    await page.locator('button[aria-label="add output node"]').click();
    await expect(node(page, "output")).toHaveCount(1);
    await shot(page, "03-nodes-added");

    // 3. configure transform: op=upper
    await node(page, "transform").click();
    const inspector = page.locator(".pipeline-inspector");
    await expect(inspector).toBeVisible();
    await inspector.locator(".stage-field input").first().fill(TRANSFORM_OP);
    await shot(page, "04-transform-param");
    // output_resource requires a "name" param — set it while we're here
    await page.locator('.pipeline-toolbar button:has-text("node")').waitFor();
    await node(page, "output").click();
    await expect(inspector).toBeVisible();
    await inspector.locator(".stage-field input").first().fill("result");
    await shot(page, "04b-output-param");
    // blur applies the edit; close the inspector
    await page.locator(".pipeline-canvas").click({ position: { x: 10, y: 10 } });
    await expect(inspector).toBeHidden();

    // 4. wire transform -> output (drag bottom handle onto top handle)
    const src = node(page, "transform").locator(".react-flow__handle.source");
    const dst = node(page, "output").locator(".react-flow__handle.target");
    const a = await src.boundingBox();
    const b = await dst.boundingBox();
    expect(a, "transform source handle visible").not.toBeNull();
    expect(b, "output target handle visible").not.toBeNull();
    await page.mouse.move(a!.x + a!.width / 2, a!.y + a!.height / 2);
    await page.mouse.down();
    await page.mouse.move(b!.x + b!.width / 2, b!.y + b!.height / 2, { steps: 12 });
    await page.mouse.up();
    await expect(page.locator(".react-flow__edge")).toHaveCount(1);
    await shot(page, "05-wired");

    // 5. save and wait for the saved mark
    await page.locator('button:has-text("save pipeline")').click();
    await expect(page.locator(".saved-mark")).toHaveText(/saved/);
    await shot(page, "06-saved");

    // 6. test run with seed text
    await page.locator('.pipeline-toolbar button:has-text("test")').click();
    const testDlg = page.locator('[role="dialog"]');
    await expect(testDlg).toBeVisible();
    await testDlg.locator("input").fill(SEED_TEXT);
    await testDlg.locator('button[type="submit"]').click();
    await shot(page, "07-run-started");

    // 7. every node must flip to ok with its icon still rendered (loop on
    // icon disappearance instead of failing the step)
    await expect_node_ok(page, "transform");
    await expect_node_ok(page, "output");
    await expect(page.locator(".pipeline-run .run-note")).toContainText("test ok");
    await shot(page, "08-run-ok");
  });
});
