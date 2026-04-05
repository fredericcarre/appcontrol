import { test, Page } from '@playwright/test';
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

async function capture(page: Page, name: string, opts?: { fullPage?: boolean }) {
  await page.screenshot({
    path: path.join(SCREENSHOT_DIR, `${name}.png`),
    fullPage: opts?.fullPage ?? false,
  });
}

test.describe('Documentation Screenshots', () => {
  test.beforeEach(async ({ page }) => {
    // Log in first (dev mode: use the seeded admin credentials)
    await page.goto('/login');
    await page.waitForTimeout(1000);

    // If we're still on login, inject mock auth state
    if (page.url().includes('/login')) {
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

  // ── Authentication ───────────────────────────────────────────

  test('login', async ({ page }) => {
    // Clear auth to show login page
    await page.evaluate(() => localStorage.removeItem('appcontrol-auth'));
    await page.goto('/login');
    await page.waitForTimeout(1500);
    await capture(page, 'login');
  });

  // ── Main Pages ───────────────────────────────────────────────

  test('dashboard', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(2000);
    await capture(page, 'dashboard');
  });

  test('map-view', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(1000);

    // Click on the first app card to open map view
    const appCard = page.locator('[data-testid="app-card"]').first();
    if (await appCard.isVisible()) {
      await appCard.click();
      await page.waitForTimeout(2000);
    } else {
      await page.goto('/apps/demo');
      await page.waitForTimeout(1500);
    }
    await capture(page, 'map-view');
  });

  test('map-view-detail-panel', async ({ page }) => {
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

    // Click on a node to open the detail panel
    const node = page.locator('.react-flow__node').first();
    if (await node.isVisible({ timeout: 3000 }).catch(() => false)) {
      await node.click();
      await page.waitForTimeout(1000);
    }
    await capture(page, 'map-view-detail-panel');
  });

  test('map-view-multi-site', async ({ page }) => {
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

    // Only capture if multi-site panels are visible
    const sitePanels = page.locator('text=Sites').first();
    if (await sitePanels.isVisible({ timeout: 2000 }).catch(() => false)) {
      await capture(page, 'map-view-multi-site');
    }
  });

  // ── Onboarding & Import ──────────────────────────────────────

  test('onboarding', async ({ page }) => {
    await page.goto('/onboarding');
    await page.waitForTimeout(1500);
    await capture(page, 'onboarding');
  });

  test('onboarding-components', async ({ page }) => {
    await page.goto('/onboarding');
    await page.waitForTimeout(1000);

    // Navigate forward to the components step (step 4)
    // Click "Next" buttons to advance through the wizard
    for (let i = 0; i < 3; i++) {
      const nextBtn = page.locator('button:has-text("Next")').first();
      if (await nextBtn.isVisible({ timeout: 1000 }).catch(() => false)) {
        await nextBtn.click();
        await page.waitForTimeout(500);
      }
    }
    await page.waitForTimeout(500);
    await capture(page, 'onboarding-components');
  });

  test('import', async ({ page }) => {
    await page.goto('/import');
    await page.waitForTimeout(1500);
    await capture(page, 'import');
  });

  test('discovery', async ({ page }) => {
    await page.goto('/discovery');
    await page.waitForTimeout(1500);
    await capture(page, 'discovery');
  });

  // ── Infrastructure ───────────────────────────────────────────

  test('agents', async ({ page }) => {
    await page.goto('/agents');
    await page.waitForTimeout(1500);
    await capture(page, 'agents');
  });

  test('gateways', async ({ page }) => {
    await page.goto('/gateways');
    await page.waitForTimeout(1500);
    await capture(page, 'gateways');
  });

  test('enrollment', async ({ page }) => {
    await page.goto('/enrollment');
    await page.waitForTimeout(1500);
    await capture(page, 'enrollment');
  });

  test('sites', async ({ page }) => {
    await page.goto('/sites');
    await page.waitForTimeout(1500);
    await capture(page, 'sites');
  });

  // ── Users & Teams ────────────────────────────────────────────

  test('users', async ({ page }) => {
    await page.goto('/users');
    await page.waitForTimeout(1500);
    await capture(page, 'users');
  });

  test('teams', async ({ page }) => {
    await page.goto('/teams');
    await page.waitForTimeout(1500);
    await capture(page, 'teams');
  });

  // ── Reports & Settings ───────────────────────────────────────

  test('reports', async ({ page }) => {
    await page.goto('/reports');
    await page.waitForTimeout(1500);
    await capture(page, 'reports');
  });

  test('settings', async ({ page }) => {
    await page.goto('/settings');
    await page.waitForTimeout(1500);
    await capture(page, 'settings');
  });

  test('api-keys', async ({ page }) => {
    await page.goto('/settings/api-keys');
    await page.waitForTimeout(1500);
    await capture(page, 'api-keys');
  });

  // ── Special Modes ────────────────────────────────────────────

  test('supervision', async ({ page }) => {
    await page.goto('/supervision');
    await page.waitForTimeout(2000);
    await capture(page, 'supervision');
  });
});
