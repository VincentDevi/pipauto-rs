import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';

test('@knowledge technical notes search, context, safe text, tags, and lifecycle work progressively', async ({ page }, testInfo) => {
  const variant = testInfo.project.name === 'no-javascript'
    ? 'nojs'
    : testInfo.project.name.replace('-chromium', '');
  const registration = `VIN-59-${variant}`;

  await page.goto('/login');
  await page.getByLabel('Email').fill('browser-smoke@example.invalid');
  await page.getByLabel('Password').fill('Browser smoke password 123!');
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL('/');

  await page.goto('/customers/new');
  await page.getByLabel('Name (required)').fill(`Knowledge Owner ${variant}`);
  await page.getByLabel('Address line 1 (required)').fill('Workshopstraat 59');
  await page.getByLabel('Postal code (required)').fill('9000');
  await page.getByLabel('City (required)').fill('Gent');
  await page.getByLabel('Country code (required)').fill('BE');
  await page.getByRole('button', { name: 'Create customer' }).click();
  await page.getByRole('link', { name: 'Add vehicle' }).first().click();
  await page.getByLabel('Make (required)').fill('Volkswagen');
  await page.getByLabel('Model (required)').fill('Golf');
  await page.getByLabel('Display registration').fill(registration);
  await page.getByLabel('Engine').fill('1.4 TSI');
  await page.getByRole('button', { name: 'Save vehicle' }).click();
  await expect(page).toHaveURL(/\/vehicles\/[a-z0-9_-]+$/);

  await page.getByRole('link', { name: 'Create technical note' }).click();
  await expect(page.getByRole('heading', { name: 'New technical note' })).toBeVisible();
  await expect(page.getByLabel('Make')).toHaveValue('Volkswagen');
  await expect(page.getByLabel('Model')).toHaveValue('Golf');
  await expect(page.getByLabel('Engine')).toHaveValue('1.4 TSI');
  await page.getByLabel('Title (required)').fill(`Golf water pump ${variant}`);
  await page.getByLabel('Body (required)').fill('Lock the crankshaft first.\n<script>alert("unsafe")</script>');
  await page.getByLabel('Ordered tag list').fill(' Cooling \nVW\nvw\nprocedure');
  await page.getByRole('button', { name: 'Save technical note' }).click();
  await expect(page).toHaveURL(/\/knowledge\/[a-z0-9_-]+$/);
  await expect(page.getByText('<script>alert("unsafe")</script>')).toBeVisible();
  await expect(page.locator('script').filter({ hasText: 'unsafe' })).toHaveCount(0);
  await expect(page.getByText('cooling', { exact: true })).toBeVisible();
  await expect(page.getByText('vw', { exact: true })).toBeVisible();
  await expect(page.getByText('procedure', { exact: true })).toBeVisible();

  const detailUrl = page.url();
  await page.getByRole('link', { name: 'Edit technical note' }).click();
  if (testInfo.project.name === 'no-javascript') {
    await page.getByLabel('Ordered tag list').fill('cooling\nprocedure');
  } else {
    await page.getByRole('button', { name: 'Remove vw' }).click();
    await expect(page.getByLabel('Ordered tag list')).toHaveValue('cooling\nprocedure');
  }
  await page.getByRole('button', { name: 'Save changes' }).click();
  await expect(page).toHaveURL(detailUrl);
  await expect(page.getByText('vw', { exact: true })).toHaveCount(0);

  await page.goto(`/knowledge?q=water&tags=cooling%0Aprocedure&make=volkswagen&model=GOLF&engine=1.4%20tsi`);
  await expect(page.getByText(`Golf water pump ${variant}`, { exact: true })).toBeVisible();
  await expect(page.getByText(/Server relevance and tie-break ordering|Updated/).first()).toBeVisible();

  await page.goto('/knowledge?q=no-such-workshop-note');
  await expect(page.getByRole('heading', { name: 'No technical notes match' })).toBeVisible();
  await page.getByRole('link', { name: 'Clear filters' }).first().click();
  await expect(page.getByText(`Golf water pump ${variant}`, { exact: true })).toBeVisible();

  await page.goto(detailUrl);
  await page.locator('.archive-customer > summary').click();
  await expect(page.getByText(/leave default knowledge search/)).toBeVisible();
  await page.getByRole('button', { name: 'Confirm archive' }).click();
  await expect(page.getByRole('heading', { name: 'Archived technical note' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Edit technical note' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Restore technical note' }).click();
  await expect(page.getByRole('link', { name: 'Edit technical note' })).toBeVisible();

  if (testInfo.project.name === 'phone-chromium') {
    await expect(page.locator('.phone-navigation')).toBeVisible();
  } else if (testInfo.project.name === 'desktop-chromium') {
    const accessibility = await new AxeBuilder({ page }).analyze();
    expect(accessibility.violations).toEqual([]);
  }
});
