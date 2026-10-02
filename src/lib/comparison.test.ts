import { describe, it, expect } from 'vitest';
import {
  comparisonRows,
  highlightLines,
  languageFor,
  ignorePattern,
} from './comparison';
describe('file comparison', () => {
  it('aligns replacements, added lines, and missing versions', () => {
    expect(comparisonRows('same\nold\n', 'same\nnew\nextra\n')).toEqual([
      { cloud: 0, local: 0, changed: false },
      { cloud: 1, local: 1, changed: true },
      { cloud: null, local: 2, changed: true },
    ]);
    expect(comparisonRows('cloud\n', '')).toEqual([
      { cloud: 0, local: null, changed: true },
    ]);
  });
  it('escapes source HTML and preserves multiline syntax scopes', () => {
    const lines = highlightLines(
      '/* one\ntwo */\nconst text = "<img onerror=alert(1)>";',
      'javascript',
    );
    expect(lines[0]).toContain('hljs-comment');
    expect(lines[1]).toContain('hljs-comment');
    for (const line of lines)
      expect((line.match(/<span\b/g) ?? []).length).toBe(
        (line.match(/<\/span>/g) ?? []).length,
      );
    expect(lines[2]).not.toContain('<img');
    expect(languageFor('app.rs')).toBe('rust');
    expect(languageFor('README')).toBe('plaintext');
  });
  it('selects literal files without wildcard expansion', () => {
    expect(ignorePattern('notes/[draft]*.txt', false)).toBe(
      '/notes/\\[draft\\]\\*.txt',
    );
    expect(ignorePattern('private notes', true)).toBe('/private\\ notes/');
  });
});
