import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';

test('@dashboard workshop dashboard supports full-page, HTMX, tablet, and phone states', async ({ page }, testInfo) => {
  await page.goto('/login');
  await page.getByLabel('Email').fill('browser-smoke@example.invalid');
  await page.getByLabel('Password').fill('Browser smoke password 123!');
  await page.getByRole('button', { name: 'Sign in' }).click();

  await expect(page).toHaveURL('/');
  await expect(page.getByRole('heading', { name: 'Welcome, Browser Smoke' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'New intervention' })).toHaveAttribute('href', '/vehicles');
  for (const action of ['New customer', 'Register vehicle', 'New invoice', 'New technical note']) {
    await expect(page.getByRole('link', { name: action })).toBeVisible();
  }
  await expect(page.getByRole('heading', { name: 'Draft interventions' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Recent service history' })).toBeVisible();
  if (testInfo.project.name === 'desktop-chromium') {
    await expect(page.getByText('No interventions have been recorded yet')).toBeVisible();
    await expect(page.getByText('There are no draft interventions')).toBeVisible();
  }
  await expect(page.getByText('Outstanding invoices')).toHaveCount(0);

  const draftRefresh = await page.request.get('/dashboard/draft-interventions', {
    headers: { 'HX-Request': 'true' },
  });
  expect(draftRefresh.status()).toBe(200);
  expect(draftRefresh.headers()['vary']).toContain('HX-Request');
  const fragment = await draftRefresh.text();
  expect(fragment).toContain('id="draft-interventions"');
  expect(fragment).not.toContain('<!doctype html>');
  expect(fragment).not.toContain('id="recent-interventions"');

  if (testInfo.project.name === 'desktop-chromium') {
    const accessibility = await new AxeBuilder({ page }).analyze();
    expect(accessibility.violations).toEqual([]);

    await page.setViewportSize({ width: 820, height: 1180 });
    await expect(page.locator('.sidebar')).toBeHidden();
    await expect(page.locator('.phone-navigation')).toBeVisible();
    await expect(page.locator('.dashboard-sections')).toHaveCSS('grid-template-columns', '788px');
  } else if (['phone-chromium', 'tablet-chromium'].includes(testInfo.project.name)) {
    await expect(page.locator('.sidebar')).toBeHidden();
    await expect(page.locator('.phone-navigation')).toBeVisible();
    const actions = page.locator('.quick-action-grid');
    await expect(actions).toHaveCSS('grid-template-columns', /[0-9.]+px [0-9.]+px/);
  }
});
