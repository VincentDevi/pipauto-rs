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
  await expect(page.locator(`.calendar-entry[aria-label*="CAL-${variant}"]`).first()).toHaveAttribute(
    'aria-label',
    new RegExp(`Tuesday 21 July 2026, \\d{2}:\\d{2} to \\d{2}:\\d{2}, CAL-${variant}, Volkswagen Golf`),
  );
  await expect(page.locator('.calendar-entry').first().locator('time')).toHaveCount(2);
  expect(await page.locator('[role="grid"], [role="gridcell"]').count()).toBe(0);

  expect(await page.locator('a[href="/calendar"][aria-current="page"]').count()).toBe(2);

  if (testInfo.project.name === 'desktop-chromium' || testInfo.project.name === 'no-javascript') {
    await expect(page.locator('.calendar-wide')).toBeVisible();
    await expect(page.locator('.calendar-month-grid > .calendar-day')).toHaveCount(35);
    const disclosure = page.getByText(/Show \d+ more interventions/, { exact: true }).first();
    await expect(disclosure).toBeVisible();
    await disclosure.click();
    expect(await page.locator('.calendar-overflow[open] .calendar-entry').count()).toBeGreaterThanOrEqual(2);
  } else {
    await expect(page.locator('.calendar-focused')).toBeVisible();
    await expect(page.locator('.calendar-date-selector > a')).toHaveCount(35);
    await expect(page.getByRole('heading', { name: /Tuesday 21 July 2026 · \d+ interventions/ })).toBeVisible();
    expect(await page.locator('.calendar-selected-entries .calendar-entry').count()).toBeGreaterThanOrEqual(5);
  }
  if (testInfo.project.name !== 'no-javascript') {
    expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
  }

  if (testInfo.project.name !== 'no-javascript') {
    const next = page.getByRole('link', { name: /^Next month,/ });
    let releaseRequest!: () => void;
    let requestStarted!: () => void;
    const gate = new Promise<void>((resolve) => { releaseRequest = resolve; });
    const started = new Promise<void>((resolve) => { requestStarted = resolve; });
    await page.route('**/calendar?view=month&date=2026-08-21', async (route) => {
      requestStarted();
      await gate;
      await route.continue();
    }, { times: 1 });
    const navigation = next.click();
    await started;
    await expect(page.locator('#calendar-region')).toHaveAttribute('aria-busy', 'true');
    await expect(page.locator('#calendar-region')).toContainText(`CAL-${variant}`);
    await expect(page.getByText('Loading Calendar…')).toBeVisible();
    await expect(next).not.toHaveAttribute('aria-busy', 'true');
    releaseRequest();
    await navigation;
    await expect(page.getByRole('link', { name: /^Next month,/ })).toBeFocused();
  } else {
    await page.getByRole('link', { name: /^Next month,/ }).press('Enter');
  }
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

  await page.getByRole('link', { name: /^Week view containing/ }).click();
  await expect(page).toHaveURL(/view=week&date=2026-07-21$/);
  await expect(page.locator('.calendar-period-heading')).toContainText('20 July 2026–26 July 2026');
  await expect(page.locator('.calendar-time-row')).toHaveCount(96);
  if (testInfo.project.name === 'desktop-chromium' || testInfo.project.name === 'no-javascript') {
    await expect(page.locator('.calendar-week-wide')).toBeVisible();
    await expect(page.locator('.calendar-week-day-heading')).toHaveCount(7);
    await expect(page.locator('.calendar-week-time-axis .calendar-time-row').last()).toContainText('23:30');
    expect(await page.locator('.calendar-week-entry').count()).toBeGreaterThanOrEqual(6);
    await expect(page.locator('.calendar-week-entry-position').first()).toHaveAttribute('style', /--calendar-start:/);
  } else {
    await expect(page.locator('.calendar-week-focused')).toBeVisible();
    await expect(page.locator('.calendar-week-selector > a')).toHaveCount(7);
    await expect(page.getByRole('heading', { name: /Tuesday 21 July 2026 · \d+ interventions/ })).toBeVisible();
    expect(await page.locator('.calendar-week-stacked-entries .calendar-entry').count()).toBeGreaterThanOrEqual(5);
    await expect(page.locator('.calendar-focused-timeline .calendar-time-row').last()).toContainText('23:30');
    await page.getByRole('link', {
      name: /Wednesday 22 July 2026(?:, Today)?, \d+ interventions?/,
    }).click();
    await expect(page).toHaveURL(/view=week&date=2026-07-22$/);
    expect(await page.locator('.calendar-week-stacked-entries .calendar-entry').count()).toBeGreaterThanOrEqual(1);
    await expect(page.locator('.calendar-week-stacked-entries')).toContainText('Continues from the previous day');
  }

  expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
  if (testInfo.project.name !== 'no-javascript') {
    const accessibility = await new AxeBuilder({ page }).analyze();
    expect(accessibility.violations).toEqual([]);
  }
  if (testInfo.project.name === 'desktop-chromium') {
    await page.setViewportSize({ width: 640, height: 900 });
    await expect(page.locator('.calendar-focused')).toBeVisible();
  }
  await page.evaluate(() => { document.documentElement.style.fontSize = '200%'; });
  await expect(page.locator('#calendar-region')).toContainText(`CAL-${variant}`);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
});

