import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';

test('@smoke @shell login and authenticated shell work without exposing auth artifacts', async ({ page }, testInfo) => {
  await page.goto('/login');
  await expect(page.getByRole('heading', { name: 'Sign in' })).toBeVisible();

  await page.getByLabel('Email').fill('browser-smoke@example.invalid');
  await page.getByLabel('Password').fill('Browser smoke password 123!');
  await page.getByRole('button', { name: 'Sign in' }).click();

  await expect(page).toHaveURL('/');
  await expect(page.getByRole('heading', { name: 'Welcome, Browser Smoke' })).toBeVisible();
  const sidebar = page.locator('.sidebar');
  const phoneNavigation = page.locator('.phone-navigation');
  if (['phone-chromium', 'tablet-chromium'].includes(testInfo.project.name)) {
    await expect(sidebar).toBeHidden();
    await expect(phoneNavigation).toBeVisible();
    await expect(phoneNavigation.getByRole('link', { name: 'Home' })).toHaveAttribute('aria-current', 'page');
    await phoneNavigation.locator('summary').click();
    await expect(page.locator('.sheet').getByRole('link', { name: 'Customers' })).toBeVisible();
    await expect(page.locator('.sheet').getByText('Browser Smoke')).toBeVisible();
  } else {
    await expect(sidebar).toBeVisible();
    await expect(phoneNavigation).toBeHidden();
    await expect(sidebar.getByText('Browser Smoke')).toBeVisible();
    await expect(sidebar.getByRole('link', { name: 'Dashboard', exact: true })).toHaveAttribute('aria-current', 'page');
  }

  await page.getByRole('link', { name: 'Skip to content' }).focus();
  await expect(page.getByRole('link', { name: 'Skip to content' })).toBeFocused();

  if (testInfo.project.name === 'desktop-chromium') {
    const accessibility = await new AxeBuilder({ page }).analyze();
    expect(accessibility.violations).toEqual([]);
  }
});
