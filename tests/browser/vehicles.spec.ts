import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';

test('@vehicles @service-history vehicle registration, metadata, lifecycle, and history work progressively', async ({ page }, testInfo) => {
  const variant = testInfo.project.name === 'desktop-chromium' ? '1' : testInfo.project.name === 'no-javascript' ? '2' : '3';
  const registration = `VIN-56-${variant}`;
  const vin = `WVWZZZ1JZXW00000${variant}`;

  await page.goto('/login');
  await page.getByLabel('Email').fill('browser-smoke@example.invalid');
  await page.getByLabel('Password').fill('Browser smoke password 123!');
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL('/');

  await page.goto('/customers/new');
  await page.getByLabel('Name (required)').fill(`Vehicle Owner ${variant}`);
  await page.getByLabel('Address line 1 (required)').fill('Workshopstraat 56');
  await page.getByLabel('Postal code (required)').fill('9000');
  await page.getByLabel('City (required)').fill('Gent');
  await page.getByLabel('Country code (required)').fill('BE');
  await page.getByRole('button', { name: 'Create customer' }).click();
  await expect(page).toHaveURL(/\/customers\/[a-z0-9_-]+$/);

  await page.getByRole('link', { name: 'Add vehicle' }).first().click();
  await page.getByLabel('Make (required)').fill('Volkswagen');
  await page.getByLabel('Model (required)').fill('Golf');
  await page.getByLabel('Year').fill('2020');
  await page.getByLabel('Display registration').fill(registration.toLowerCase());
  await page.getByLabel('VIN').fill('WVWZZZ1JZXW00000I');
  await page.getByLabel('Current mileage').fill('126400');
  await page.getByLabel('Engine').fill('2.0 TDI');
  await page.getByLabel('Workshop notes').fill('Preserve the displayed registration and VIN.');
  await page.getByRole('button', { name: 'Save vehicle' }).click();
  await expect(page.getByText('Enter a 17-character VIN without I, O, or Q.').first()).toBeVisible();
  await expect(page.getByLabel('Display registration')).toHaveValue(registration.toLowerCase());

  await page.getByLabel('VIN').fill(vin.toLowerCase());
  await page.getByRole('button', { name: 'Save vehicle' }).click();
  await expect(page).toHaveURL(/\/vehicles\/[a-z0-9_-]+$/);
  await expect(page.getByText(registration.toLowerCase()).first()).toBeVisible();
  await expect(page.getByText(vin.toLowerCase()).first()).toBeVisible();
  await expect(page.getByText('Metadata only — no files have been uploaded.')).toBeVisible();

  const vehicleUrl = page.url();
  await page.getByRole('link', { name: 'Add attachment metadata' }).click();
  await expect(page.getByText('Metadata only — no file has been uploaded.')).toBeVisible();
  await expect(page.locator('input[type="file"]')).toHaveCount(0);
  await page.getByLabel('Display name (required)').fill(`Inspection ${variant}.jpg`);
  await page.getByLabel('Content type (required)').selectOption('image/jpeg');
  await page.getByLabel('Byte size').fill('24512');
  await page.getByLabel('Caption').fill('Before repair');
  await page.getByRole('button', { name: 'Add metadata' }).click();
  await expect(page).toHaveURL(vehicleUrl);
  await expect(page.getByText(`Inspection ${variant}.jpg`, { exact: true })).toBeVisible();
  await expect(page.getByText(/METADATA ONLY/).first()).toBeVisible();

  await page.getByRole('link', { name: 'View complete history' }).click();
  await expect(page.getByRole('heading', { name: 'Complete service history' })).toBeVisible();
  await expect(page.getByText(/Authoritative server order/)).toBeVisible();
  await page.getByLabel('Lifecycle').selectOption('cancelled');
  await page.getByRole('button', { name: 'Filter history' }).click();
  await expect(page.getByRole('heading', { name: 'No service history found' })).toBeVisible();
  await page.getByRole('link', { name: 'Back to vehicle' }).click();

  await page.locator('.archive-customer > summary').click();
  await expect(page.getByText('New interventions, invoices, and attachment metadata cannot be added while archived.').first()).toBeVisible();
  await page.getByRole('button', { name: 'Confirm archive' }).click();
  await expect(page.getByRole('heading', { name: 'Archived vehicle' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Add attachment metadata' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Restore vehicle' }).click();
  await expect(page.getByRole('link', { name: 'Add attachment metadata' })).toBeVisible();

  await page.goto(`/vehicles?registration=${registration.toLowerCase()}`);

  if (testInfo.project.name === 'phone-chromium') {
    await expect(page.locator('.customer-card-list')).toBeVisible();
    await expect(page.locator('.customer-table-wrap')).toBeHidden();
    await expect(
      page.locator('.customer-card-list').getByText(registration.toLowerCase()).first(),
    ).toBeVisible();
  } else if (testInfo.project.name === 'desktop-chromium') {
    await expect(
      page.locator('.customer-table-wrap').getByText(registration.toLowerCase()).first(),
    ).toBeVisible();
    const accessibility = await new AxeBuilder({ page }).analyze();
    expect(accessibility.violations).toEqual([]);
  } else {
    await expect(page.getByText(registration.toLowerCase()).first()).toBeVisible();
  }
});
