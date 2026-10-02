import { describe, it, expect } from 'vitest';
import { relative, tone, acceptSnapshot } from './format';
import type { StatusSnapshot } from './models';
const snapshot: StatusSnapshot = {
  version: 1,
  sequence: 1,
  repositoryId: '1',
  phase: 'idle',
  label: 'Synced',
  connectivity: 'online',
  ahead: 0,
  behind: 0,
  changes: { added: 0, modified: 0, deleted: 0 },
  lastSync: null,
  recovery: false,
  error: null,
};
describe('status snapshots', () => {
  it('rejects stale versions, repositories and sequence numbers', () => {
    expect(acceptSnapshot(snapshot, { ...snapshot, sequence: 0 }, '1')).toBe(
      false,
    );
    expect(
      acceptSnapshot(snapshot, { ...snapshot, repositoryId: '2' }, '1'),
    ).toBe(false);
    expect(acceptSnapshot(snapshot, { ...snapshot, version: 2 }, '1')).toBe(
      false,
    );
    expect(acceptSnapshot(snapshot, { ...snapshot, sequence: 2 }, '1')).toBe(
      true,
    );
  });
  it('communicates attention with labeled color state', () => {
    expect(tone(snapshot)).toBe('green');
    expect(tone({ ...snapshot, ahead: 1 })).toBe('amber');
    expect(tone({ ...snapshot, phase: 'paused' })).toBe('red');
  });
  it('formats last success including absent/future timestamps', () => {
    expect(relative(null)).toBe('Not synced yet');
    expect(relative(new Date(100000).toISOString(), 100001)).toBe('Just now');
    expect(relative(new Date(100000).toISOString(), 220000)).toBe('2 min ago');
  });
});
