import hljs from 'highlight.js/lib/core';
import javascript from 'highlight.js/lib/languages/javascript';
import typescript from 'highlight.js/lib/languages/typescript';
import python from 'highlight.js/lib/languages/python';
import rust from 'highlight.js/lib/languages/rust';
import json from 'highlight.js/lib/languages/json';
import css from 'highlight.js/lib/languages/css';
import xml from 'highlight.js/lib/languages/xml';
import bash from 'highlight.js/lib/languages/bash';
import powershell from 'highlight.js/lib/languages/powershell';
import markdown from 'highlight.js/lib/languages/markdown';
import yaml from 'highlight.js/lib/languages/yaml';
import cpp from 'highlight.js/lib/languages/cpp';
import csharp from 'highlight.js/lib/languages/csharp';
import go from 'highlight.js/lib/languages/go';
import java from 'highlight.js/lib/languages/java';
import lua from 'highlight.js/lib/languages/lua';
import sql from 'highlight.js/lib/languages/sql';
import ini from 'highlight.js/lib/languages/ini';
import { diffLines } from 'diff';

for (const [name, grammar] of Object.entries({
  javascript,
  typescript,
  python,
  rust,
  json,
  css,
  xml,
  bash,
  powershell,
  markdown,
  yaml,
  cpp,
  csharp,
  go,
  java,
  lua,
  sql,
  ini,
}))
  hljs.registerLanguage(name, grammar);
const languages: Record<string, string> = {
  js: 'javascript',
  jsx: 'javascript',
  mjs: 'javascript',
  cjs: 'javascript',
  ts: 'typescript',
  tsx: 'typescript',
  py: 'python',
  rs: 'rust',
  json: 'json',
  css: 'css',
  scss: 'css',
  html: 'xml',
  svg: 'xml',
  xml: 'xml',
  svelte: 'xml',
  vue: 'xml',
  sh: 'bash',
  bash: 'bash',
  ps1: 'powershell',
  md: 'markdown',
  yml: 'yaml',
  yaml: 'yaml',
  c: 'cpp',
  h: 'cpp',
  cpp: 'cpp',
  hpp: 'cpp',
  cs: 'csharp',
  go: 'go',
  java: 'java',
  lua: 'lua',
  luau: 'lua',
  sql: 'sql',
  toml: 'ini',
  ini: 'ini',
};
export const languageFor = (path: string) =>
  languages[path.split('.').pop()?.toLowerCase() ?? ''] ?? 'plaintext';
const escape = (text: string) =>
  text
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
// Keep multiline token scopes across rows while returning balanced, escaped HTML.
export function highlightLines(text: string, language: string): string[] {
  const html =
    language === 'plaintext'
      ? escape(text)
      : hljs.highlight(text, { language, ignoreIllegals: true }).value;
  const active: string[] = [];
  return html.split('\n').map((line) => {
    const prefix = active.join('');
    for (const match of line.matchAll(/<span\b[^>]*>|<\/span>/g)) {
      if (match[0] === '</span>') active.pop();
      else active.push(match[0]);
    }
    return prefix + line + '</span>'.repeat(active.length);
  });
}
export type ComparisonRow = {
  local: number | null;
  cloud: number | null;
  changed: boolean;
};
export function comparisonRows(cloud: string, local: string): ComparisonRow[] {
  let cloudLine = 0,
    localLine = 0;
  const rows: ComparisonRow[] = [];
  const changes = diffLines(cloud, local, {
    timeout: 1000,
    maxEditLength: 2000,
  });
  if (!changes) {
    const count = (s: string) => (s === '' ? 0 : s.split('\n').length);
    for (let i = 0; i < Math.max(count(cloud), count(local)); i++)
      rows.push({
        local: i < count(local) ? i : null,
        cloud: i < count(cloud) ? i : null,
        changed: true,
      });
    return rows;
  }
  for (let i = 0; i < changes.length; i++) {
    const chunk = changes[i];
    if (chunk.removed && changes[i + 1]?.added) {
      const next = changes[++i];
      for (let n = 0; n < Math.max(chunk.count!, next.count!); n++)
        rows.push({
          cloud: n < chunk.count! ? cloudLine++ : null,
          local: n < next.count! ? localLine++ : null,
          changed: true,
        });
    } else {
      for (let n = 0; n < chunk.count!; n++)
        rows.push({
          cloud: chunk.added ? null : cloudLine++,
          local: chunk.removed ? null : localLine++,
          changed: !!chunk.added || !!chunk.removed,
        });
    }
  }
  return rows;
}
export function ignorePattern(path: string, directory: boolean): string {
  // Root the path and escape Git wildcards so selecting a file ignores that file.
  return '/' + path.replace(/[\\*?\[\]#! ]/g, '\\$&') + (directory ? '/' : '');
}
