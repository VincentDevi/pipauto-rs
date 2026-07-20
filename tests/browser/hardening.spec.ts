import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

async function signIn(page: Page) {
  await page.goto('/login');
  await page.getByLabel('Email').fill('browser-smoke@example.invalid');
  await page.getByLabel('Password').fill('Browser smoke password 123!');
  await page.getByLabel('Password').press('Enter');
  await expect(page).toHaveURL('/');
}

async function expectNoHorizontalPageScroll(page: Page) {
  const overflow = await page.evaluate(() => ({
    clientWidth: document.documentElement.clientWidth,
    scrollWidth: document.documentElement.scrollWidth,
  }));
  expect(overflow.scrollWidth).toBeLessThanOrEqual(overflow.clientWidth + 1);
}

test('@hardening shell is accessible, keyboard usable, responsive, and motion safe', async ({ page }, testInfo) => {
  await signIn(page);
  await expectNoHorizontalPageScroll(page);

  if (testInfo.project.name !== 'no-javascript') {
    const accessibility = await new AxeBuilder({ page })
      .options({ runOnly: { type: 'tag', values: ['wcag2a', 'wcag2aa'] } })
      .analyze();
    expect(
      accessibility.violations.filter(
        ({ impact }) => impact === 'serious' || impact === 'critical',
      ),
    ).toEqual([]);
  }

  await page.emulateMedia({ reducedMotion: 'reduce' });
  const animationDuration = await page.evaluate(() => {
    const indicator = document.createElement('span');
    indicator.className = 'loading-indicator';
    document.body.append(indicator);
    const duration = getComputedStyle(indicator, '::before').animationDuration;
    indicator.remove();
    return duration;
  });
  expect(Number.parseFloat(animationDuration)).toBeLessThanOrEqual(0.01);

  if (testInfo.project.name === 'desktop-chromium') {
    const viewport = page.viewportSize();
    await page.setViewportSize({
      width: Math.floor((viewport?.width ?? 1280) / 2),
      height: viewport?.height ?? 720,
    });
    await page.evaluate(() => { document.documentElement.style.fontSize = '200%'; });
    await expect(page.locator('.sidebar')).toBeHidden();
    await expect(page.locator('.phone-navigation')).toBeVisible();
    await expectNoHorizontalPageScroll(page);
  }
});

test('@hardening HTMX failures recover controls and expired sessions leave private pages', async ({ page }, testInfo) => {
  await signIn(page);

  if (testInfo.project.name === 'no-javascript') {
    await page.context().clearCookies();
    await page.goto('/customers');
    await expect(page).toHaveURL(/\/login\?next=/);
    await expect(page.getByRole('heading', { name: 'Sign in' })).toBeVisible();
    return;
  }

  await page.goto('/customers');
  await page.getByLabel('Search customers').fill('focus restoration');
  const filter = page.getByRole('button', { name: 'Apply filters' });
  await filter.click();
  await expect(filter).toBeFocused();
  await expect(page).toHaveURL(/q=focus(\+|%20)restoration/);
  await expect(page).toHaveTitle('Customers · Pipauto');

  await page.goto('/customers/new');
  await page.getByLabel('Name (required)').fill('Network recovery customer');
  await page.getByLabel('Address line 1 (required)').fill('Recovery street 62');
  await page.getByLabel('Postal code (required)').fill('9000');
  await page.getByLabel('City (required)').fill('Gent');
  await page.getByLabel('Country code (required)').fill('BE');
  await page.route('**/customers', async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 250));
    await route.abort('failed');
  });
  const submit = page.getByRole('button', { name: 'Create customer' });
  await submit.click();
  await expect(submit).toBeDisabled();
  await expect(submit).toBeEnabled();
  await expect(page.getByRole('alert')).toContainText('Reload the latest workshop record');
  await page.unroute('**/customers');

  await page.context().clearCookies();
  await page.evaluate(() => (window as unknown as { htmx: { ajax: (method: string, path: string, options: object) => void } }).htmx.ajax('GET', '/customers', {
    target: '#main-content',
  }));
  await expect(page).toHaveURL(/\/login\?next=/);
  await expect(page.getByRole('heading', { name: 'Sign in' })).toBeVisible();
  await expect(page.locator('#main-content')).not.toContainText('Network recovery customer');
});
