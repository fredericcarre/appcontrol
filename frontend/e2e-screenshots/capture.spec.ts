import { test } from '@playwright/test';
import path from 'path';

const SCREENSHOT_DIR = path.resolve(__dirname, '../../docs/screenshots');

/**
 * Automated screenshot capture for documentation.
 *
 * These screenshots are embedded in docs/USER_GUIDE.md via
 * <!-- SCREENSHOT:name --> markers. The CI workflow replaces
 * these markers with actual image references after capture.
 *
 * Prerequisites:
 * - Full stack running (docker compose up)
 * - Seeded with demo data (the seed script creates sample apps)
 */

test.describe('Documentation Screenshots', () => {
  test.beforeEach(async ({ page }) => {
    // Log in first (dev mode: use the seeded admin credentials)
    // In dev mode, the frontend stores auth in localStorage
    await page.goto('/login');

    // Wait for the page to load — if already logged in, redirect happens
    await page.waitForTimeout(1000);

    // If we're still on login, perform login
    if (page.url().includes('/login')) {
      // Dev mode login: the OIDC/SAML flow would redirect.
      // For screenshot purposes, we inject a mock auth state.
      await page.evaluate(() => {
        const authState = {
          state: {
            token: 'screenshot-token',
            user: {
              id: '00000000-0000-0000-0000-000000000001',
              email: 'admin@appcontrol.local',
              name: 'Admin User',
              org_id: '00000000-0000-0000-0000-000000000001',
              role: 'admin',
            },
          },
          version: 0,
        };
        localStorage.setItem('appcontrol-auth', JSON.stringify(authState));
      });
      await page.goto('/');
      await page.waitForTimeout(500);
    }
  });

  test('dashboard', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(1500);
    await page.screenshot({
      path: path.join(SCREENSHOT_DIR, 'dashboard.png'),
      fullPage: false,
    });
  });

  test('map-view', async ({ page }) => {
    // Navigate to first app if available, otherwise just capture the page
    await page.goto('/');
    await page.waitForTimeout(1000);

    // Try clicking on an app card to open map view
    const appCard = page.locator('[data-testid="app-card"]').first();
    if (await appCard.isVisible()) {
      await appCard.click();
      await page.waitForTimeout(1500);
    } else {
      // Fallback: go to a demo URL
      await page.goto('/apps/demo');
      await page.waitForTimeout(1000);
    }

    await page.screenshot({
      path: path.join(SCREENSHOT_DIR, 'map-view.png'),
      fullPage: false,
    });
  });

  test('agents', async ({ page }) => {
    await page.goto('/agents');
    await page.waitForTimeout(1500);
    await page.screenshot({
      path: path.join(SCREENSHOT_DIR, 'agents.png'),
      fullPage: false,
    });
  });

  test('teams', async ({ page }) => {
    await page.goto('/teams');
    await page.waitForTimeout(1500);
    await page.screenshot({
      path: path.join(SCREENSHOT_DIR, 'teams.png'),
      fullPage: false,
    });
  });

  test('reports', async ({ page }) => {
    await page.goto('/reports');
    await page.waitForTimeout(1500);
    await page.screenshot({
      path: path.join(SCREENSHOT_DIR, 'reports.png'),
      fullPage: false,
    });
  });

  test('settings', async ({ page }) => {
    await page.goto('/settings');
    await page.waitForTimeout(1500);
    await page.screenshot({
      path: path.join(SCREENSHOT_DIR, 'settings.png'),
      fullPage: false,
    });
  });

  test('api-keys', async ({ page }) => {
    await page.goto('/settings/api-keys');
    await page.waitForTimeout(1500);
    await page.screenshot({
      path: path.join(SCREENSHOT_DIR, 'api-keys.png'),
      fullPage: false,
    });
  });

  test('enrollment', async ({ page }) => {
    await page.goto('/enrollment');
    await page.waitForTimeout(1500);
    await page.screenshot({
      path: path.join(SCREENSHOT_DIR, 'enrollment.png'),
      fullPage: false,
    });
  });

  test('import', async ({ page }) => {
    await page.goto('/import');
    await page.waitForTimeout(1500);
    await page.screenshot({
      path: path.join(SCREENSHOT_DIR, 'import.png'),
      fullPage: false,
    });
  });

  test('map-view-multi-site', async ({ page }) => {
    // Navigate to first app to capture multi-site split-node visualization.
    // When site_overrides are configured, component nodes display split panels
    // showing primary + DR site status side by side.
    await page.goto('/');
    await page.waitForTimeout(1000);

    const appCard = page.locator('[data-testid="app-card"]').first();
    if (await appCard.isVisible()) {
      await appCard.click();
      await page.waitForTimeout(2000);
    } else {
      await page.goto('/apps/demo');
      await page.waitForTimeout(1500);
    }

    // Look for multi-site panels (Sites label appears when overrides exist)
    const sitePanels = page.locator('text=Sites').first();
    if (await sitePanels.isVisible({ timeout: 2000 }).catch(() => false)) {
      await page.screenshot({
        path: path.join(SCREENSHOT_DIR, 'map-view-multi-site.png'),
        fullPage: false,
      });
    }
  });
});
