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
  await expect(
    page.getByRole('button', { name: 'Quit', exact: true }),
  ).toHaveCount(0);
  await page.setViewportSize({ width: 1120, height: 640 });
  await page
    .getByRole('button', { name: 'Difference checker', exact: true })
    .click();
  await expect(
    page.getByRole('region', { name: 'Difference checker', exact: true }),
  ).toBeVisible();
  await expect(page.locator('.hljs-keyword').first()).toBeVisible();
  await expect(page.locator('.code-row.changed').first()).toBeVisible();
  const comparison = await page.locator('.difference-checker').boundingBox();
  const flyout = await page.locator('.flyout').boundingBox();
  expect(comparison!.x + comparison!.width).toBeLessThanOrEqual(flyout!.x);
  await page.screenshot({ path: 'docs/difference-checker.png' });
  await page.getByRole('button', { name: /cover.png/ }).click();
  await expect(
    page.getByRole('heading', { name: 'Binary file' }),
  ).toBeVisible();
  await page.getByRole('button', { name: /old-notes.txt/ }).click();
  await expect(
    page.getByText('Missing locally. The cloud file is shown on the right.'),
  ).toBeVisible();
  await page.getByRole('button', { name: /ideas.md/ }).click();
  await expect(
    page.getByText('Not in the cloud. The local file is shown on the left.'),
  ).toBeVisible();
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
  await page.getByRole('button', { name: /Repository.*field-notes/ }).click();
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
  await page.getByText('Ignored files & folders', { exact: true }).click();
  await page
    .getByRole('textbox', { name: 'Local folder', exact: true })
    .fill('C:\\Projects\\new-project');
  await page
    .getByRole('checkbox', { name: 'Ignore .env', exact: true })
    .check();
  await expect(page.getByLabel('Ignore patterns')).toHaveValue('/.env');
});
test('ignore settings support folder browsing, presets, and safe save', async ({
  page,
}) => {
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page
    .getByText('Repository settings · ignored files', { exact: true })
    .click();
  await page
    .getByRole('checkbox', { name: 'Ignore node_modules', exact: true })
    .check();
  await expect(page.getByLabel('Ignore patterns')).toHaveValue(
    '/node_modules/',
  );
  await page.getByRole('button', { name: 'Secrets', exact: true }).click();
  await expect(page.getByLabel('Ignore patterns')).toHaveValue(/\.env/);
  await page.getByRole('button', { name: 'src/', exact: true }).click();
  await expect(
    page.getByRole('checkbox', { name: 'Ignore src/sync.ts', exact: true }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Up one folder' }).click();
  await page.getByRole('button', { name: 'Save ignore rules' }).click();
  await expect(
    page.getByText('Ignore rules saved.', { exact: true }),
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
