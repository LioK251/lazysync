export const GITHUB_FILE_LIMIT = 100 * 1024 * 1024;

export function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null || !Number.isFinite(bytes) || bytes < 0) return '—';
  if (bytes < 1024) return `${bytes} B`;
  const units = ['KiB', 'MiB', 'GiB', 'TiB', 'PiB'];
  const index = Math.min(
    Math.floor(Math.log2(bytes) / 10) - 1,
    units.length - 1,
  );
  const value = bytes / 1024 ** (index + 1);
  return `${Number(value.toFixed(1))} ${units[index]}`;
}

export function sizeTitle(bytes: number | null | undefined): string {
  return bytes == null ? 'Size unavailable' : `${bytes.toLocaleString()} bytes`;
}

export function exceedsLimit(bytes: number | null | undefined): boolean {
  return bytes != null && bytes > GITHUB_FILE_LIMIT;
}
