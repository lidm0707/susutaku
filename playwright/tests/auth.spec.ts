import { test, expect } from "./fixtures";
import { E2E_USER, E2E_PASSWORD } from "./helpers";

test.describe("auth", () => {
  test("login page renders and rejects bad credentials", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByRole("heading", { name: "login" })).toBeVisible();
    await page.fill('input[placeholder="username"]', "wronguser");
    await page.fill('input[placeholder="password"]', "wrongpass");
    await page.click('button[type="submit"]');
    await expect(page.locator("p.error")).toBeVisible();
    // Still on login page.
    await expect(page).toHaveURL(/\/$|\/login/);
  });

  test("e2e user can log in and reaches kanban", async ({ page }) => {
    await page.goto("/");
    await page.fill('input[placeholder="username"]', E2E_USER);
    await page.fill('input[placeholder="password"]', E2E_PASSWORD);
    await page.click('button[type="submit"]');
    await expect(page).toHaveURL(/\/kanban/);
  });

  test("unauthenticated user is redirected to login", async ({ page }) => {
    await page.goto("/kanban");
    await expect(page).toHaveURL(/\/$|\/login/);
  });
});
