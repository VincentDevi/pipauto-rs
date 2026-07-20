import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';

test('@invoice-drafts unnumbered drafts and ordered lines work progressively', async ({ page }, testInfo) => {
  const variant = testInfo.project.name === 'desktop-chromium' ? 'desktop' : testInfo.project.name === 'no-javascript' ? 'nojs' : 'phone';
  const owner = `VIN-60 Invoice Owner ${variant}`;

  await page.goto('/login');
  await page.getByLabel('Email').fill('browser-smoke@example.invalid');
  await page.getByLabel('Password').fill('Browser smoke password 123!');
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL('/');

  await page.goto('/customers/new');
  await page.getByLabel('Name (required)').fill(owner);
  await page.getByLabel('Address line 1 (required)').fill('Workshopstraat 60');
  await page.getByLabel('Postal code (required)').fill('9000');
  await page.getByLabel('City (required)').fill('Gent');
  await page.getByLabel('Country code (required)').fill('BE');
  await page.getByRole('button', { name: 'Create customer' }).click();
  await expect(page).toHaveURL(/^http:\/\/localhost:5150\/customers\/[a-z0-9_-]+$/);
  await page.waitForLoadState('networkidle');

  await page.goto('/invoices/new');
  await expect(page.getByRole('heading', { name: 'New invoice draft' })).toBeVisible();
  await expect(page.locator('[name="number"]')).toHaveCount(0);
  await expect(page.locator('[name="due_date"]')).toHaveCount(0);
  await expect(page.getByLabel('Currency')).toHaveValue('EUR');
  await page.getByLabel('Customer (required)').selectOption({ label: owner });
  await page.getByLabel('Notes (optional)').fill('VIN-60 progressive draft');
  await page.getByRole('button', { name: 'Create draft' }).click();
  await expect(page.getByRole('heading', { name: 'Invoice draft' })).toBeVisible();
  await expect(page).toHaveURL(/^http:\/\/localhost:5150\/invoices\/(?!new$)[a-z0-9_-]+$/);
  const invoiceUrl = page.url();
  await expect(page.getByText('Unnumbered until issuance. This draft cannot receive payments.')).toBeVisible();
  await expect(page.getByText('EUR 0.00').first()).toBeVisible();
  await expect(page.getByRole('button', { name: /payment/i })).toHaveCount(0);

  for (const line of [
    { description: 'First invoice line', price: '10.01', position: '10' },
    { description: 'Second invoice line', price: '3.50', position: '20' },
  ]) {
    await page.getByRole('link', { name: 'Add line' }).click();
    await page.getByLabel('Description (required)').fill(line.description);
    await page.getByLabel('Quantity (required)').fill('1.005');
    await page.getByLabel('Unit label (required)').fill('hour');
    await page.getByLabel(/Unit price/).fill(line.price);
    await page.getByLabel('Position (required)').fill(line.position);
    await expect(page.locator('[name="currency"]')).toHaveCount(0);
    await expect(page.locator('[name="line_total"]')).toHaveCount(0);
    await page.getByRole('button', { name: 'Add line' }).click();
    await expect(page).toHaveURL(invoiceUrl);
  }

  await expect(page.getByText('EUR 10.06')).toBeVisible();
  await expect(page.getByText('EUR 3.52')).toBeVisible();
  await expect(page.getByText('EUR 13.58')).toHaveCount(2);
  await page.getByRole('button', { name: 'Move up' }).click();
  const descriptions = page.locator('#invoice-line-region tbody tr td[data-label="Description"]');
  await expect(descriptions.first()).toContainText('Second invoice line');
  await expect(page.getByText('EUR 13.58')).toHaveCount(2);

  await page.goto('/invoices?status=draft');
  await expect(page.getByText(owner).first()).toBeVisible();
  await expect(page.getByText('Draft').first()).toBeVisible();
  await expect(page.locator('[name="q"]')).toHaveCount(0);
  await expect(page.locator('[name="payment"]')).toHaveCount(0);

  if (testInfo.project.name === 'desktop-chromium') {
    const accessibility = await new AxeBuilder({ page }).analyze();
    expect(accessibility.violations).toEqual([]);
  }
});