test('@calendar dense, empty, DST, and unavailable fixtures stay complete and accessible', async ({ page }, testInfo) => {
  const variant = `hard-${testInfo.project.name.replace('-chromium', '').replace('no-javascript', 'nojs')}`;
  await signIn(page);
  const csrf = await page.locator('meta[name="csrf-token"]').getAttribute('content');
  expect(csrf).toBeTruthy();
  const headers = { Origin: 'http://localhost:5150', 'X-CSRF-Token': csrf! };
  const customer = await apiData(page, '/api/v1/customers', headers, {
    display_name: `Dense Calendar Owner ${variant}`,
  });
  const vehicle = await apiData(page, '/api/v1/vehicles', headers, {
    customer_id: customer.id,
    make: 'Peugeot',
    model: 'Partner',
    registration: `DEN-${variant}`,
  });
  for (const start of ['09:00', '09:15', '09:30']) {
    await apiData(page, '/api/v1/interventions', headers, {
      vehicle_id: vehicle.id,
      service_date: `2026-07-23T${start}`,
      estimated_duration_minutes: 120,
      performed_work: 'Dense overlap accessibility fixture',
    });
  }
  await apiData(page, '/api/v1/interventions', headers, {
    vehicle_id: vehicle.id,
    service_date: '2026-03-29T01:30',
    estimated_duration_minutes: 120,
    performed_work: 'DST accessibility fixture',
  });

  await page.goto('/calendar?view=month&date=2026-07-23');
  await expect(page.locator('#calendar-region')).toContainText(`DEN-${variant}`);
  const firstEntry = page.locator('.calendar-entry:visible').first();
  await expect(firstEntry).toHaveJSProperty('tagName', 'ARTICLE');
  await expect(firstEntry).not.toHaveAttribute('role');
  await expect(firstEntry).not.toHaveAttribute('tabindex');
  expect(await firstEntry.locator('a, button, input, [role="button"]').count()).toBe(0);
  expect(await firstEntry.evaluate((entry) => getComputedStyle(entry).cursor)).not.toBe('pointer');
  if (testInfo.project.name !== 'no-javascript') {
    expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
  }

  await page.goto('/calendar?view=week&date=2026-07-23');
  await expect(page.locator('#calendar-region')).toContainText(`DEN-${variant}`);
  if (testInfo.project.name !== 'no-javascript') {
    expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
  }

  await page.goto('/calendar?view=month&date=2037-01-15');
  await expect(page.getByRole('heading', { name: 'No interventions scheduled this month' })).toBeVisible();
  if (testInfo.project.name !== 'no-javascript') {
    expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
  }

  await page.goto('/calendar?view=week&date=2026-03-29');
  await expect(page.locator('#calendar-region')).toContainText('01:30–04:30');
  await expect(page.locator('#calendar-region')).toContainText('2 h elapsed');

  if (testInfo.project.name === 'no-javascript') return;
  const unavailableUrl = '/calendar?view=week&date=2026-04-05';
  await page.route(`**${unavailableUrl}`, async (route) => {
    await route.fulfill({
      status: 503,
      contentType: 'text/html',
      body: `<div id="calendar-region" class="calendar-page" data-calendar-view="week">
        <header class="page-header"><div><p class="eyebrow">Workshop schedule</p><h1>Calendar</h1></div></header>
        <nav aria-label="Calendar period"><a class="button" href="${unavailableUrl}" aria-label="Try this week again">Try again</a></nav>
        <section class="panel-state" aria-labelledby="unavailable-heading"><h2 id="unavailable-heading">Calendar is temporarily unavailable</h2><p>Try this Calendar view again shortly. No intervention data was changed.</p></section>
      </div>`,
    });
  }, { times: 1 });
  await page.evaluate((path) => {
    (window as unknown as { htmx: { ajax: (method: string, url: string, options: object) => void } })
      .htmx.ajax('GET', path, { target: '#calendar-region', swap: 'outerHTML' });
  }, unavailableUrl);
  await expect(page.getByRole('heading', { name: 'Calendar is temporarily unavailable' })).toBeVisible();
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
});

test('@calendar invalid query and expired session have explicit recovery', async ({ page }, testInfo) => {
  await signIn(page);
  const invalid = await page.goto('/calendar?view=day&date=2026-07-21');
  expect(invalid?.status()).toBe(422);
  await expect(page.getByRole('heading', { name: 'Check the Calendar link' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Open current month' })).toBeVisible();
  expect(await page.locator('a[href="/calendar"][aria-current="page"]').count()).toBe(2);
  if (testInfo.project.name !== 'no-javascript') {
    expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
  }

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
