import { expect, it } from 'vitest';
import { formatBytes, exceedsLimit, GITHUB_FILE_LIMIT } from './size';

it('shows empty, missing, and binary-unit sizes without confusing the GitHub limit', () => {
  expect(formatBytes(0)).toBe('0 B');
  expect(formatBytes(null)).toBe('—');
  expect(formatBytes(1024)).toBe('1 KiB');
  expect(formatBytes(1048576)).toBe('1 MiB');
  expect(formatBytes(130.7 * 1048576)).toBe('130.7 MiB');
  expect(formatBytes(1073741824)).toBe('1 GiB');
  expect(exceedsLimit(GITHUB_FILE_LIMIT)).toBe(false);
  expect(exceedsLimit(GITHUB_FILE_LIMIT + 1)).toBe(true);
});
