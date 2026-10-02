import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import type {
  AppError,
  Automation,
  Settings,
  RemoteRepository,
  StatusSnapshot,
  Commit,
  FileDiff,
  Conflict,
  Identity,
} from './models';
type Commands = {
  get_settings: [undefined, Settings];
  get_status: [undefined, StatusSnapshot];
  authenticate: [{ token: string }, Identity];
  save_settings: [
    {
      deviceName: string;
      commitName: string;
      commitEmail: string;
      automation: Automation;
    },
    Settings,
  ];
  list_repositories: [{ page: number }, RemoteRepository[]];
  connect_repository: [{ remote: RemoteRepository; folder: string }, Settings];
  clone_repository: [{ remote: RemoteRepository; folder: string }, Settings];
  select_repository: [{ id: string }, Settings];
  create_repository: [
    { name: string; description: string; folder: string },
    Settings,
  ];
  abandon_setup: [undefined, Settings];
  reconfirm_branch: [undefined, Settings];
  sync_now: [undefined, StatusSnapshot];
  continue_sync: [undefined, StatusSnapshot];
  get_history: [{ page: number }, Commit[]];
  get_diffs: [{ oid: string | null }, FileDiff[]];
  get_conflicts: [undefined, Conflict[]];
  resolve_conflict: [
    { path: string; choice: 'local' | 'remote' | 'edited' },
    Conflict[],
  ];
  finish_recovery: [undefined, StatusSnapshot];
  open_conflict_editor: [{ path: string }, void];
  set_dialog_open: [{ open: boolean }, void];
  quit: [undefined, void];
};
export const preview =
  import.meta.env.DEV && new URLSearchParams(location.search).has('preview');
export async function call<K extends keyof Commands>(
  command: K,
  ...args: Commands[K][0] extends undefined ? [] : [Commands[K][0]]
): Promise<Commands[K][1]> {
  if (preview) {
    const demo = await import('./preview');
    return (await demo.callPreview(command, args[0])) as Commands[K][1];
  }
  if (!isTauri())
    throw {
      code: 'desktopRequired',
      message: 'Open lazysync as a desktop app.',
      action: 'Run npm run tauri dev to connect repositories and sync.',
    } satisfies AppError;
  return invoke<Commands[K][1]>(command, args[0]);
}
export async function onStatus(
  callback: (s: StatusSnapshot) => void,
): Promise<() => void> {
  return isTauri()
    ? listen<StatusSnapshot>('lazysync:status:v1', (e) => callback(e.payload))
    : () => {};
}
export async function chooseFolder(): Promise<string | null> {
  if (!isTauri()) return null;
  return open({
    directory: true,
    multiple: false,
    title: 'Choose repository folder',
  });
}
export async function hide(): Promise<void> {
  if (isTauri()) {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().hide();
  }
}
export function errorOf(e: unknown): AppError {
  if (typeof e === 'object' && e && 'message' in e && 'action' in e)
    return e as AppError;
  return {
    code: 'unexpected',
    message: 'The operation could not finish.',
    action: 'Retry or review your repository in a Git client.',
  };
}
