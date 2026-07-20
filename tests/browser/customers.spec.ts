import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';

test('@customers customer workflows support standard forms, HTMX, and responsive records', async ({ page }, testInfo) => {
  await page.goto('/login');
  await page.getByLabel('Email').fill('browser-smoke@example.invalid');
  await page.getByLabel('Password').fill('Browser smoke password 123!');
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL('/');

  await page.goto('/customers');
  await expect(page.getByRole('heading', { name: 'Customers', exact: true })).toBeVisible();
  await page.getByRole('link', { name: 'New customer' }).first().click();

  await page.getByLabel('Name (required)').fill('Customer Browser Workflow');
  await page.getByLabel('Email').fill('Customer.Display@Example.COM');
  await page.getByLabel('Phone').fill('+32 (0) 475 12 34 56');
  await page.getByLabel('Address line 1 (required)').fill('Workshopstraat 55');
  await page.getByLabel('Address line 2').fill('Unit B');
  await page.getByLabel('Postal code (required)').fill('9000');
  await page.getByLabel('City (required)').fill('Gent');
  await page.getByLabel('Country code (required)').fill('be');
  await page.getByLabel('Workshop notes').fill('Display values must remain intact.');
  await page.getByRole('button', { name: 'Create customer' }).click();

  await expect(page.getByText('Use a two-letter uppercase country code.').first()).toBeVisible();
  await expect(page.getByLabel('Email')).toHaveValue('Customer.Display@Example.COM');
  await expect(page.getByLabel('Workshop notes')).toHaveValue('Display values must remain intact.');

  await page.getByLabel('Country code (required)').fill('BE');
  await page.getByRole('button', { name: 'Create customer' }).click();
  await expect(page).toHaveURL(/\/customers\/[a-z0-9_-]+$/);
  await expect(page.getByRole('heading', { name: 'Customer Browser Workflow' })).toBeVisible();
  await expect(page.getByText('Customer.Display@Example.COM')).toBeVisible();
  await expect(page.getByText('Workshopstraat 55')).toBeVisible();
  await expect(page.getByRole('link', { name: 'Add vehicle' }).first()).toHaveAttribute(
    'href',
    /\/customers\/[a-z0-9_-]+\/vehicles\/new/,
  );

  await page.getByRole('link', { name: 'Edit customer' }).click();
  await page.getByLabel('Name (required)').fill('Customer Browser Updated');
  await page.getByRole('button', { name: 'Save changes' }).click();
  await expect(page.getByRole('heading', { name: 'Customer Browser Updated' })).toBeVisible();

  await page.locator('.archive-customer > summary').click();
  await expect(page.getByText('Existing vehicles and service history remain.')).toBeVisible();
  await page.getByRole('button', { name: 'Confirm archive' }).click();
  await expect(page.getByRole('heading', { name: 'Archived customer' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Restore customer' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Edit customer' })).toHaveCount(0);
  await expect(page.getByRole('link', { name: 'Add vehicle' })).toHaveCount(0);

  await page.getByRole('button', { name: 'Restore customer' }).click();
  await expect(page.getByRole('link', { name: 'Edit customer' })).toBeVisible();

  await page.goto('/customers');
  await page.getByLabel('Search customers').fill('Customer Browser Updated');
  await page.getByRole('button', { name: 'Apply filters' }).click();
  await expect(page).toHaveURL(/q=Customer(\+|%20)Browser(\+|%20)Updated/);

  if (testInfo.project.name === 'phone-chromium') {
    await expect(page.locator('.customer-card-list')).toBeVisible();
    await expect(page.locator('.customer-table-wrap')).toBeHidden();
    await expect(
      page.locator('.customer-card:visible').getByText('Customer Browser Updated').first(),
    ).toBeVisible();
  } else {
    await expect(page.locator('.customer-table-wrap')).toBeVisible();
    await expect(page.locator('.customer-card-list')).toBeHidden();
    await expect(
      page.locator('.customer-table-wrap').getByText('Customer Browser Updated').first(),
    ).toBeVisible();
  }

  await page.getByLabel('Search customers').fill('No customer can match this');
  await page.getByRole('button', { name: 'Apply filters' }).click();
  await expect(page).toHaveURL(/q=No(\+|%20)customer(\+|%20)can(\+|%20)match(\+|%20)this/);
  await expect(page.getByRole('heading', { name: 'No customers match these filters' })).toBeVisible();
  await expect(
    page.getByRole('heading', { name: 'No customers match these filters' })
      .locator('..')
      .getByRole('link', { name: 'Clear filters' }),
  ).toBeVisible();

  if (testInfo.project.name === 'desktop-chromium') {
    const accessibility = await new AxeBuilder({ page }).analyze();
    expect(accessibility.violations).toEqual([]);
  }
});
