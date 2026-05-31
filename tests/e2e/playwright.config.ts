/**
 * Playwright E2E configuration for pointe.dev golden-path tests.
 *
 * Layer   : End-to-end — runs against the live production app at
 *           https://go.pointe.dev (or BASE_URL env override).
 * Covers  : Chat widget visibility, message send/receive, no JS console errors.
 * Does NOT cover: payment flow (requires Stripe test keys in the browser),
 *                 email confirmation (requires inbox access),
 *                 pipeline internal stages, Lighthouse performance metrics.
 */
import { defineConfig, devices } from "@playwright/test";

const BASE_URL = process.env.BASE_URL ?? "https://go.pointe.dev";

export default defineConfig({
  testDir: "./",
  testMatch: "**/*.spec.ts",

  /* Run tests in files in parallel */
  fullyParallel: false,

  /* Fail the build on CI if you accidentally left test.only in the source */
  forbidOnly: !!process.env.CI,

  /* Retry on CI only */
  retries: process.env.CI ? 2 : 0,

  /* Single worker in CI to avoid hammering production */
  workers: process.env.CI ? 1 : undefined,

  reporter: process.env.CI ? [["github"], ["list"]] : [["list"]],

  use: {
    baseURL: BASE_URL,
    /* Capture trace on retry so failures are debuggable */
    trace: "on-first-retry",
    /* Capture screenshot on failure */
    screenshot: "only-on-failure",
    /* Timeout for each action (e.g. click, fill) */
    actionTimeout: 15_000,
  },

  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],

  /* Global timeout per test */
  timeout: 60_000,
});
