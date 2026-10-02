// Development-only fixtures; the release build never routes commands here.
import type { Settings, StatusSnapshot, RemoteRepository } from './models';
const remote: RemoteRepository = {
  id: '1',
  fullName: 'morgan/field-notes',
  owner: 'morgan',
  private: true,
  writable: true,
  defaultBranch: 'main',
};
let settings: Settings = {
  version: 1,
  repositories: [
    {
      remote,
      folder: 'C:\\Projects\\field-notes',
      branch: 'main',
      lastSync: new Date(Date.now() - 240000).toISOString(),
    },
  ],
  activeId: '1',
  deviceName: 'Studio PC',
  commitName: 'Morgan',
  commitEmail: 'morgan@users.noreply.github.com',
  automation: 'off',
  statusIntervalSecs: 10,
  fetchIntervalSecs: 60,
  pendingSetup: null,
};
let status: StatusSnapshot = {
  version: 1,
  sequence: 1,
  repositoryId: '1',
  phase: 'idle',
  label: 'Ready to sync',
  connectivity: 'online',
  ahead: 1,
  behind: 0,
  changes: { added: 2, modified: 3, deleted: 1 },
  lastSync: settings.repositories[0].lastSync,
  recovery: false,
  error: null,
};
export async function callPreview(
  command: string,
  arg: unknown,
): Promise<unknown> {
  const args = arg as Record<string, unknown> | undefined;
  switch (command) {
    case 'get_settings':
      return structuredClone(settings);
    case 'get_status':
      return structuredClone(status);
    case 'list_repositories':
      return args?.page === 1
        ? [
            remote,
            { ...remote, id: '2', fullName: 'morgan/dotfiles' },
            {
              ...remote,
              id: '3',
              fullName: 'team/design-system',
              owner: 'team',
              writable: false,
            },
          ]
        : [];
    case 'get_history':
      return args?.page === 0
        ? [
            {
              oid: 'a18cf73213ee654fed3105625a70be1233456789',
              summary: 'Sync from Studio PC - 2026-10-02 18:42',
              author: 'Morgan',
              timestamp: new Date().toISOString(),
            },
            {
              oid: 'b724fc8213ee654fed3105625a70be1233456789',
              summary: 'Add research notes',
              author: 'Morgan',
              timestamp: new Date(Date.now() - 3600000).toISOString(),
            },
          ]
        : [];
    case 'get_diffs':
      return [
        {
          path: 'notes/ideas.md',
          binary: false,
          truncated: false,
          patch:
            'diff --git a/notes/ideas.md b/notes/ideas.md\n--- a/notes/ideas.md\n+++ b/notes/ideas.md\n@@ -1 +1,2 @@\n # Field notes\n+ Make room for the next idea.',
        },
        { path: 'cover.png', binary: true, truncated: false, patch: '' },
      ];
    case 'get_conflicts':
      return [];
    case 'save_settings':
      settings = { ...settings, ...args };
      return structuredClone(settings);
    case 'sync_now':
      await new Promise((r) => setTimeout(r, 300));
      status = {
        ...status,
        sequence: status.sequence + 1,
        label: 'All changes synced',
        ahead: 0,
        changes: { added: 0, modified: 0, deleted: 0 },
        lastSync: new Date().toISOString(),
      };
      return structuredClone(status);
    case 'set_dialog_open':
    case 'quit':
      return;
    default:
      throw {
        code: 'preview',
        message: 'This is a development preview.',
        action:
          'Use the native app to authenticate, connect, and create repositories.',
      };
  }
}
