import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests/browser',
  fullyParallel: false,
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
      name: 'no-javascript',
      use: { ...devices['Desktop Chrome'], javaScriptEnabled: false },
    },
    {
      name: 'phone-chromium',
      use: { ...devices['Pixel 5'] },
    },
  ],
});
