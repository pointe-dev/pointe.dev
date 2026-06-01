/**
 * Shared Playwright fixtures for pointe.dev E2E tests.
 *
 * Provides a `page` that injects the Cloudflare WAF bypass header
 * (X-CI-Token) on SAME-ORIGIN requests only.
 *
 * Why scoped, not a blanket extraHTTPHeaders: setting the header on the whole
 * browser context leaks it to third-party origins (cdn.fontshare.com fonts,
 * static.cloudflareinsights.com analytics). Those trigger a CORS preflight
 * that fails — "x-ci-token is not allowed by Access-Control-Allow-Headers" —
 * breaking font loading and surfacing as console errors. Routing only
 * go.pointe.dev requests keeps the header where Cloudflare needs it (to let
 * the Actions runner's datacenter IP past the block) and nowhere else.
 *
 * When CI_BYPASS_TOKEN is unset (local dev on a residential, non-blocked IP),
 * no route is registered and requests flow normally.
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
      await page.route(
        (requestUrl) => requestUrl.href.startsWith(BASE_URL),
        async (route) => {
          const headers = {
            ...route.request().headers(),
            "x-ci-token": CI_TOKEN,
          };
          await route.continue({ headers });
        }
      );
    }
    await use(page);
  },
});

export { expect } from "@playwright/test";
