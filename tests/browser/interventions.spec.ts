import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';

test('@interventions draft, lifecycle, filtering, and locked history work progressively', async ({ page }, testInfo) => {
  const variant = testInfo.project.name === 'desktop-chromium' ? '1' : testInfo.project.name === 'no-javascript' ? '2' : '3';
  const registration = `VIN-57-${variant}`;

  await page.goto('/login');
  await page.getByLabel('Email').fill('browser-smoke@example.invalid');
  await page.getByLabel('Password').fill('Browser smoke password 123!');
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL('/');

  await page.goto('/customers/new');
  await page.getByLabel('Name (required)').fill(`Intervention Owner ${variant}`);
  await page.getByLabel('Address line 1 (required)').fill('Workshopstraat 57');
  await page.getByLabel('Postal code (required)').fill('9000');
  await page.getByLabel('City (required)').fill('Gent');
  await page.getByLabel('Country code (required)').fill('BE');
  await page.getByRole('button', { name: 'Create customer' }).click();
  await page.getByRole('link', { name: 'Add vehicle' }).first().click();
  await page.getByLabel('Make (required)').fill('Fiat');
  await page.getByLabel('Model (required)').fill('Panda');
  await page.getByLabel('Display registration').fill(registration);
  await page.getByLabel('Current mileage').fill('88200');
  await page.getByRole('button', { name: 'Save vehicle' }).click();
  await expect(page).toHaveURL(/\/vehicles\/[a-z0-9_-]+$/);

  await page.getByRole('link', { name: 'New intervention' }).first().click();
  await expect(page.getByRole('heading', { name: 'New intervention' })).toBeVisible();
  await page.getByLabel('Service date').fill('2026-07-17');
  await page.getByLabel('Recorded mileage').fill('88200');
  await page.getByLabel('Customer-reported problem').fill('Annual service requested');
  await page.getByLabel('Diagnostics').fill('Oil due for replacement');
  await page.getByLabel('Work performed').fill('Changed engine oil and filter');
  await page.getByLabel('Recommendations').fill('Inspect brakes next visit');
  await page.getByLabel('General notes').fill('Exact browser workflow note');
  await page.getByRole('button', { name: 'Save draft' }).click();
  await expect(page).toHaveURL(/^http:\/\/localhost:5150\/interventions\/[a-z0-9_-]+$/);
  await expect(page.getByText('Annual service requested')).toBeVisible();
  await expect(page.getByText('Changed engine oil and filter').first()).toBeVisible();
  await expect(page.getByText('EUR 0.00').first()).toBeVisible();

  const interventionUrl = page.url();
  await page.getByRole('link', { name: 'Complete intervention' }).click();
  await expect(page.getByRole('button', { name: 'Complete and lock intervention' })).toBeVisible();
  await expect(page.getByText(/cannot be undone/)).toBeVisible();
  await page.getByRole('button', { name: 'Complete and lock intervention' }).click();
  await expect(page).toHaveURL(interventionUrl);
  await expect(page.getByText('Completed').first()).toBeVisible();
  await expect(page.getByRole('link', { name: 'Edit details' })).toHaveCount(0);
  await expect(page.getByRole('link', { name: 'Add line item' })).toHaveCount(0);
  await expect(page.getByRole('link', { name: 'Create invoice draft' })).toBeVisible();

  await page.goto(`${interventionUrl}/edit`);
  await expect(page.getByText(/authoritative read-only state/)).toBeVisible();
  await expect(page.getByRole('button', { name: 'Save changes' })).toHaveCount(0);

  await page.goto(`/interventions?status=completed&from=2026-07-01&to=2026-07-31`);
  await expect(page.getByText('Changed engine oil and filter').first()).toBeVisible();
  await expect(page.locator('input[name="q"]')).toHaveCount(0);
  await expect(page.locator('[name="customer"]')).toHaveCount(0);

  if (testInfo.project.name === 'desktop-chromium') {
    const accessibility = await new AxeBuilder({ page }).analyze();
    expect(accessibility.violations).toEqual([]);
  }
});

