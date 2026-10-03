import { describe, expect, it } from 'vitest';
import { displayPath } from './paths';

describe('Windows folder display', () => {
  it('formats drive and UNC paths, including spaces, Unicode, and long names', () => {
    const path = 'C:\\hard drive\\Scripts\\ไทย\\' + 'long-folder\\'.repeat(40);
    expect(displayPath('\\\\?\\' + path)).toBe(path);
    expect(displayPath('\\\\?\\UNC\\server\\share\\my folder')).toBe(
      '\\\\server\\share\\my folder',
    );
    expect(displayPath('\\\\?\\unc\\server\\share')).toBe('\\\\server\\share');
  });
  it('preserves normal paths and unknown device namespaces', () => {
    for (const path of [
      '',
      'C:\\Projects',
      '\\\\server\\share',
      '/Users/example',
      '\\\\?\\Volume{123}\\',
      '\\\\.\\device',
    ]) {
      expect(displayPath(path)).toBe(path);
    }
  });
});
