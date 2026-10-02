import { test, expect } from '@playwright/test';
test.beforeEach(async ({ page }) => {
  await page.goto('/?preview');
  await expect(
    page.getByRole('button', { name: 'Sync Now', exact: true }),
  ).toBeVisible();
});
test('compact flyout, pending diff, history, and sync feedback', async ({
  page,
}) => {
  await expect(
    page.getByRole('heading', { name: 'lazysync', exact: true }),
  ).toBeVisible();
  await expect(page.getByText('Ready to sync', { exact: true })).toBeVisible();
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= 340),
  ).toBe(true);
  const toggle = await page
    .getByRole('switch', { name: 'Automatic sync' })
    .boundingBox();
  const area = await page.locator('.content').boundingBox();
  expect(toggle!.y + toggle!.height).toBeLessThanOrEqual(
    area!.y + area!.height,
  );
  await page.getByRole('button', { name: 'View pending changes' }).click();
  await expect(
    page.getByRole('dialog', { name: 'Pending changes' }),
  ).toBeVisible();
  await page.getByText('cover.png', { exact: false }).click();
  await expect(page.getByText('Binary file · no text preview')).toBeVisible();
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'History', exact: true }).click();
  await page.getByRole('button', { name: /Add research notes/ }).click();
  await expect(page.getByText('a18cf73', { exact: false })).not.toBeVisible();
  await expect(page.getByText('notes/ideas.md', { exact: true })).toBeVisible();
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Sync Now', exact: true }).click();
  await expect(
    page.getByText('All changes synced', { exact: true }),
  ).toBeVisible();
});
test('picker search, write permissions, private creation form', async ({
  page,
}) => {
  await page.getByRole('button', { name: /REPOSITORY/ }).click();
  await expect(
    page.getByRole('dialog', { name: 'Repositories' }),
  ).toBeVisible();
  await expect(
    page.getByRole('button', { name: /team\/design-system/ }),
  ).toBeDisabled();
  await page
    .getByRole('textbox', { name: 'Search loaded repositories' })
    .fill('dotfiles');
  await expect(
    page.getByRole('button', { name: /morgan\/dotfiles/ }),
  ).toBeVisible();
  await expect(
    page.getByRole('button', { name: /team\/design-system/ }),
  ).toHaveCount(0);
  await page
    .getByRole('button', { name: 'Create private repository', exact: true })
    .click();
  await expect(
    page.getByRole('dialog', { name: 'Create private repository' }),
  ).toBeVisible();
  await expect(page.getByLabel('Repository name')).toBeVisible();
  await expect(
    page.getByRole('textbox', { name: 'Local folder', exact: true }),
  ).toBeVisible();
});
test('settings, automation, and focus are keyboard accessible', async ({
  page,
}) => {
  await page.getByRole('switch', { name: 'Automatic sync' }).click();
  await expect(
    page.getByRole('switch', { name: 'Automatic sync' }),
  ).toHaveAttribute('aria-checked', 'true');
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByLabel('Device name').fill('Laptop');
  await page
    .getByRole('combobox', { name: 'Automatic sync', exact: true })
    .selectOption('folderIdle');
  await page
    .getByRole('button', { name: 'Save settings', exact: true })
    .click();
  await expect(page.getByText('Laptop', { exact: true })).toBeVisible();
  await expect(page.getByText('On folder idle', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.keyboard.press('Escape');
  await expect(
    page.getByRole('button', { name: 'Settings', exact: true }),
  ).toBeFocused();
});