test('@intervention-lines @attachment-metadata ordered lines and metadata-only attachments work progressively', async ({ page }, testInfo) => {
  const variant = testInfo.project.name === 'desktop-chromium' ? 'desktop' : testInfo.project.name === 'no-javascript' ? 'nojs' : 'mobile';
  const registration = `VIN-58-${variant}`;

  await page.goto('/login');
  await page.getByLabel('Email').fill('browser-smoke@example.invalid');
  await page.getByLabel('Password').fill('Browser smoke password 123!');
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL('/');
  await page.goto('/customers/new');
  await page.getByLabel('Name (required)').fill(`VIN-58 Owner ${variant}`);
  await page.getByLabel('Address line 1 (required)').fill('Workshopstraat 58');
  await page.getByLabel('Postal code (required)').fill('9000');
  await page.getByLabel('City (required)').fill('Gent');
  await page.getByLabel('Country code (required)').fill('BE');
  await page.getByRole('button', { name: 'Create customer' }).click();
  await page.getByRole('link', { name: 'Add vehicle' }).first().click();
  await page.getByLabel('Make (required)').fill('Volvo');
  await page.getByLabel('Model (required)').fill('V60');
  await page.getByLabel('Display registration').fill(registration);
  await page.getByRole('button', { name: 'Save vehicle' }).click();
  await page.getByRole('link', { name: 'New intervention' }).first().click();
  await page.getByLabel('Service date').fill('2026-07-20');
  await page.getByLabel('Work performed').fill('Verified ordered lines and metadata');
  await page.getByRole('button', { name: 'Save draft' }).click();
  await expect(page).toHaveURL(/^http:\/\/localhost:5150\/interventions\/[a-z0-9_-]+$/);
  const interventionUrl = page.url();
  for (const line of [
    { description: 'First line', price: '10.01', position: '10' },
    { description: 'Second line', price: '3.50', position: '20' },
  ]) {
    await page.getByRole('link', { name: 'Add line item' }).click();
    await page.getByLabel('Category (required)').selectOption('labour');
    await page.getByLabel('Description (required)').fill(line.description);
    await page.getByLabel('Quantity (required)').fill('1.005');
    await page.getByLabel('Unit label (required)').fill('hour');
    await page.getByLabel(/Unit price/).fill(line.price);
    await page.getByLabel('Position (required)').fill(line.position);
    await page.getByRole('button', { name: 'Add line item' }).click();
    await expect(page).toHaveURL(interventionUrl);
  }
  await expect(page.getByText('EUR 10.06')).toBeVisible();
  await page.getByRole('button', { name: 'Move up' }).click();
  const descriptions = page.locator('#line-region tbody tr td[data-label="Description"]');
  await expect(descriptions.first()).toContainText('Second line');

  await page.getByRole('link', { name: 'Add attachment metadata' }).click();
  await expect(page.getByText('Metadata only — no file has been uploaded.')).toBeVisible();
  await expect(page.locator('input[type="file"]')).toHaveCount(0);
  await page.getByLabel('Display name (required)').fill('Brake inspection photo');
  await page.getByLabel('Content type (required)').selectOption('image/jpeg');
  await page.getByLabel('Byte size').fill('12345');
  await page.getByLabel('Caption').fill('Metadata record without binary storage');
  await page.getByRole('button', { name: 'Add metadata' }).click();
  await expect(page).toHaveURL(interventionUrl);
  await expect(page.getByText('Brake inspection photo', { exact: true })).toBeVisible();
  await expect(page.getByText(/METADATA ONLY/).last()).toBeVisible();

  await page.getByRole('link', { name: 'Complete intervention' }).click();
  await page.getByRole('button', { name: 'Complete and lock intervention' }).click();
  await expect(page.getByRole('link', { name: 'Add line item' })).toHaveCount(0);
  await expect(page.getByRole('link', { name: 'Add attachment metadata' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Move up' })).toHaveCount(0);
  await expect(page.getByText('Brake inspection photo', { exact: true })).toBeVisible();

  if (testInfo.project.name === 'desktop-chromium') {
    const accessibility = await new AxeBuilder({ page }).analyze();
    expect(accessibility.violations).toEqual([]);
  }
});
