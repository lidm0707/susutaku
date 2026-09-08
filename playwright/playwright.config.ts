import { defineConfig } from "@playwright/test";

const BASE_URL = process.env.PLAYWRIGHT_BASE_URL || "http://localhost:3334";

export default defineConfig({
  globalSetup: "./setup.ts",
  globalTeardown: "./teardown.ts",
  testDir: "./tests",
  timeout: 30_000,
  retries: process.env.CI ? 1 : 0,
  workers: 1,
  reporter: [["list"]],
  use: {
    baseURL: BASE_URL,
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  expect: {
    toHaveScreenshot: {
      // Docker fonts render slightly differently across hosts; allow small drift.
      maxDiffPixelRatio: 0.02,
    },
  },
});
