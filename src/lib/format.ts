import type { StatusSnapshot } from './models';
export function tone(s: StatusSnapshot): 'green' | 'amber' | 'red' {
  if (s.error || s.phase === 'paused') return 'red';
  if (
    s.phase !== 'idle' ||
    s.ahead +
      s.behind +
      s.changes.added +
      s.changes.modified +
      s.changes.deleted >
      0
  )
    return 'amber';
  return 'green';
}
export function relative(value: string | null, now = Date.now()): string {
  if (!value) return 'Not synced yet';
  const minutes = Math.max(
    0,
    Math.floor((now - new Date(value).getTime()) / 60000),
  );
  return minutes === 0
    ? 'Just now'
    : minutes < 60
      ? `${minutes} min ago`
      : minutes < 1440
        ? `${Math.floor(minutes / 60)} hr ago`
        : `${Math.floor(minutes / 1440)} days ago`;
}
export function acceptSnapshot(
  current: StatusSnapshot | null,
  next: StatusSnapshot,
  activeId: string | null,
): boolean {
  return (
    next.version === 1 &&
    next.repositoryId === activeId &&
    (!current || next.sequence >= current.sequence)
  );
}
