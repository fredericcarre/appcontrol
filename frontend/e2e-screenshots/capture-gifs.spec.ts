import { test, Page } from '@playwright/test';
import path from 'path';
import { fileURLToPath } from 'url';

// frontend/package.json sets "type": "module", so __dirname is not
// defined. Reconstruct it from import.meta.url so the resolved path
// is stable both locally and in CI.
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Playwright writes each video to a per-test subfolder under
// outputDir; we save a copy alongside with a predictable name so
// the post-processing ffmpeg pass in CI can map test → output gif
// without parsing Playwright's internal naming scheme.
const VIDEO_DIR = path.resolve(__dirname, 'gif-videos');

async function realLogin(page: Page) {
  const resp = await page.request.post('/api/v1/auth/login', {
    data: { email: 'admin@localhost', password: 'admin' },
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
}

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
      const body = await resp.json();
      const list = Array.isArray(body) ? body : body?.apps;
      return Array.isArray(list) && list.length > 0 ? list[0].id : null;
    } catch {
      return null;
    }
  });
  if (appId) {
    await page.goto(`/apps/${appId}`);
  } else {
    await page.goto('/apps/demo');
  }
  await page.waitForTimeout(2500);
}

// Rename the recorded WebM into VIDEO_DIR with the test's title so
// the CI conversion step can find it. Playwright finalises the
// video only after the context is closed by the test runner, so we
// schedule the rename in afterEach with a path captured during the
// test.
test.afterEach(async ({ page }, testInfo) => {
  const video = page.video();
  if (!video) return;
  try {
    await page.close();
    const target = path.join(VIDEO_DIR, `${testInfo.title}.webm`);
    await video.saveAs(target);
  } catch {
    // Best-effort — if the video failed to finalise, the GIF will
    // simply be missing and the workflow will warn on upload.
  }
});

test.describe('GIF Recordings', () => {
  test.beforeEach(async ({ page }) => {
    await realLogin(page);
    await page.goto('/');
    await page.waitForTimeout(800);
  });

  // ────────────────────────────────────────────────────────────────
  // GIF 1 — "Dimanche 3h17". The seed pipeline has already left the
  // batch branch in FAILED. The operator opens the map, identifies
  // the red node, opens the action panel, and clicks "Start with
  // deps" to bring the branch back. With the agent's check interval
  // of ~30s, the components transition FAILED → STARTING → RUNNING
  // within the GIF window.
  // ────────────────────────────────────────────────────────────────
  test('incident-recovery', async ({ page }) => {
    await openFirstAppMap(page);

    // Frame the failed batch node first.
    const failedNode = page
      .locator('.react-flow__node:has-text("Batch Controller")')
      .first();
    if (await failedNode.isVisible({ timeout: 5000 }).catch(() => false)) {
      await failedNode.hover();
      await page.waitForTimeout(800);
      await failedNode.click();
      await page.waitForTimeout(1500);

      // Look for the "Start with deps" action button in the detail
      // panel. Falls back to a plain "Start" if not present.
      const startWithDeps = page
        .locator('button:has-text("Start with deps")')
        .first();
      const startBtn = page.locator('button:has-text("Start")').first();
      const target = (await startWithDeps
        .isVisible({ timeout: 1000 })
        .catch(() => false))
        ? startWithDeps
        : startBtn;
      if (await target.isVisible({ timeout: 1000 }).catch(() => false)) {
        await target.click();
        // Hold long enough for STARTING → RUNNING transitions.
        await page.waitForTimeout(15_000);
      }
    } else {
      // No failed node — still record a brief tour of the map so
      // the GIF is not zero bytes.
      await page.waitForTimeout(6000);
    }
  });

  // ────────────────────────────────────────────────────────────────
  // GIF 2 — "Mardi 14h". Open the Site Switchover modal so the
  // explanatory copy about the 6 phases is visible. We do NOT click
  // "Start Switchover" — the seed only ships one site, so an actual
  // bascule would fail. The modal alone shows the feature.
  // ────────────────────────────────────────────────────────────────
  test('dr-switchover', async ({ page }) => {
    await openFirstAppMap(page);

    // Try every reasonable selector for the switchover trigger.
    const candidates = [
      'button:has-text("Switchover")',
      'button:has-text("Bascule")',
      'button[title*="witchover" i]',
      'button[aria-label*="witchover" i]',
    ];
    let opened = false;
    for (const selector of candidates) {
      const btn = page.locator(selector).first();
      if (await btn.isVisible({ timeout: 1000 }).catch(() => false)) {
        await btn.hover();
        await page.waitForTimeout(500);
        await btn.click();
        await page.waitForTimeout(1500);
        opened = true;
        break;
      }
    }
    if (opened) {
      // Click the "Switchover Mode" dropdown to expose the options.
      const modeSelect = page.locator('text=Switchover Mode').first();
      if (await modeSelect.isVisible({ timeout: 1000 }).catch(() => false)) {
        await modeSelect.click({ force: true }).catch(() => {});
        await page.waitForTimeout(1000);
      }
      await page.waitForTimeout(4000);
    } else {
      await page.waitForTimeout(5000);
    }
  });

  // ────────────────────────────────────────────────────────────────
  // GIF 3 — "Vendredi 10h". Navigate from the dashboard to Reports,
  // surface the Audit Trail tab, scroll to show the timeline. The
  // seed pipeline left several action_log entries (Create Site,
  // Import Json, Create Enrollment Token, Issue Server Cert,
  // Start App) which populate the table.
  // ────────────────────────────────────────────────────────────────
  test('audit-export', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(800);
    const reportsLink = page
      .locator('a:has-text("Reports"), button:has-text("Reports")')
      .first();
    if (await reportsLink.isVisible({ timeout: 2000 }).catch(() => false)) {
      await reportsLink.click();
    } else {
      await page.goto('/reports');
    }
    await page.waitForTimeout(2500);

    // If the Audit Trail tab is not active, click it.
    const auditTab = page
      .locator('button:has-text("Audit Trail"), [role="tab"]:has-text("Audit Trail")')
      .first();
    if (await auditTab.isVisible({ timeout: 1000 }).catch(() => false)) {
      await auditTab.click();
      await page.waitForTimeout(1500);
    }

    // Scroll the audit table to suggest length / completeness.
    await page.mouse.wheel(0, 200);
    await page.waitForTimeout(2000);
    await page.mouse.wheel(0, -200);
    await page.waitForTimeout(1500);

    // Try clicking an Export button if one is visible.
    const exportBtn = page
      .locator('button:has-text("Export")')
      .first();
    if (await exportBtn.isVisible({ timeout: 1000 }).catch(() => false)) {
      await exportBtn.hover();
      await page.waitForTimeout(1200);
    }
  });
});
