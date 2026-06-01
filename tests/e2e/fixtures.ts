/**
 * Shared Playwright fixtures for pointe.dev E2E tests.
 *
 * Provides a `page` that injects the Cloudflare WAF bypass header
 * (X-CI-Token) on SAME-ORIGIN requests only via fetch interception.
 *
 * Uses addInitScript to wrap window.fetch before any page code runs, ensuring
 * the header is attached even when fetch() is called from page.evaluate().
 * The header is only added to same-origin requests (those starting with BASE_URL),
 * avoiding CORS preflight issues on third-party origins.
 *
 * When CI_BYPASS_TOKEN is unset (local dev on a residential, non-blocked IP),
 * no script is injected and requests flow normally.
 */
import { test as base } from "@playwright/test";

const env = (globalThis as {
  process?: { env?: Record<string, string | undefined> };
}).process?.env ?? {};

const CI_TOKEN = env.CI_BYPASS_TOKEN ?? "";
const BASE_URL = env.BASE_URL ?? "https://go.pointe.dev";

export const test = base.extend({
  page: async ({ page }, use) => {
    if (CI_TOKEN) {
      // Inject a fetch wrapper that adds the bypass header to all same-origin requests.
      // This ensures the header is attached even when fetch() is called from page.evaluate().
      await page.addInitScript(
        ({ token, baseUrl }) => {
          const originalFetch = window.fetch;
          window.fetch = function (...args: any[]) {
            const urlArg = args[0];
            const isString = typeof urlArg === "string";
            const urlStr = isString ? urlArg : urlArg.url;
            const urlObj = new URL(urlStr, window.location.origin);

            if (urlObj.href.startsWith(baseUrl)) {
              args[1] = args[1] || {};
              args[1].headers = args[1].headers || {};
              args[1].headers["x-ci-token"] = token;
            }
            return originalFetch.apply(this, args);
          };
        },
        { token: CI_TOKEN, baseUrl: BASE_URL }
      );
    }
    await use(page);
  },
});

export { expect } from "@playwright/test";
