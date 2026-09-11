import { test as base } from "@playwright/test";
import { e2eCredentials, seedUser } from "./helpers";

type Fixtures = { login: void };

export const test = base.extend<Fixtures>({
  login: [
    async ({ page }, use) => {
      await seedUser();
      const { username, password } = e2eCredentials();
      await page.goto("/");
      await page.fill('input[placeholder="username"]', username);
      await page.fill('input[placeholder="password"]', password);
      await page.click('button[type="submit"]');
      await page.waitForURL("**/kanban");
      await use();
    },
    { auto: false },
  ],
});

export { expect } from "@playwright/test";
