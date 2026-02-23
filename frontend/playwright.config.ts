import { defineConfig } from '@playwright/test';

/**
 * Playwright configuration for automated UI screenshot generation.
 *
 * These tests are NOT functional tests — they capture screenshots
 * of each page for documentation. Screenshots are committed to
 * docs/screenshots/ and embedded in the User Guide.
 *
 * Run locally:  npx playwright test --project=screenshots
 * Run in CI:    automatically triggered by .github/workflows/docs-screenshots.yaml
 */
export default defineConfig({
  testDir: './e2e-screenshots',
  outputDir: './e2e-screenshots/results',
  timeout: 30_000,
  retries: 1,
  use: {
    baseURL: 'http://localhost:8080',
    screenshot: 'off', // We take screenshots manually in tests
    viewport: { width: 1440, height: 900 },
    colorScheme: 'light',
  },
  projects: [
    {
      name: 'screenshots',
      use: {
        browserName: 'chromium',
      },
    },
  ],
});
