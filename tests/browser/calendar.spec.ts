import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';

test('@calendar authenticated Month navigation and responsive entries work progressively', async ({ page }, testInfo) => {
  const variant = testInfo.project.name.replace('-chromium', '').replace('no-javascript', 'nojs');
  await signIn(page);
  const csrf = await page.locator('meta[name="csrf-token"]').getAttribute('content');
  expect(csrf).toBeTruthy();
  const headers = { Origin: 'http://localhost:5150', 'X-CSRF-Token': csrf! };
  const customer = await apiData(page, '/api/v1/customers', headers, {
    display_name: `Calendar Owner ${variant}`,
  });
  const vehicle = await apiData(page, '/api/v1/vehicles', headers, {
    customer_id: customer.id,
    make: 'Volkswagen',
    model: 'Golf',
    registration: `CAL-${variant}`,
  });
  const interventionIds: string[] = [];
  for (const start of ['08:00', '09:00', '10:00', '11:00']) {
    const intervention = await apiData(page, '/api/v1/interventions', headers, {
      vehicle_id: vehicle.id,
      service_date: `2026-07-21T${start}`,
      estimated_duration_minutes: 60,
      performed_work: 'Calendar browser fixture',
    });
    interventionIds.push(intervention.id);
  }
  await apiData(page, `/api/v1/interventions/${interventionIds[1]}/complete`, headers, null);
  await apiData(page, '/api/v1/interventions', headers, {
    vehicle_id: vehicle.id,
    service_date: '2026-07-21T23:30',
    estimated_duration_minutes: 120,
    performed_work: 'Midnight continuation fixture',
  });

  await page.goto('/calendar?view=month&date=2026-07-21');
  await expect(page).toHaveURL(/\/calendar\?view=month&date=2026-07-21$/);
  await expect(page.getByRole('heading', { name: 'Calendar', exact: true })).toBeVisible();
  await expect(page.getByRole('link', { name: 'New intervention' })).toHaveAttribute('href', '/vehicles');
  await expect(page.locator('.calendar-toolbar')).toContainText('Timezone: Europe/Brussels');
  await expect(page.locator('#calendar-region')).toContainText(`CAL-${variant}`);
  await expect(page.locator('#calendar-region')).toContainText('Draft');
  await expect(page.locator('#calendar-region')).toContainText('Completed');
  await expect(page.locator('#calendar-region')).toContainText('Continues into the next day');
  await expect(page.locator('#calendar-region')).toContainText('Continues from the previous day');

  expect(await page.locator('a[href="/calendar"][aria-current="page"]').count()).toBe(2);

  if (testInfo.project.name === 'desktop-chromium' || testInfo.project.name === 'no-javascript') {
    await expect(page.locator('.calendar-wide')).toBeVisible();
    await expect(page.locator('.calendar-month-grid > .calendar-day')).toHaveCount(35);
    const disclosure = page.getByText('Show 2 more interventions', { exact: true });
    await expect(disclosure).toBeVisible();
    await disclosure.click();
    await expect(page.locator('.calendar-overflow[open] .calendar-entry')).toHaveCount(2);
  } else {
    await expect(page.locator('.calendar-focused')).toBeVisible();
    await expect(page.locator('.calendar-date-selector > a')).toHaveCount(35);
    await expect(page.getByRole('heading', { name: /Tuesday 21 July 2026 · 5 interventions/ })).toBeVisible();
    await expect(page.locator('.calendar-selected-entries .calendar-entry')).toHaveCount(5);
  }

  await page.getByRole('link', { name: 'Next', exact: true }).click();
  await expect(page).toHaveURL(/view=month&date=2026-08-21$/);
  await expect(page.locator('.calendar-period-heading')).toContainText('August 2026');
  await page.goBack();
  await expect(page).toHaveURL(/view=month&date=2026-07-21$/);
  await expect(page.locator('#calendar-region')).toContainText(`CAL-${variant}`);

  const fragment = await page.request.get('/calendar?view=month&date=2026-07-21', {
    headers: { 'HX-Request': 'true' },
  });
  expect(fragment.status()).toBe(200);
  expect(fragment.headers()['cache-control']).toBe('no-store');
  expect(await fragment.text()).toMatch(/^<div id="calendar-region"/);

  await page.getByRole('link', { name: 'Week', exact: true }).click();
  await expect(page).toHaveURL(/view=week&date=2026-07-21$/);
  await expect(page.locator('.calendar-period-heading')).toContainText('20 July 2026–26 July 2026');
  await expect(page.locator('.calendar-time-row')).toHaveCount(96);
  if (testInfo.project.name === 'desktop-chromium' || testInfo.project.name === 'no-javascript') {
    await expect(page.locator('.calendar-week-wide')).toBeVisible();
    await expect(page.locator('.calendar-week-day-heading')).toHaveCount(7);
    await expect(page.locator('.calendar-week-time-axis .calendar-time-row').last()).toContainText('23:30');
    await expect(page.locator('.calendar-week-entry')).toHaveCount(6);
    await expect(page.locator('.calendar-week-entry').first()).toHaveAttribute('style', /--calendar-start:/);
  } else {
    await expect(page.locator('.calendar-week-focused')).toBeVisible();
    await expect(page.locator('.calendar-week-selector > a')).toHaveCount(7);
    await expect(page.getByRole('heading', { name: /Tuesday 21 July 2026 · 5 interventions/ })).toBeVisible();
    await expect(page.locator('.calendar-week-stacked-entries .calendar-entry')).toHaveCount(5);
    await expect(page.locator('.calendar-focused-timeline .calendar-time-row').last()).toContainText('23:30');
    await page.getByRole('link', { name: /Wednesday 22 July 2026, 1 intervention/ }).click();
    await expect(page).toHaveURL(/view=week&date=2026-07-22$/);
    await expect(page.locator('.calendar-week-stacked-entries .calendar-entry')).toHaveCount(1);
    await expect(page.locator('.calendar-week-stacked-entries')).toContainText('Continues from the previous day');
  }

  expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
  if (testInfo.project.name === 'desktop-chromium') {
    const accessibility = await new AxeBuilder({ page }).analyze();
    expect(accessibility.violations).toEqual([]);
    await page.setViewportSize({ width: 640, height: 900 });
    await expect(page.locator('.calendar-focused')).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
  }
});

test('@calendar invalid query and expired session have explicit recovery', async ({ page }) => {
  await signIn(page);
  const invalid = await page.goto('/calendar?view=day&date=2026-07-21');
  expect(invalid?.status()).toBe(422);
  await expect(page.getByRole('heading', { name: 'Check the Calendar link' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Open current month' })).toBeVisible();
  expect(await page.locator('a[href="/calendar"][aria-current="page"]').count()).toBe(2);

  await page.context().clearCookies();
  await page.goto('/calendar?view=month&date=2026-07-21');
  await expect(page).toHaveURL(/\/login\?next=/);
  await expect(page.getByRole('heading', { name: 'Sign in' })).toBeVisible();
});

async function signIn(page: import('@playwright/test').Page) {
  await page.goto('/login');
  await page.getByLabel('Email').fill('browser-smoke@example.invalid');
  await page.getByLabel('Password').fill('Browser smoke password 123!');
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL('/');
}

async function apiData(
  page: import('@playwright/test').Page,
  path: string,
  headers: Record<string, string>,
  data: unknown,
) {
  const response = await page.request.post(path, { headers, data });
  expect(response.ok(), `${path}: ${await response.text()}`).toBe(true);
  const payload = await response.json();
  return payload.data;
}
