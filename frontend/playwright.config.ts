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
  reporter: process.env.CI
    ? [['list'], ['html', { outputFolder: 'playwright-report', open: 'never' }]]
    : 'list',
  use: {
    trace: 'retain-on-failure',
    video: 'retain-on-failure',
    // Access app through nginx (HTTPS with self-signed cert)
    baseURL: 'https://localhost:443',
    ignoreHTTPSErrors: true, // Allow self-signed certificates
    screenshot: 'off', // We take screenshots manually in tests
    viewport: { width: 1440, height: 900 },
    colorScheme: 'light',
  },
  projects: [
    {
      name: 'screenshots',
      testIgnore: /capture-gifs\.spec\.ts/,
      use: {
        browserName: 'chromium',
      },
    },
    {
      // The README has three "Trois moments, trois clics" sections that
      // each open with a short animation rather than a static frame.
      // This project records a WebM per test; CI converts each file to
      // a GIF via ffmpeg and commits it next to the PNGs.
      name: 'gifs',
      timeout: 90_000,
      testMatch: /capture-gifs\.spec\.ts/,
      use: {
        browserName: 'chromium',
        video: { mode: 'on', size: { width: 1280, height: 800 } },
      },
    },
  ],
});
