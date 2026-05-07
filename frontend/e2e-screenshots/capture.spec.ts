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

// ── README Screenshots ──────────────────────────────────────────
// Dedicated describe block: these tests fill the <!-- SCREENSHOT:name -->
// markers in README.md and rely on the demo app being seeded by
// scripts/seed-demo.sh (run by the demo-seeder service in compose).
//
// Unlike the screenshots above which inject a mock auth state to bypass
// the login screen, these tests perform a real login and use the
// resulting JWT to query the API — that way the dashboard is rendered
// with the real seeded data.

test.describe('README Screenshots', () => {
  test.beforeEach(async ({ page, request }) => {
    // Real login. Seed config uses email "admin@localhost" and any
    // password is accepted in demo auth mode.
    const resp = await request.post('/api/v1/auth/login', {
      data: { email: 'admin@localhost', password: 'demo' },
    });
    const data = resp.ok() ? await resp.json() : null;

    await page.goto('/login');
    await page.evaluate((d) => {
      const fallback = {
        token: 'screenshot-token',
        user: {
          id: '00000000-0000-0000-0000-000000000001',
          email: 'admin@appcontrol.local',
          name: 'Admin User',
          org_id: '00000000-0000-0000-0000-000000000001',
          role: 'admin',
        },
      };
      const state = d ?? fallback;
      localStorage.setItem(
        'appcontrol-auth',
        JSON.stringify({ state, version: 0 }),
      );
    }, data);
    await page.goto('/');
    await page.waitForTimeout(800);
  });

  // Navigate to the first available application's map, querying the
  // API directly to avoid relying on DOM selectors that may not have
  // a stable testid yet.
  async function openFirstAppMap(page: Page) {
    const appId = await page.evaluate(async () => {
      try {
        const auth = localStorage.getItem('appcontrol-auth');
        const token = auth ? JSON.parse(auth).state?.token : null;
        if (!token) return null;
        const resp = await fetch('/api/v1/apps', {
          headers: { Authorization: `Bearer ${token}` },
        });
        if (!resp.ok) return null;
        const apps = await resp.json();
        return Array.isArray(apps) && apps.length > 0 ? apps[0].id : null;
      } catch {
        return null;
      }
    });
    if (appId) {
      await page.goto(`/apps/${appId}`);
    } else {
      // Last-resort fallback: legacy demo route. Will render an empty
      // state if no app is seeded — better than nothing.
      await page.goto('/apps/demo');
    }
    await page.waitForTimeout(2500);
  }

  test('map-overview', async ({ page }) => {
    // Hero shot at the top of README.md — the application map with all
    // its components and dependencies visible.
    await openFirstAppMap(page);
    await capture(page, 'map-overview');
  });

  test('incident-recovery', async ({ page }) => {
    // "Dimanche 3h17" section. Best-effort: surface the detail panel
    // for a component so the shot shows both the map and a per-component
    // view (state, history, checks). True FAILED states require an
    // active agent + flag-file removal, which is a follow-up chantier.
    await openFirstAppMap(page);
    const node = page.locator('.react-flow__node').first();
    if (await node.isVisible({ timeout: 2000 }).catch(() => false)) {
      await node.click();
      await page.waitForTimeout(900);
    }
    await capture(page, 'incident-recovery');
  });

  test('dr-switchover', async ({ page }) => {
    // "Mardi 14h" section. Try to open the Switchover panel via the
    // toolbar; fall back to the bare map if the trigger is not
    // discoverable without a stable testid.
    await openFirstAppMap(page);
    const candidates = [
      'button:has-text("Switchover")',
      'button:has-text("Bascule")',
      'button[title*="witchover" i]',
      'button[aria-label*="witchover" i]',
    ];
    for (const selector of candidates) {
      const btn = page.locator(selector).first();
      if (await btn.isVisible({ timeout: 800 }).catch(() => false)) {
        await btn.click().catch(() => {});
        await page.waitForTimeout(1200);
        break;
      }
    }
    await capture(page, 'dr-switchover');
  });

  test('audit-export', async ({ page }) => {
    // "Vendredi 10h" section. Reports page after the seed has run —
    // there is at least one action_log entry for the import itself.
    await page.goto('/reports');
    await page.waitForTimeout(1500);
    await capture(page, 'audit-export');
  });

  // mcp-claude-control: pending the in-product chat bubble component.
  // The MCP server exists as the `mcp/` Rust crate, but no inline UI
  // surface is shipped yet. Once <McpChatBubble /> lands, add a test
  // that types a sample question into it and captures the response.
});
