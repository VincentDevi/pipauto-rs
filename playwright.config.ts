import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests/browser',
  fullyParallel: false,
  // Browser projects share one disposable database and one deliberately expensive login account.
  workers: 1,
  forbidOnly: true,
  retries: 0,
  reporter: 'list',
  use: {
    baseURL: 'http://localhost:5150',
    screenshot: 'off',
    trace: 'off',
    video: 'off',
  },
  webServer: {
    command: './scripts/browser-smoke-server',
    url: 'http://localhost:5150/login',
    reuseExistingServer: false,
    timeout: 180_000,
  },
  projects: [
    {
      name: 'desktop-chromium',
      use: { ...devices['Desktop Chrome'] },
    },
    {
      name: 'tablet-chromium',
      use: { ...devices['iPad Mini'], browserName: 'chromium' },
    },
    {
      name: 'no-javascript',
      use: { ...devices['Desktop Chrome'], javaScriptEnabled: false },
    },
    {
      name: 'phone-chromium',
      use: { ...devices['Pixel 5'] },
    },
  ],
});
